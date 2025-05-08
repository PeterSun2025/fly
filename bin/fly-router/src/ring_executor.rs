use crate::graph::Graph;
use crate::ring;
use crate::ring_executor;
use crate::routing_types::Route;
use crate::source::token_cache::TokenCache;
use dashmap::DashMap;
use futures::future::join_all;
use rayon::prelude::*;
use router_lib::dex::AccountProviderView;
use std::collections::HashMap;
use std::collections::HashSet;
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};
// 引入自定义的预导入模块，包含常用的类型和特性
use crate::ring::Ring;
use crate::util::tokio_spawn;
use crate::{edge, prelude::*};

use router_config_lib::Config;

const MAX_PARALLEL_HEAVY_RING_REFRESH_SOME: usize = 16;

#[derive(Error, Debug)]
pub enum RingingError {
    #[error("unsupported input mint {0:?}")]
    UnsupportedInputMint(Pubkey),
    #[error("unsupported output mint {0:?}")]
    UnsupportedOutputMint(Pubkey),
    #[error("no path between {0:?} and {1:?}")]
    NoPathBetweenMintPair(Pubkey, Pubkey),
    #[error("could not compute out amount")]
    CouldNotComputeOut,
}

#[derive(Default)]
struct RingExecutorState {
    // 是否准备好
    pub is_ready: bool,

    dirty_rings: DashMap<String, Arc<Ring>>, //使用DashMap,并发处理
}

pub struct RingExecutor {
    // 准备就绪信号发送器
    // ready_sender: async_channel::Sender<()>,
    chain_data: AccountProviderView,

    token_cache: Arc<TokenCache>,

    trading_mint_rings: HashMap<Pubkey, Vec<Arc<Ring>>>,

    edge_rings: HashMap<(Pubkey, Pubkey), HashMap<String, Arc<Ring>>>,

    path_warming_amounts: Vec<u64>,

    in_amounts: Vec<u64>,

    expected_gain: u64,

    max_path_length: usize,

    graph: Graph,

    route_sender: async_channel::Sender<Arc<Route>>,

    state: RingExecutorState,
}

impl RingExecutor {
    pub fn new(
        config: &Config,
        chain_data: AccountProviderView,
        token_cache: Arc<TokenCache>,
        // ready_sender: async_channel::Sender<()>,
        //edge_price_updates: broadcast::Receiver<Arc<Edge>>,
        path_warming_amounts: Vec<u64>,
        edges: Vec<Arc<Edge>>,
        route_sender: async_channel::Sender<Arc<Route>>,
    ) -> Self {
        let max_path_length: usize = config.ring.max_path_length.unwrap_or(3);
        let mut trading_mints: Vec<String> = config.ring.trading_mints.clone().unwrap_or_default();
        if trading_mints.is_empty() {
            trading_mints.push("So11111111111111111111111111111111111111112".to_string());
        }

        let mut in_amounts: Vec<u64> = config
            .sender
            .in_amounts
            .clone()
            .unwrap_or([10_00_000_000, 500_000_000, 100_000_000].to_vec());
        in_amounts.sort_by(|a, b| b.cmp(a));
        let expected_gain: u64 = config.sender.expected_gain.unwrap_or(1_000_000);

        let mut ring_mint_rings: HashMap<Pubkey, Vec<Arc<Ring>>> = HashMap::new();
        let mut edge_rings: HashMap<(Pubkey, Pubkey), HashMap<String, Arc<Ring>>> = HashMap::new();
        let mut graph = Graph::new();
        graph.add_edges(edges.clone());
        for trading_mint_string in trading_mints.iter() {
            let ring_mint = Pubkey::from_str(trading_mint_string)
                .unwrap_or_else(|_| panic!("Invalid mint address: {}", trading_mint_string));
            let cycles: Vec<Vec<Arc<Edge>>> = graph.find_cycles(ring_mint, max_path_length);

            info!(
                "ring trading mint {} have {} rings",
                trading_mint_string,
                cycles.len(),
            );
            for cycle in cycles.iter() {
                let mut ring_ming_symbols: HashSet<String> = HashSet::new();
                for edge in cycle.iter() {
                    let input_mint = edge.input_mint;
                    let output_mint = edge.output_mint;
                    if let Some(input_symbol) = token_cache.get_symbol_by_mint(input_mint) {
                        ring_ming_symbols.insert(input_symbol);
                    }
                    if let Some(output_symbol) = token_cache.get_symbol_by_mint(output_mint) {
                        ring_ming_symbols.insert(output_symbol);
                    }
                }
                let ring = Arc::new(Ring::new(ring_mint, cycle.clone(), ring_ming_symbols));
                let mut ring_state = ring.ring_state.write().unwrap();
                ring_state.set_valid(true);
                ring_state.reset_cooldown();
                ring_mint_rings
                    .entry(ring_mint)
                    .or_default()
                    .push(ring.clone());
                for edge in cycle.iter() {
                    let unique_id = edge.unique_id();
                    edge_rings
                        .entry(unique_id)
                        .or_default()
                        .insert(ring.get_ring_id(), ring.clone());
                }
            }
        }

        Self {
            // ready_sender,
            chain_data,
            token_cache,
            trading_mint_rings: ring_mint_rings,
            edge_rings,
            //dirty_rings: HashMap::new(),
            //edge_price_updates,
            path_warming_amounts,
            in_amounts,
            expected_gain,
            max_path_length,
            graph,
            route_sender,
            state: RingExecutorState::default(),
        }
    }

