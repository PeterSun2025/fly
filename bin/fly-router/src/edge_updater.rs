// 引入自定义模块中的 Edge 结构体
use crate::edge::Edge;
// 引入自定义的指标模块
use crate::metrics;
// 引入自定义的代币缓存模块
use crate::source::token_cache::TokenCache;
// 引入自定义的工具函数，用于生成 Tokio 任务
use crate::util::tokio_spawn;
// 引入锚点库中的 SPL 代币模块
use anchor_spl::token::spl_token;
// 引入 itertools 库，用于提供更便捷的迭代器方法
use itertools::Itertools;
// 引入路由配置库中的 Config 结构体
use router_config_lib::Config;
// 引入路由数据馈送库中的获取程序账户模块的 FeedMetadata 结构体
use router_feed_lib::get_program_account::FeedMetadata;
// 引入路由库中的 DEX 相关模块
use router_lib::dex::{AccountProviderView, DexSubscriptionMode};
// 引入路由库中的价格馈送模块的价格缓存结构体
use router_lib::price_feeds::price_cache::PriceCache;
// 引入路由库中的价格馈送模块的价格更新结构体
use router_lib::price_feeds::price_feed::PriceUpdate;
// 引入 Solana 程序库中的公钥类型
use solana_program::pubkey::Pubkey;
// 引入标准库中的集合类型
use std::collections::{HashMap, HashSet};
// 引入标准库中的原子引用计数类型
use std::sync::Arc;
// 引入标准库中的时间模块
use std::time::{Duration, Instant};
// 引入 Tokio 异步运行时的广播通道模块
use tokio::sync::broadcast;
// 引入 Tokio 广播通道的接收错误类型
use tokio::sync::broadcast::error::RecvError;
// 引入 Tokio 任务的句柄类型
use tokio::task::JoinHandle;
// 引入日志跟踪模块
use tracing::{debug, error, info, warn};

// 定义一个可克隆的 DEX 结构体
#[derive(Clone)]
pub struct Dex {
    // DEX 的名称
    pub name: String,
    // 按订阅的公钥引用边，以便在账户更改时更新它们
    /// reference edges by the subscribed_pks so they can be updated on account change
    pub edges_per_pk: HashMap<Pubkey, Vec<Arc<Edge>>>,
    // 订阅模式，防止程序账户过多导致 RPC 订阅过载，可直接订阅程序 ID
    /// in case the program has too many accounts it could overload the rpc subscription
    /// it can be easier to subscribe to the program id directly
    pub subscription_mode: DexSubscriptionMode,
}

// 为 Dex 结构体实现方法
impl Dex {
    // 获取 DEX 中的所有边
    pub fn edges(&self) -> Vec<Arc<Edge>> {
        // 从 edges_per_pk 中提取所有边，排序并去重
        let edges: Vec<Arc<Edge>> = self
           .edges_per_pk
           .clone()
           .into_iter()
           .map(|x| x.1)
           .flatten()
           .sorted_by_key(|x| x.unique_id())
           .unique_by(|x| x.unique_id())
           .collect();
        edges
    }
}

// 定义边更新器的状态结构体，默认实现
#[derive(Default)]
struct EdgeUpdaterState {
    // 是否准备好
    pub is_ready: bool,
    // 最新待处理的槽位
    pub latest_slot_pending: u64,
    // 最新已处理的槽位
    pub latest_slot_processed: u64,
    // 槽位过度滞后开始的时间
    pub slot_excessive_lagging_since: Option<Instant>,
    // 价格是否需要更新
    pub dirty_prices: bool,
    // 已接收的账户公钥集合
    pub received_account: HashSet<Pubkey>,
    // 需要更新的程序公钥集合
    pub dirty_programs: HashSet<Pubkey>,
    // 所有者的代币账户是否需要更新
    pub dirty_token_accounts_for_owners: bool,
    // 需要更新的边的映射
    pub dirty_edges: HashMap<(Pubkey, Pubkey), Arc<Edge>>,
    // 按铸币公钥分类的边的映射
    pub edges_per_mint: HashMap<Pubkey, Vec<Arc<Edge>>>,
}

// 定义边更新器结构体
struct EdgeUpdater {
    // DEX 实例
    dex: Dex,
    // 链上数据视图
    chain_data: AccountProviderView,
    // 代币缓存
    token_cache: Arc<TokenCache>,
    // 价格缓存
    price_cache: PriceCache,
    // 准备就绪信号发送器
    ready_sender: async_channel::Sender<()>,
    // 注册铸币信号发送器
    register_mint_sender: async_channel::Sender<Pubkey>,
    // 边更新器的状态
    state: EdgeUpdaterState,
    // 配置信息
    config: Config,
    // 路径预热金额列表
    path_warming_amounts: Vec<u64>,

