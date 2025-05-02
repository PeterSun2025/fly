// 引入自定义的调试工具模块
use crate::debug_tools;
// 引入自定义的预导入模块，包含常用的类型和特性
use crate::prelude::*;
// 引入自定义的代币缓存模块
use crate::token_cache::TokenCache;
// 引入 ordered_float 库中的 Pow 特性，用于浮点数的幂运算
use ordered_float::Pow;
// 引入路由库中的 DEX 相关模块
use router_lib::dex::{
    AccountProviderView, DexEdge, DexEdgeIdentifier, DexInterface, Quote, SwapInstruction,
};
// 引入路由库中的价格缓存模块
use router_lib::price_feeds::price_cache::PriceCache;
// 引入 serde 库，用于序列化和反序列化
use serde::{Deserialize, Deserializer, Serialize, Serializer};
// 引入标准库中的 cmp 模块，用于比较操作
use std::cmp::min;
// 引入标准库中的 fmt 模块，用于格式化输出
use std::fmt::Formatter;
// 引入标准库中的时间模块
use std::time::Duration;
// 引入标准库中的 hash 模块，用于哈希操作
use std::hash::{Hash, Hasher};

// 定义一个可克隆、可调试、可序列化和反序列化的边状态结构体
#[derive(Clone, Debug, Default, serde_derive::Serialize, serde_derive::Deserialize)]
pub struct EdgeState {
    /// 按输入升序排序的 (输入, 价格, 价格的自然对数) 对列表
    // TODO: 集中存储这个列表可能会更好，这样进行快照操作会更高效
    pub cached_prices: Vec<(u64, f64, f64)>,
    // 边状态是否有效
    is_valid: bool,
    // 上次更新的时间戳（毫秒）
    pub last_update: u64,
    // 上次更新时的槽位号
    pub last_update_slot: u64,

    /// 这条边经历了多少次冷却事件
    pub cooldown_event: u64,
    /// 这条边何时会再次可用
    pub cooldown_until: Option<u64>,
}

// 定义边结构体
pub struct Edge {
    // 输入代币的铸币公钥
    pub input_mint: Pubkey,
    // 输出代币的铸币公钥
    pub output_mint: Pubkey,
    // 对实现了 DexInterface 特性的 DEX 的原子引用
    pub dex: Arc<dyn DexInterface>,
    // 对实现了 DexEdgeIdentifier 特性的边标识符的原子引用
    pub id: Arc<dyn DexEdgeIdentifier>,

    /// 遍历这条边所需的账户数量，不包括源代币账户、签名者、代币程序、关联代币账户程序、系统程序
    // TODO: 这里也许应该使用 Vec<Pubkey> 类型，这样多个相同类型的边所需的账户数量可能会更少
    // 并且有助于选择地址查找表，但这取决于具体的 quote() 结果需要哪些 tick 数组
    pub accounts_needed: usize,

    // 边状态的读写锁
    pub state: RwLock<EdgeState>,
    // TODO: 地址查找表，去提升
}

// 为 Edge 结构体实现 Debug 特性，用于格式化输出
impl std::fmt::Debug for Edge {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} => {} ({})",
            // 使用调试工具获取输入铸币公钥的名称
            debug_tools::name(&self.input_mint),
            // 使用调试工具获取输出铸币公钥的名称
            debug_tools::name(&self.output_mint),
            // 获取 DEX 的名称
            self.dex.name()
        )
    }
}

// 为 Edge 结构体实现 Serialize 特性，但目前只是占位，未实现具体逻辑
impl Serialize for Edge {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        todo!()
    }
}

// 为 Edge 结构体实现 Deserialize 特性，但目前只是占位，未实现具体逻辑
impl<'de> Deserialize<'de> for Edge {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        todo!()
    }
}

// 为 Edge 结构体实现方法
impl Edge {
    // 获取边的公钥
    pub fn key(&self) -> Pubkey {
        self.id.key()
    }

    // 获取边的唯一标识符
    pub fn unique_id(&self) -> (Pubkey, Pubkey) {
        (self.id.key(), self.id.input_mint())
    }

    // 获取边的描述信息
    pub fn desc(&self) -> String {
        self.id.desc()
    }

    // 获取边所属 DEX 的类型
    pub fn kind(&self) -> String {
        self.dex.name()
    }