    pub fn do_dirty_ring(&mut self, edge: Arc<Edge>) {
        let unique_id = edge.unique_id();

        if let Some(ring_map) = self.edge_rings.get(&unique_id) {
            debug!(
                "do_dirty_ring : dirty edge: {:?}, dirty ring num {} ",
                unique_id,
                ring_map.len()
            );

            for (ring_id, ring) in ring_map {
                // 分离读锁和写锁的作用域
                let should_insert = {
                    let ring_state = ring.ring_state.read().unwrap();
                    ring_state.is_valid()
                };

                if should_insert {
                    self.state
                        .dirty_rings
                        .entry(ring_id.clone())
                        .or_insert(ring.clone());
                } else {
                    let should_reset = {
                        let ring_state = ring.ring_state.read().unwrap();
                        ring_state.can_reset_cooldown()
                    };

                    if should_reset {
                        if let Ok(mut ring_state) = ring.ring_state.write() {
                            ring_state.reset_cooldown();
                            debug!("Reset cooldown for ring_id: {}", ring_id);
                        }
                    }
                }
            }
        } else {
            debug!("No ring found for edge: {:?}", unique_id);
        }
    }

    async fn refresh_some(&mut self) {
        if self.state.dirty_rings.is_empty() || !self.state.is_ready {
            return;
        }

        let started_at = Instant::now();
        let dirty_rings_len = self.state.dirty_rings.len();
        debug!(
            "ring executor refresh_some doing dirty rings, count: {}",
            dirty_rings_len
        );

        // 批量处理以提高性能
        const BATCH_SIZE: usize = 50;
        let mut processed_count = 0;
        let mut invalid_rings = Vec::new();

        // 将 DashMap 转换为 Vec 以避免长时间持有锁
        // 使用 take 而不是 clear，保留处理期间新增的 dirty rings
        let rings: Vec<(String, Arc<Ring>)> = {
            let mut temp = DashMap::new();
            std::mem::swap(&mut temp, &mut self.state.dirty_rings);
            temp.into_iter().collect()
        };

        for chunk in rings.chunks(BATCH_SIZE) {
            //let chunk = chunk.to_vec(); // 克隆当前批次的 rings
            let results = futures::future::join_all(chunk.to_vec().into_iter().map(|(ring_id, ring)| {
                let chain_data = self.chain_data.clone();
                let in_amounts = self.in_amounts.clone();
                let expected_gain = self.expected_gain;

                tokio::spawn(async move {
                    let mut snapshot = HashMap::new();
                    let mut has_at_least_one_non_zero = false;
                    let mut best_route = None;

                    for &in_amount in &in_amounts {
                        if let Ok((steps, out_amount, slot)) =
                            ring.build_route_steps(&chain_data, &mut snapshot, in_amount)
                        {
                            has_at_least_one_non_zero = true;
                            let gain: i128 = out_amount
                                .checked_sub(in_amount)
                                .map(Into::into)
                                .unwrap_or(0);

                            if gain > i128::from(expected_gain) {
                                best_route = Some((steps, out_amount, slot, gain));
                                break;
                            }
                        }
                    }

                    (ring_id.clone(), ring, has_at_least_one_non_zero, best_route)
                })
            }))
            .await;

            for result in results {
                if let Ok((ring_id, ring, has_non_zero, best_route)) = result {
                    if let Some((route_steps, out_amount, context_slot, gain)) = best_route {
                        // 更新 ring state
                        if let Ok(mut state) = ring.ring_state.write() {
                            state.current_gain = gain;
                        }

                        // 创建并发送路由
                        let route = Arc::new(Route {
                            input_mint: ring.trading_mint.clone(),
                            output_mint: ring.trading_mint.clone(),
                            in_amount: route_steps.first().map_or(0, |step| step.in_amount),
                            out_amount,
                            price_impact_bps: 0,
                            steps: route_steps,
                            slot: context_slot,
                            accounts: Default::default(),
                        });

                        if let Err(e) = self.route_sender.send(route).await {
                            error!("Failed to send route for ring {}: {}", ring_id, e);
                        }
                    } else if !has_non_zero {
                        invalid_rings.push((ring_id, ring));
                    }
                }
            }

            processed_count += chunk.len();

            // 检查超时
            if started_at.elapsed() > Duration::from_millis(400) {
                warn!(
                    "amount calculation timeout after processing {}/{} rings in {}ms",
                    processed_count,
                    dirty_rings_len,
                    started_at.elapsed().as_millis()
                );
                break;
            }
        }

        let invalid_rings_len = invalid_rings.len();

        // 处理无效的 rings
        for (ring_id, ring) in invalid_rings {
            if let Ok(mut state) = ring.ring_state.write() {
                state.set_valid(false);
                state.add_cooldown(&Duration::from_secs(30));
            }
            self.state.dirty_rings.remove(&ring_id);
        }
        

        // 清理已处理的 rings
        //self.state.dirty_rings.clear();

        debug!(
            "amount calculation completed in {}ms, processed {}/{} rings,invalid rings {}",
            started_at.elapsed().as_millis(),
            processed_count,
            dirty_rings_len,
            invalid_rings_len
        );
    }
}