    edge_price_sender: async_channel::Sender<Arc<Edge>>,
}

// 启动边更新器任务
pub fn spawn_updater_job(
    dex: &Dex,
    config: &Config,
    chain_data: AccountProviderView,
    token_cache: Arc<TokenCache>,
    price_cache: PriceCache,
    path_warming_amounts: Vec<u64>,
    register_mint_sender: async_channel::Sender<Pubkey>,
    ready_sender: async_channel::Sender<()>,
    mut slot_updates: broadcast::Receiver<u64>,
    mut account_updates: broadcast::Receiver<(Pubkey, Pubkey, u64)>,
    mut metadata_updates: broadcast::Receiver<FeedMetadata>,
    mut price_updates: broadcast::Receiver<PriceUpdate>,
    mut exit: broadcast::Receiver<()>,
    edge_price_sender: async_channel::Sender<Arc<Edge>>,
) -> Option<JoinHandle<()>> {
    // 克隆 DEX 实例
    let dex = dex.clone();
    // 克隆配置信息
    let config = config.clone();
    // 获取 DEX 中的所有边
    let edges = dex.edges();

    // 初始化按铸币公钥分类的边的映射
    let mut edges_per_mint = HashMap::<Pubkey, Vec<Arc<Edge>>>::default();
    for edge in &edges {
        // 将边添加到输入铸币公钥对应的列表中
        edges_per_mint
           .entry(edge.input_mint)
           .or_default()
           .push(edge.clone());
        // 将边添加到输出铸币公钥对应的列表中
        edges_per_mint
           .entry(edge.output_mint)
           .or_default()
           .push(edge.clone());
    }

    // 根据订阅模式记录日志
    match &dex.subscription_mode {
        DexSubscriptionMode::Accounts(x) => info!(
            dex_name = dex.name,
            accounts_count = x.len(),
            "subscribing to accounts"
        ),
        DexSubscriptionMode::Programs(x) => info!(
            dex_name = dex.name,
            programs = x.iter().map(|p| p.to_string()).join(", "),
            "subscribing to programs"
        ),
        DexSubscriptionMode::Disabled => {
            debug!(dex_name = dex.name, "disabled");
            // 发送准备就绪信号
            let _ = ready_sender.try_send(());
            return None;
        }
        DexSubscriptionMode::Mixed(m) => info!(
            dex_name = dex.name,
            accounts_count = m.accounts.len(),
            programs = m.programs.iter().map(|p| p.to_string()).join(", "),
            token_accounts_for_owner = m
               .token_accounts_for_owner
               .iter()
               .map(|p| p.to_string())
               .join(", "),
            "subscribing to mixed mode"
        ),
    };

    // 获取初始化超时时间，默认为 5 分钟
    let init_timeout_in_seconds = config.snapshot_timeout_in_seconds.unwrap_or(60 * 30);  //暂时修改为30分钟
    // 计算初始化超时时刻
    let init_timeout = Instant::now() + Duration::from_secs(init_timeout_in_seconds);
    // 生成 Tokio 任务
    let listener_job = tokio_spawn(format!("edge_updater_{}", dex.name).as_str(), async move {
        // 创建边更新器实例
        let mut updater = EdgeUpdater {
            dex,
            chain_data,
            token_cache,
            price_cache,
            register_mint_sender,
            ready_sender,
            config,
            state: EdgeUpdaterState {
                edges_per_mint,
                ..EdgeUpdaterState::default()
            },
            path_warming_amounts,
            edge_price_sender,
        };

        // 初始化刷新间隔
        let mut refresh_one_interval = tokio::time::interval(Duration::from_millis(10));
        // 等待第一次刷新间隔
        refresh_one_interval.tick().await;

        // 主循环，处理各种更新事件
        'drain_loop: loop {
            tokio::select! {
                // 处理退出信号
                _ = exit.recv() => {
                    info!("shutting down {} update task", updater.dex.name);
                    break;
                }
                // 处理槽位更新
                slot = slot_updates.recv() => {
                    debug!("{} - slot update {:?}", updater.dex.name, slot);
                    updater.detect_and_handle_slot_lag(slot);
                }
                // 处理元数据更新
                res = metadata_updates.recv() => {
                    debug!("{} - metadata update {:?}", updater.dex.name, res);
                    // 处理元数据更新
                    updater.on_metadata_update(res);
                }
                // 处理账户更新
                res = account_updates.recv() => {
                    if !updater.invalidate_one(res) {
                        break 'drain_loop;
                    }
                    // 批量处理账户更新的代码注释掉了
                    // let mut batchsize: u32 = 0;
                    // let started_at = Instant::now();
                    // 'batch_loop: while let Ok(res) = account_updates.try_recv() {
                    //     batchsize += 1;
                    //     if !updater.invalidate_one(Ok(res)) {
                    //         break 'drain_loop;
                    //     }

                    //     // budget for microbatch
                    //     if batchsize > 10 || started_at.elapsed() > Duration::from_micros(500) {
                    //         break 'batch_loop;
                    //     }
                    // }
                },
                // 处理价格更新
                Ok(price_upd) = price_updates.recv() => {
                    if let Some(impacted_edges) = updater.state.edges_per_mint.get(&price_upd.mint) {
                        for edge in impacted_edges {
                            updater.state.dirty_edges.insert(edge.unique_id(), edge.clone());
                        }
                    };
                },
                // 处理刷新间隔事件
                _ = refresh_one_interval.tick() => {
                    if !updater.state.is_ready && init_timeout < Instant::now() {
                        error!("Failed to init '{}' before timeout", updater.dex.name);
                        break;
                    }

                    updater.refresh_some();
                }
            }
        }

        error!("Edge updater {} job exited..", updater.dex.name);
        // 发送准备就绪信号，解除退出处理程序前的阻塞
        // send this to unblock the code in front of the exit handler
        let _ = updater.ready_sender.try_send(());
    });

    Some(listener_job)
}