    // 构建交换指令
    pub fn build_swap_ix(
        &self,
        chain_data: &AccountProviderView,
        wallet_pk: &Pubkey,
        amount_in: u64,
        out_amount: u64,
        max_slippage_bps: i32,
    ) -> anyhow::Result<SwapInstruction> {
        self.dex.build_swap_ix(
            &self.id,
            chain_data,
            wallet_pk,
            amount_in,
            out_amount,
            max_slippage_bps,
        )
    }

    // 准备边的引用
    pub fn prepare(&self, chain_data: &AccountProviderView) -> anyhow::Result<Arc<dyn DexEdge>> {
        let edge = self.dex.load(&self.id, chain_data)?;
        Ok(edge)
    }

    // 获取输入金额的报价
    pub fn quote(
        &self,
        prepared_quote: &Arc<dyn DexEdge>,
        chain_data: &AccountProviderView,
        in_amount: u64,
    ) -> anyhow::Result<Quote> {
        self.dex
           .quote(&self.id, &prepared_quote, chain_data, in_amount)
    }

    // 检查边是否支持精确输出
    pub fn supports_exact_out(&self) -> bool {
        self.dex.supports_exact_out(&self.id)
    }

    // 获取精确输出金额的报价
    pub fn quote_exact_out(
        &self,
        prepared_quote: &Arc<dyn DexEdge>,
        chain_data: &AccountProviderView,
        out_amount: u64,
    ) -> anyhow::Result<Quote> {
        self.dex
           .quote_exact_out(&self.id, &prepared_quote, chain_data, out_amount)
    }

    // 内部更新边的状态
    pub fn update_internal(
        &self,
        chain_data: &AccountProviderView,
        decimals: u8,
        price: f64,
        path_warming_amounts: &Vec<u64>,
    ) {
        //在环中更新价格，边不再更新自己价格
        // // 计算乘数
        // let multiplier = 10u64.pow(decimals as u32) as f64;
        // // 计算不同输入金额对应的数量
        // let amounts = path_warming_amounts
        //    .iter()
        //    .map(|amount| {
        //         let quantity_ui = *amount as f64 / price;
        //         let quantity_native = quantity_ui * multiplier;
        //         quantity_native.ceil() as u64
        //     })
        //    .collect_vec();

        // // 记录价格数据的调试信息
        // debug!(input_mint = %self.input_mint, pool = %self.key(), multiplier = multiplier, price = price, amounts = amounts.iter().join(";"), "price_data");

        // // 检查是否有溢出情况
        // let overflow = amounts.iter().any(|x| *x == u64::MAX);
        // if overflow {
        //     if self.state.read().unwrap().is_valid {
        //         debug!("amount error, disabling edge {}", self.desc());
        //     }

        //     // 获取边状态的写锁
        //     let mut state = self.state.write().unwrap();
        //     // 更新上次更新时间
        //     state.last_update = millis_since_epoch();
        //     // 更新上次更新槽位
        //     state.last_update_slot = chain_data.newest_processed_slot();
        //     // 清空缓存价格
        //     state.cached_prices.clear();
        //     // 标记状态无效
        //     state.is_valid = false;
        //     return;
        // }

        // // 准备报价
        // let prepared_quote = self.prepare(chain_data);

        // // 计算不同输入金额的报价结果
        // let quote_results_in = amounts
        //    .iter()
        //    .map(|&amount| match &prepared_quote {
        //         Ok(p) => (amount, self.quote(&p, chain_data, amount)),
        //         Err(e) => (
        //             amount,
        //             anyhow::Result::<Quote>::Err(anyhow::format_err!("{}", e)),
        //         ),
        //     })
        //    .collect_vec();

        // // 检查是否有报价错误
        // if let Some((_, err)) = quote_results_in.iter().find(|v| v.1.is_err()) {
        //     if self.state.read().unwrap().is_valid {
        //         warn!("quote error, disabling edge: {} {err:?}", self.desc());
        //     } else {
        //         info!("edge update_internal quote error: {} {err:?}", self.desc());
        //     }
        // }

        // 获取边状态的写锁
        let mut state = self.state.write().unwrap();
        // 更新上次更新时间
        state.last_update = millis_since_epoch();
        // 更新上次更新槽位
        state.last_update_slot = chain_data.newest_processed_slot();
        // 清空缓存价格
        state.cached_prices.clear();
        // 标记状态有效
        state.is_valid = true;

        // 检查冷却时间是否已过
        if let Some(timestamp) = state.cooldown_until {
            if timestamp < state.last_update {
                state.cooldown_until = None;
            }
        };

        // let mut has_at_least_one_non_zero = false;
        // for quote_result in quote_results_in {
        //     if let (in_amount, Ok(quote)) = quote_result {
        //         // 计算价格
        //         let price = quote.out_amount as f64 / in_amount as f64;
        //         if price.is_nan() {
        //             state.is_valid = false;
        //             continue;
        //         }
        //         if price > 0.0000001 {
        //             has_at_least_one_non_zero = true;
        //         }
        //         // 将价格和其对数存入缓存
        //         state.cached_prices.push((in_amount, price, f64::ln(price)));
        //     } else {
        //         // 如果报价失败，标记状态无效
        //         state.is_valid = false;
        //     };
        // }

        // // 如果没有至少一个非零价格，标记状态无效
        // if !has_at_least_one_non_zero {
        //     state.is_valid = false;
        // }
    }

