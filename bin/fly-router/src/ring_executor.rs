
use ahash::AHashMap;
use router_lib::dex::AccountProviderView;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio::time::{Instant,Duration};
use std::collections::HashSet;
use crate::graph::Graph;
use crate::routing_types::Route;
use crate::source::token_cache::TokenCache;
// 引入自定义的预导入模块，包含常用的类型和特性
use crate::{edge, prelude::*};
use crate::ring::Ring;
use crate::util::tokio_spawn;

use router_config_lib::Config;





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

    dirty_rings: AHashMap<String, Arc<Ring>>,
}

pub struct RingExecutor {
    // 准备就绪信号发送器
   // ready_sender: async_channel::Sender<()>,

    chain_data: AccountProviderView,

    token_cache: Arc<TokenCache>,

    trading_mint_rings: AHashMap<Pubkey, Vec<Arc<Ring>>>,

    edge_rings: AHashMap<(Pubkey, Pubkey), AHashMap<String,Arc<Ring>>>,

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
        let max_path_length: usize  = config.ring.max_path_length.unwrap_or(3);
        let mut trading_mints: Vec<String> = config.ring.trading_mints.clone().unwrap_or_default();
        if trading_mints.is_empty() {     
            trading_mints.push("So11111111111111111111111111111111111111112".to_string());
        }


        let mut in_amounts: Vec<u64> = config.sender.in_amounts.clone().unwrap_or([10_00_000_000, 500_000_000, 100_000_000].to_vec());
        in_amounts.sort_by(|a, b| b.cmp(a));
        let expected_gain: u64 = config.sender.expected_gain.unwrap_or(1_000_000);        

        let mut ring_mint_rings: AHashMap<Pubkey, Vec<Arc<Ring>>> = AHashMap::new();
        let mut edge_rings: AHashMap<(Pubkey, Pubkey), AHashMap<String, Arc<Ring>>> = AHashMap::new();
        let mut graph = Graph::new();
        graph.add_edges(edges.clone()); 
        for trading_mint_string in trading_mints.iter() {
            let ring_mint = Pubkey::from_str(trading_mint_string).unwrap_or_else(|_| {
                panic!("Invalid mint address: {}", trading_mint_string)
            });
            let cycles : Vec<Vec<Arc<Edge>>> = graph.find_cycles(ring_mint, max_path_length);

            info!("ring mint {} have {} rings",trading_mint_string,cycles.len(),);
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
                let ring = Arc::new(Ring::new(ring_mint, cycle.clone(),ring_ming_symbols));
                let mut ring_state = ring.ring_state.write().unwrap();
                ring_state.set_valid(true);
                ring_state.reset_cooldown();
                ring_mint_rings.entry(ring_mint).or_default().push(ring.clone());
                for edge in cycle.iter() {
                    let unique_id = edge.unique_id();
                    edge_rings.entry(unique_id).or_default().insert(ring.get_ring_id(), ring.clone());
                }
            }   
        }

        Self {
           // ready_sender,
            chain_data,
            token_cache,
            trading_mint_rings: ring_mint_rings,
            edge_rings,
            //dirty_rings: AHashMap::new(),
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

    pub fn do_dirty_ring(&mut self,edge:Arc<Edge>) {
        let unique_id = edge.unique_id();
        
        if let Some(ring_map) = self.edge_rings.get(&unique_id){
            debug!("do_dirty_ring : dirty edge: {:?},dirty ring num {} ", unique_id.clone(),ring_map.len());
            // Iterate over the rings associated with the edge and mark them as dirty
            ring_map.iter().for_each(|(ring_id, ring)| {
                let ring_state = ring.ring_state.read().unwrap();
                if ring_state.is_valid() {
                    self.state.dirty_rings.entry(ring_id.clone()).or_insert(ring.clone());
                } else if ring_state.can_reset_cooldown() {
                    // Reset cooldown if the ring is valid and can reset cooldown
                    let mut ring_state = ring.ring_state.write().unwrap();
                    ring_state.reset_cooldown();
                    debug!("Reset cooldown for ring_id: {}", ring_id);
                }
            });
        } else {
            debug!("No ring found for edge: {:?}", unique_id);
        }
        
    }

    pub fn refresh_some(&mut self) {
        let state = &mut self.state;

        debug!("ring executor refresh_some start {},dirty_rings is empty? {}", state.is_ready,state.dirty_rings.is_empty());
        
        if state.dirty_rings.is_empty() || !state.is_ready {
            return;
        }
        info!("ring executor refresh_some doing dirty rings, count: {}", state.dirty_rings.len());
        let started_at = Instant::now();
        let mut refreshed_rings = vec![];

        let mut snapshot = HashMap::new();

        let dirty_rings_len = state.dirty_rings.len();
        for (ring_id, ring) in self.state.dirty_rings.iter() { 
            refreshed_rings.push(ring_id.clone());
            let mut has_at_least_one_non_zero = false;
            for in_amount in self.in_amounts.iter() {
                if let Some((route_steps, out_amount, context_slot)) = ring.build_route_steps(
                    &self.chain_data,
                    &mut snapshot,
                    *in_amount,
                ).ok() {
                    let gain:i128 = (out_amount - in_amount).into();
                    if gain > self.expected_gain.into() && gain != ring.ring_state.read().unwrap().current_gain {
                        ring.ring_state.write().unwrap().current_gain = gain;
                        info!(
                            "ring_id = {},  in_amount = {}, out_amount = {}, gain = {}, context_slot = {}",
                            ring_id,
                            in_amount,
                            out_amount,
                            gain,
                            context_slot,
                        );
        
                        let route = Arc::new(Route {
                            input_mint: ring.trading_mint.clone(),
                            output_mint: ring.trading_mint.clone(),
                            in_amount: route_steps.first().map_or(0, |step| step.in_amount),
                            out_amount: route_steps.last().map_or(0, |step| step.out_amount),
                            price_impact_bps:0,
                            steps: route_steps,
                            slot: context_slot,
                            accounts: Default::default(),//为什么quote里是Default::default()？
                        });
                        let _ = self.route_sender.send(route);
                        has_at_least_one_non_zero = true;
                        break;
                    }
                } else {
                    debug!("Failed to build route steps for ring_id: {}, in_amount: {}", ring_id, in_amount);
                }
                
            }

            if !has_at_least_one_non_zero {
                let mut ring_state = ring.ring_state.write().unwrap();
                ring_state.set_valid(false);
                ring_state.add_cooldown(&Duration::from_secs(30));
                debug!("Failed to execute ring_id: {}, in_amounts: {:?}", ring_id, self.in_amounts);
            }
            
            if started_at.elapsed() > Duration::from_millis(200) {
                warn!("computing ring price took more than 200ms, dirty_rings {}, refreshed_rings {} ", 
                    dirty_rings_len,refreshed_rings.len());
                self.state.dirty_rings.clear();
                break;
            }
        }
        
        for ring_id in refreshed_rings {
            self.state.dirty_rings.remove(&ring_id);
        }
        
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
    let mut ring_executor = RingExecutor::new (
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
                            info!("receiving  edge_price_updates, edge: {:?}", edge.unique_id());
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

                    ring_executor.refresh_some();
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