// 为 EdgeUpdater 结构体实现方法
impl EdgeUpdater {
    // 检测并处理槽位滞后问题
    fn detect_and_handle_slot_lag(&mut self, slot: Result<u64, RecvError>) {
        let state = &mut self.state;
        if state.latest_slot_processed == 0 {
            return;
        }
        if let Ok(slot) = slot {
            // 计算槽位滞后值
            let lag = slot as i64 - state.latest_slot_processed as i64;
            if lag <= 0 {
                return;
            }
            // 记录槽位相关指标
            debug!(
                state.latest_slot_processed,
                state.latest_slot_pending, slot, lag, self.dex.name, "metrics"
            );

            // 更新指标数据
            metrics::GRPC_TO_EDGE_SLOT_LAG
               .with_label_values(&[&self.dex.name])
               .set(lag);

            // 获取最大允许的槽位滞后值
            let max_lag = self.config.routing.slot_excessive_lag.unwrap_or(300);
            // 获取最大允许的槽位滞后持续时间
            let max_lag_duration = Duration::from_secs(
                self.config
                   .routing
                   .slot_excessive_lag_max_duration_secs
                   .unwrap_or(60),
            );

            if lag as u64 >= max_lag {
                match state.slot_excessive_lagging_since {
                    None => state.slot_excessive_lagging_since = Some(Instant::now()),
                    Some(since) => {
                        if since.elapsed() > max_lag_duration {
                            panic!(
                                "Lagging a lot {} for more than {}s, for dex {}..",
                                lag,
                                max_lag_duration.as_secs(),
                                self.dex.name,
                            );
                        }
                    }
                }
                return;
            } else if state.slot_excessive_lagging_since.is_some() {
                state.slot_excessive_lagging_since = None;
            }
        }
    }

    // 启动后调用一次，处理准备就绪事件
    // called once after startup
    #[tracing::instrument(skip_all, level = "trace")]
    fn on_ready(&self) {
        let mut mints = HashSet::new();
        for edge in self.dex.edges() {
            // 收集所有边的输入和输出铸币公钥
            mints.insert(edge.input_mint);
            mints.insert(edge.output_mint);
        }

        info!(
            "Received all accounts needed for {} [mints count={}]",
            self.dex.name,
            mints.len()
        );

        for mint in mints {
            // 尝试发送注册铸币信号
            match self.register_mint_sender.try_send(mint) {
                Ok(_) => {}
                Err(_) => warn!("Failed to register mint '{}' for price update", mint),
            }
        }

        // 发送准备就绪信号
        let _ = self.ready_sender.try_send(());
    }

    // 处理元数据更新事件
    fn on_metadata_update(&mut self, res: Result<FeedMetadata, RecvError>) {
        let state = &mut self.state;
        match res {
            Ok(v) => match v {
                FeedMetadata::InvalidAccount(key) => {
                    state.received_account.insert(key);
                    self.check_readiness();
                }
                FeedMetadata::SnapshotStart(_) => {}
                FeedMetadata::SnapshotEnd(x) => {
                    if let Some(x) = x {
                        if x == spl_token::ID {
                            // 处理代币账户所有者的更新
                            // TODO Handle multiples owners
                            state.dirty_token_accounts_for_owners = true;
                        } else {
                            state.dirty_programs.insert(x);
                        }
                        self.check_readiness();
                    }
                }
            },
            Err(e) => {
                warn!(
                    "Error on metadata update channel in {} update task {:?}",
                    self.dex.name, e
                );
            }
        }
    }