pub fn spawn_ring_executor_job(
    config: &Config,
    //   ready_sender: async_channel::Sender<()>,
    chain_data: AccountProviderView,
    token_cache: Arc<TokenCache>,
    path_warming_amounts: Vec<u64>,
    edges: Vec<Arc<Edge>>,
    edge_price_updates: async_channel::Receiver<Arc<Edge>>,
    route_sender: async_channel::Sender<Arc<Route>>,
    mut exit: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    // Initialize the RingExecutor with the provided configuration and data
    let mut ring_executor = RingExecutor::new(
        config,
        chain_data,
        token_cache,
        //  ready_sender,
        // edge_price_updates,
        path_warming_amounts,
        edges.clone(),
        route_sender,
    );

    // // 获取初始化超时时间，默认为 5 分钟
    // let init_timeout_in_seconds = config.snapshot_timeout_in_seconds.unwrap_or(60);
    // // 计算初始化超时时刻
    // let init_timeout = Instant::now() + Duration::from_secs(init_timeout_in_seconds);

    // for edge in edges.iter() {
    //     ring_executor.do_dirty_ring(edge.clone());
    //     if !ring_executor.state.is_ready && init_timeout < Instant::now() {
    //         error!("Failed to init ring executor to do dirty ring before timeout");
    //         break;
    //     }
    // }
    ring_executor.state.is_ready = true;

    // 生成 Tokio 任务
    let listener_job = tokio_spawn("ring_executor", async move {
        // 初始化刷新间隔
        let mut refresh_one_interval = tokio::time::interval(Duration::from_millis(100));

        // refresh_one_interval.tick().await;

        // 等待第一次刷新间隔
        refresh_one_interval.tick().await;

        info!("ring executor is ready");

        // 主循环，处理各种更新事件
        'drain_loop: loop {
            tokio::select! {
                // 处理退出信号
                _ = exit.recv() => {
                    info!("shutting down ring executor task");
                    break;
                }
                edge = edge_price_updates.recv() => {
                    match edge {
                        Ok(edge) => {
                            debug!("receiving  edge_price_updates, edge: {:?}", edge.unique_id());
                            ring_executor.do_dirty_ring(edge);
                        },
                        Err(e) => {
                            error!(
                                "Error on edge_price_updates channel in ring update task {:?}",
                                 e
                            );
                        }
                    }

                },
                // 处理刷新间隔事件
                _ = refresh_one_interval.tick() => {

                    ring_executor.refresh_some().await;
                }
            }
        }

        // 发送准备就绪信号，解除退出处理程序前的阻塞
        // send this to unblock the code in front of the exit handler
        //  let _ = ring_executor.ready_sender.try_send(());

        error!("ring executor job exited..");
    });

    listener_job
}
