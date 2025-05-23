
use std::hash::{Hasher,Hash};
use std::time::Duration;
use std::collections::hash_map::DefaultHasher;
use std::cmp::min;
use std::collections::HashSet;
use ordered_float::Pow;

// 引入自定义的调试工具模块
use crate::debug_tools;
// 引入自定义的预导入模块，包含常用的类型和特性
use crate::prelude::*;
use crate::ring_executor::RingingError;
use crate::routing_types::*;

use router_lib::dex::AccountProviderView;
use router_lib::dex::DexEdge;



#[derive(Clone, Debug, Default, serde_derive::Serialize, serde_derive::Deserialize)]
pub struct RingState {
    pub cached_prices: Vec<(u64, f64, f64)>,
    pub current_gain: i128,
    is_valid: bool,

    /// 这条环经历了多少次冷却事件
    pub cooldown_event: u64,
    /// 这条环何时会再次可用
    pub cooldown_until: Option<u64>,
}

pub struct Ring {
    /// The mint of the ring. This is the mint of the token that will be used to pay for the swap.
    pub trading_mint: Pubkey,
    pub ring_id: String,
    pub edges: Vec<Arc<Edge>>,
    //dex_edges: HashMap<(Pubkey, Pubkey), Option<Arc<dyn DexEdge>>>,
    pub ring_ming_symbols: HashSet<String>,
    pub ring_state: Arc<RwLock<RingState>>,
}