    // 处理单个账户更新事件
    fn invalidate_one(&mut self, res: Result<(Pubkey, Pubkey, u64), RecvError>) -> bool {
        let (pk, owner, slot) = match res {
            Ok(v) => v,
            Err(broadcast::error::RecvError::Closed) => {
                error!("account update channel closed unexpectedly");
                return false;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(
                    "lagged {n} on account update channel in {} update task",
                    self.dex.name
                );
                return true;
            }
        };

        // 检查是否需要更新
        //check if we need the update
        if !self.do_update(&pk, &owner) {
            return true;
        }
        let state = &mut self.state;
        if let Some(impacted_edges) = self.dex.edges_per_pk.get(&pk) {
            for edge in impacted_edges {
                state.dirty_edges.insert(edge.unique_id(), edge.clone());
            }
        };

        state.received_account.insert(pk);
        if state.latest_slot_pending < slot {
            state.latest_slot_pending = slot;
        }

        self.check_readiness();

        return true;
    }

    // 检查是否准备就绪
    fn check_readiness(&mut self) {
        let state = &mut self.state;

        if state.is_ready {
            return;
        }

        match &self.dex.subscription_mode {
            DexSubscriptionMode::Accounts(accounts) => {
                state.is_ready = state.received_account.is_superset(&accounts);
            }
            DexSubscriptionMode::Disabled => {}
            DexSubscriptionMode::Programs(programs) => {
                state.is_ready = state.dirty_programs.is_superset(&programs);
            }
            DexSubscriptionMode::Mixed(m) => {
                state.is_ready = state.received_account.is_superset(&m.accounts)
                    && state.dirty_programs.is_superset(&m.programs)
                    && (state.dirty_token_accounts_for_owners
                        || m.token_accounts_for_owner.is_empty());
            }
        }

        if state.is_ready {
            self.on_ready();
        }
    }

    // 判断是否需要更新
    // ignore update if current dex does not need it
    fn do_update(&mut self, pk: &Pubkey, owner: &Pubkey) -> bool {
        if self.dex.edges_per_pk.contains_key(pk) {
            return true;
        }
        match &self.dex.subscription_mode {
            DexSubscriptionMode::Accounts(accounts) => return accounts.contains(pk),
            DexSubscriptionMode::Disabled => false,
            DexSubscriptionMode::Programs(programs) => {
                programs.contains(pk) || programs.contains(owner)
            }
            DexSubscriptionMode::Mixed(m) => {
                m.accounts.contains(pk)
                    || m.token_accounts_for_owner.contains(pk)
                    || m.programs.contains(pk)
                    || m.programs.contains(owner)
            }
        }
    }

    // 刷新部分数据
    fn refresh_some(&mut self) {
        let state = &mut self.state;
        if state.dirty_edges.is_empty() || !state.is_ready {
            return;
        }

        let started_at = Instant::now();
        let mut refreshed_edges = vec![];

        for (unique_id, edge) in &state.dirty_edges {
            // 更新边的数据
            edge.update(
                &self.chain_data,
                &self.token_cache,
                &self.price_cache,
                &self.path_warming_amounts,
            );
            refreshed_edges.push(unique_id.clone());

            // 避免处理时间过长
            // Do not process for too long or we could miss update in account write queue
            if started_at.elapsed() > Duration::from_millis(100) {
                warn!(
                    "{} - refresh {} - took - {:?} - too long",
                    self.dex.name,
                    refreshed_edges.len(),
                    started_at.elapsed()
                );
                break;
            }
        }
        for unique_id in &refreshed_edges {
            if let Some(edge) = state.dirty_edges.get(&unique_id) {
                //let _ = self.edge_price_sender.send(edge.clone());
                match self.edge_price_sender.try_send(edge.clone()) {
                    Ok(()) => {
                        debug!("edge send success {:?}", edge.unique_id());
                    }
                    Err(err) => {
                        error!("edge send error {:?} failed to send message: {:?}", edge.unique_id(), err);
                    }
                }
            }
            
            state.dirty_edges.remove(&unique_id);
        }

        state.latest_slot_processed = state.latest_slot_pending;

        // if started_at.elapsed() > Duration::from_millis(100) {
        //     // debug!(
        //     //     "{} - refresh {} - took - {:?}",
        //     //     self.dex.name,
        //     //     refreshed_edges.len(),
        //     //     started_at.elapsed()
        //     // )
        //     info!(
        //         "{} - refresh {} - took - {:?}",
        //         self.dex.name,
        //         refreshed_edges.len(),
        //         started_at.elapsed()
        //     )
        // }
    }
}