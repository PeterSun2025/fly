
use ahash::AHashMap;
use router_lib::dex::AccountProviderView;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio::time::{Instant,Duration};

use crate::graph::Graph;
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

    ring_mint_rings: AHashMap<Pubkey, Vec<Arc<Ring>>>,

    edge_rings: AHashMap<(Pubkey, Pubkey), AHashMap<String,Arc<Ring>>>,

    path_warming_amounts: Vec<u64>,

    max_path_length: usize,

    graph: Graph,

    state: RingExecutorState,
}

impl RingExecutor {
    pub fn new(
        config: &Config,
        chain_data: AccountProviderView,
       // ready_sender: async_channel::Sender<()>,
        //edge_price_updates: broadcast::Receiver<Arc<Edge>>,
        path_warming_amounts: Vec<u64>,
        edges: Vec<Arc<Edge>>,
    ) -> Self {
        let max_path_length: usize  = config.ring.max_path_length.unwrap_or(3);
        let mut ring_mints: Vec<String> = config.ring.ring_mints.clone().unwrap_or_default();
        if ring_mints.is_empty() {     
            ring_mints.push("So11111111111111111111111111111111111111112".to_string());
         }
        let mut ring_mint_rings: AHashMap<Pubkey, Vec<Arc<Ring>>> = AHashMap::new();
        let mut edge_rings: AHashMap<(Pubkey, Pubkey), AHashMap<String, Arc<Ring>>> = AHashMap::new();
        let mut graph = Graph::new();
        graph.add_edges(edges.clone()); 
        for ring_mint_string in ring_mints.iter() {
            let ring_mint = Pubkey::from_str(ring_mint_string).unwrap_or_else(|_| {
                panic!("Invalid mint address: {}", ring_mint_string)
            });
            let cycles : Vec<Vec<Arc<Edge>>> = graph.find_cycles(ring_mint, max_path_length);

            info!("ring mint {} have {} rings",ring_mint_string,cycles.len(),);
            for cycle in cycles.iter() {   
                let ring = Arc::new(Ring::new(ring_mint, cycle.clone()));
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
            ring_mint_rings,
            edge_rings,
            //dirty_rings: AHashMap::new(),
            //edge_price_updates,
            path_warming_amounts,
            max_path_length,
            graph,
            state: RingExecutorState::default(),
        }
        

    }

    pub fn do_dirty_ring(&mut self,edge:Arc<Edge>) {
        let unique_id = edge.unique_id();
        
        if let Some(ring_map) = self.edge_rings.get(&unique_id){
            debug!("do_dirty_ring : dirty edge: {:?},dirty ring num {} ", unique_id.clone(),ring_map.len());
            // Iterate over the rings associated with the edge and mark them as dirty
            ring_map.iter().for_each(|(ring_id, ring)| {
                self.state.dirty_rings.entry(ring_id.clone()).or_insert(ring.clone());
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
        
        for (ring_id, ring) in self.state.dirty_rings.iter() {
            refreshed_rings.push(ring_id.clone());
            if let Some((route_steps, out_amount, context_slot)) = ring.build_route_steps(
                &self.chain_data,
                &mut snapshot,
                1_000_000,
            ).ok() {
                info!(
                    "ring_id = {},  in_amount = {}, out_amount = {}, context_slot = {}",
                    ring_id,
                    1_000_000,
                    out_amount,
                    context_slot,
                );
            }
            
            if started_at.elapsed() > Duration::from_millis(200) {
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
    path_warming_amounts: Vec<u64>,
    edges: Vec<Arc<Edge>>,
    edge_price_updates: async_channel::Receiver<Arc<Edge>>,
    mut exit: broadcast::Receiver<()>,
) -> JoinHandle<()> {   

    // Initialize the RingExecutor with the provided configuration and data
    let mut ring_executor = RingExecutor::new (
        config,
        chain_data,
      //  ready_sender,
       // edge_price_updates,
        path_warming_amounts,
        edges.clone(),
    );

    // // Spawn the executor job
    // Some(tokio::spawn(async move {
    //     loop {
    //         tokio::select! {
    //             _ = exit.recv() => {
    //                 info!("shutting down ring executor task,mints");
    //                 break;
    //             }
    //             edge = edge_price_updates.recv() => {
    //                 ring_executor.do_dirty_ring(edge.unwrap_or_else(|_| {
    //                     panic!("Error receiving edge price update")
    //                 }));
    //             },
    //         }
    //     }
    // }))


    // 获取初始化超时时间，默认为 5 分钟
    let init_timeout_in_seconds = config.snapshot_timeout_in_seconds.unwrap_or(60);
    // 计算初始化超时时刻
    let init_timeout = Instant::now() + Duration::from_secs(init_timeout_in_seconds);

    for edge in edges.iter() {
        ring_executor.do_dirty_ring(edge.clone());
        if !ring_executor.state.is_ready && init_timeout < Instant::now() {
            error!("Failed to init ring executor to do dirty ring before timeout");
            break;
        }
    }
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