pub  fn ring_id_hash_from_edges (ring_mint:&Pubkey,edges: &[Arc<Edge>]) -> String {
    let mut hasher = DefaultHasher::new();
    ring_mint.hash(&mut hasher);
    for edge in edges {
        edge.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

impl Ring {
    pub fn new(trading_mint: Pubkey, edges: Vec<Arc<Edge>>,ring_ming_symbols:HashSet<String>) -> Self {
        let ring_id = ring_id_hash_from_edges(&trading_mint,&edges);
        // let mut dex_edges: HashMap<(Pubkey, Pubkey), Option<Arc<dyn DexEdge>>> = HashMap::new();
        // for edge in edges.iter() {
        //     dex_edges.entry(edge.unique_id())
        //     .or_insert_with(move || edge.prepare(chain_data).ok());
        // }

        Self {
            trading_mint,
            ring_id,
            edges,
            ring_ming_symbols,
            ring_state: Arc::new(RwLock::new(RingState::default())),
        }
    }

    pub fn get_edges(&self) -> Vec<Arc<Edge>> {
        self.edges.clone()
    }

    pub fn get_ring_id(&self) -> String {
        self.ring_id.clone()
    }

    pub fn get_trading_mint(&self) -> Pubkey {
        self.trading_mint
    }  

    pub fn compute_out_amount(&self,
        chain_data: &AccountProviderView,
        mut snapshot: &mut HashMap<(Pubkey, Pubkey), Option<Arc<dyn DexEdge>>>,
        amount: u64,
        add_cooldown: bool,
    ) -> anyhow::Result<Option<(u64, u64)>> /* (quote price, cached price) */ {
        let mut current_in_amount = amount;
        let mut current_in_amount_dumb = amount;

        for edge in self.edges.iter() {
            if !edge.state.read().unwrap().is_valid() {
                warn!(edge = edge.desc(), "invalid edge");
                return Ok(None);
            }
            let prepared_quote = match prepare(&mut snapshot, &edge, chain_data) {
                Some(p) => p,
                _ => bail!(RingingError::CouldNotComputeOut),
            };
            let quote_res = edge.quote(&prepared_quote, chain_data, current_in_amount);
            let Ok(quote) = quote_res else {
                if add_cooldown {
                    edge.state
                        .write()
                        .unwrap()
                        .add_cooldown(&Duration::from_secs(30));
                }
                warn!(
                    edge = edge.desc(),
                    amount,
                    "failed to quote, err: {:?}",
                    quote_res.unwrap_err()
                );
                return Ok(None);
            };

            if quote.out_amount == 0 {
                if add_cooldown {
                    edge.state
                        .write()
                        .unwrap()
                        .add_cooldown(&Duration::from_secs(30));
                }
                warn!(edge = edge.desc(), amount, "quote is zero, skipping");
                return Ok(None);
            }

            let Some(price) = edge
                .state
                .read()
                .unwrap()
                .cached_price_for(current_in_amount)
            else {
                return Ok(None);
            };

            current_in_amount = quote.out_amount;
            current_in_amount_dumb = ((quote.in_amount as f64) * price.0).round() as u64;

            //为什么要校验缓存价格？
            // 1. 价格不一致，可能是因为缓存价格过期了
            //saturating_mul 方法则采用了饱和运算的策略。当乘法结果超出该类型所能表示的最大值时，它会返回该类型的最大值，而不是发生回绕或产生未定义行为。例如，对于 u8 类型，u8::MAX.saturating_mul(2) 会返回 255。
            if current_in_amount_dumb > current_in_amount.saturating_mul(3) {
                if add_cooldown {
                    edge.state
                        .write()
                        .unwrap()
                        .add_cooldown(&Duration::from_secs(30));
                }
                warn!(
                    out_quote = quote.out_amount,
                    out_dumb = current_in_amount_dumb,
                    in_quote = quote.in_amount,
                    price = price.0,
                    edge = edge.desc(),
                    input_mint = debug_tools::name(&edge.input_mint),
                    output_mint = debug_tools::name(&edge.output_mint),
                    prices = edge
                        .state
                        .read()
                        .unwrap()
                        .cached_prices
                        .iter()
                        .map(|x| format!("in={}, price={}", x.0, x.1))
                        .join("||"),
                    "recomputed path amount diverge a lot from estimation - path ignored"
                );
                return Ok(None);
            }
        } 
        Ok(Some((current_in_amount, current_in_amount_dumb)))
    }

    

    pub fn build_route_steps(&self,
        chain_data: &AccountProviderView,
        mut snapshot: &mut HashMap<(Pubkey, Pubkey), Option<Arc<dyn DexEdge>>>,
        in_amount: u64,
    ) -> anyhow::Result<(Vec<RouteStep>, u64, u64)> {
        let mut context_slot = 0;
        let mut steps = Vec::with_capacity(self.edges.len());
        let mut current_in_amount = in_amount;
        for edge in self.edges.iter() {
            let prepared_quote = match prepare(&mut snapshot, &edge, chain_data) {
                Some(p) => p,
                _ => bail!(RingingError::CouldNotComputeOut),
            };

            let quote = edge.quote(&prepared_quote, chain_data, current_in_amount)?;
            steps.push(RouteStep {
                edge: edge.clone(),
                in_amount: quote.in_amount,
                out_amount: quote.out_amount,
                fee_amount: quote.fee_amount,
                fee_mint: quote.fee_mint,
            });
            current_in_amount = quote.out_amount;
            let edge_slot = edge.state.read().unwrap().last_update_slot;
            context_slot = edge_slot.max(context_slot);
        }

        Ok((steps, current_in_amount,context_slot))
    }

    

}

impl RingState {
    
    pub fn set_valid(&mut self, valid: bool) {
        self.is_valid = valid;
    }

    pub fn is_valid(&self) -> bool {
        if !self.is_valid {
            return false;
        }

        if self.cooldown_until.is_some() {
            // 这里不检查时间！
            // 我们会在冷却期后的第一次账户更新时重置 "cooldown until"
            // 所以如果还没有重置，说明我们没有做任何更改
            // 没有理由再次启用
            return false;
        }

        true
    }

    pub fn can_reset_cooldown(&self) -> bool {
        if self.cooldown_until.is_none() {
            return true;
        }

        let now = millis_since_epoch();
        if let Some(until) = self.cooldown_until {
            if now > until {
                return true;
            }
        }

        false
    }

    /// 重置冷却状态
    pub fn reset_cooldown(&mut self) {
        self.cooldown_event += 0;
        self.cooldown_until = None;
    }

    /// 添加冷却时间
    pub fn add_cooldown(&mut self, duration: &Duration) {
        self.cooldown_event += 1;

        // 计算冷却因子
        let counter = min(self.cooldown_event, 5) as f64;
        let exp_factor = 1.2.pow(counter);
        let factor = (counter * exp_factor).round() as u64;
        let until = millis_since_epoch() + (duration.as_millis() as u64 * factor);

        // 更新冷却结束时间
        self.cooldown_until = match self.cooldown_until {
            None => Some(until),
            Some(current) => Some(current.max(until)),
        };
    }
}



fn prepare(
    s: &mut HashMap<(Pubkey, Pubkey), Option<Arc<dyn DexEdge>>>,
    e: &Arc<Edge>,
    c: &AccountProviderView,
) -> Option<Arc<dyn DexEdge>> {
    s.entry(e.unique_id())
        .or_insert_with(move || e.prepare(c).ok())
        .clone()
}