    // 更新边的状态
    pub fn update(
        &self,
        chain_data: &AccountProviderView,
        token_cache: &TokenCache,
        price_cache: &PriceCache,
        path_warming_amounts: &Vec<u64>,
    ) {
        // 记录更新边的跟踪信息
        trace!(edge = self.desc(), "updating");

        // 获取输入代币的小数位数
        let Ok(decimals) = token_cache.token(self.input_mint).map(|x| x.decimals) else {
            let mut state = self.state.write().unwrap();
            trace!("no decimals for {}", self.input_mint);
            state.is_valid = false;
            return;
        };
        // 获取输入代币的价格
        // let Some(price) = price_cache.price_ui(self.input_mint) else {
        //     let mut state = self.state.write().unwrap();
        //     state.is_valid = false;
        //     trace!("no price for {}", self.input_mint);
        //     return;
        // };

        let price =1.0; // TODO: 这里需要获取输入代币的价格

        // 调用内部更新方法
        self.update_internal(chain_data, decimals, price, path_warming_amounts);
    }
}

impl Hash for Edge {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // 使用边的唯一标识符、输出铸币公钥
        self.unique_id().hash(state);
        self.output_mint.hash(state);
    }
}

// 为 EdgeState 结构体实现方法
impl EdgeState {
    /// 根据输入金额返回最适用的价格（原生代币/原生代币）和价格的自然对数
    /// 如果状态无效则返回 None
    pub fn cached_price_for(&self, in_amount: u64) -> Option<(f64, f64)> {
        if !self.is_valid() || self.cached_prices.is_empty() {
            return None;
        }

        // 查找最适用的缓存价格
        let cached_price = self
           .cached_prices
           .iter()
           .find(|(cached_in_amount, _, _)| *cached_in_amount >= in_amount)
           .unwrap_or(&self.cached_prices.last().unwrap());
        Some((cached_price.1, cached_price.2))
    }

    /// 根据输出金额返回精确输出的最适用价格（原生代币/原生代币）和价格的自然对数
    /// 如果状态无效则返回 None
    pub fn cached_price_exact_out_for(&self, out_amount: u64) -> Option<(f64, f64)> {
        if !self.is_valid() {
            return None;
        }

        let out_amount_f = out_amount as f64;
        // 查找最适用的缓存价格
        let cached_price = self
           .cached_prices
           .iter()
           .find(|(cached_in_amount, p, _)| (*cached_in_amount as f64) * p >= out_amount_f)
           .unwrap_or(&self.cached_prices.last().unwrap());

        // 计算精确输出的逆价格
        let price = 1.0 / cached_price.1;
        Some((price, f64::ln(price)))
    }

    /// 检查边状态是否有效
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

    /// 重置冷却状态
    pub fn reset_cooldown(&mut self) {
        self.cooldown_event += 0;
        self.cooldown_until = None;
    }

    /// 添加冷却时间
    pub fn add_cooldown(&mut self, duration: &Duration) {
        self.cooldown_event += 1;

        // 计算冷却因子
        let counter = min(self.cooldown_event, 10) as f64;
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