use std::collections::HashMap;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::Ok;
use router_lib::dex::SwapMode;
use serde::{Serialize, Deserialize};
use solana_client::client_error::reqwest;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;

use crate::ix_builder::SwapInstructionsBuilder;
use crate::routing_types::{Route, RouteStep};
use crate::swap::Swap;

lazy_static::lazy_static! {
    static ref ATA_PROGRAM_ID: Pubkey = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap();
}


   /// 交换信息结构体
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(rename_all = "camelCase")]
   pub struct JupRouteStep {
       pub amm_key: String,        // JSON 中的 "ammKey"
       pub label: String,
       pub input_mint: String,     // JSON 中的 "inputMint"
       pub output_mint: String,    // JSON 中的 "outputMint"
       pub in_amount: String,      // JSON 中的 "inAmount"
       pub out_amount: String,     // JSON 中的 "outAmount"
       pub fee_amount: String,     // JSON 中的 "feeAmount"
       pub fee_mint: String,       // JSON 中的 "feeMint"
   }

   /// 路由计划条目结构体
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(rename_all = "camelCase")]
   pub struct JupRoutePlanEntry {
       pub jup_route_step: JupRouteStep,    // JSON 中的 "swapInfo"
       pub percent: u8,            // 百分比（示例中为 100）
   }

   /// 主 JupSwap 结构体
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(rename_all = "camelCase")]
   pub struct JupRoute {
       pub input_mint: String,              // JSON 中的 "inputMint"
       pub in_amount: String,               // JSON 中的 "inAmount"
       pub output_mint: String,             // JSON 中的 "outputMint"
       pub out_amount: String,              // JSON 中的 "outAmount"
       pub other_amount_threshold: String,  // JSON 中的 "otherAmountThreshold"
       pub swap_mode: SwapMode,             // JSON 中的 "swapMode"
       pub slippage_bps: u32,               // JSON 中的 "slippageBps"
       pub platform_fee: Option<()>,        // JSON 中的 "platformFee"（示例为 null）
       pub price_impact_pct: String,        // JSON 中的 "priceImpactPct"
       pub route_plan: Vec<JupRoutePlanEntry>,  // JSON 中的 "routePlan"
       pub score_report: Option<()>,        // JSON 中的 "scoreReport"（示例为 null）
       pub context_slot: u64,               // JSON 中的 "contextSlot"
       pub time_taken: f64,                 // JSON 中的 "timeTaken"
       pub swap_usd_value: String,          // JSON 中的 "swapUsdValue"
       pub simpler_route_used: bool,        // JSON 中的 "simplerRouteUsed"
   }

pub struct JupSwapStepInstructionBuilder {
    pub jup_url: String,

}

impl JupSwapStepInstructionBuilder {
    pub fn new(jup_url: String) -> Self {
        Self {
            jup_url,
        }
    }

    pub fn transfer_to_jup_route(&self, 
        route: &Route,
        max_slippage_bps: i32,
        other_amount_threshold: u64,
        swap_mode: SwapMode) -> anyhow::Result<JupRoute> {
            
            let mut route_plan = Vec::with_capacity(route.steps.len());

            // 转换每个路由步骤为 Jupiter 格式
            for step in &route.steps {
                let jup_route_step = JupRouteStep {
                    amm_key: step.edge.key().to_string(),
                    label: step.edge.dex.name().to_string(),
                    input_mint: step.edge.input_mint.to_string(),
                    output_mint: step.edge.output_mint.to_string(),
                    in_amount: step.in_amount.to_string(),
                    out_amount: step.out_amount.to_string(),
                    fee_amount: step.fee_amount.to_string(),
                    fee_mint: step.fee_mint.to_string(),
                };
        
                route_plan.push(JupRoutePlanEntry {
                    jup_route_step,
                    percent: 100, // 默认使用100%
                });
            }
        
            // 计算价格影响百分比
            let price_impact_pct = (route.price_impact_bps as f64 / 100.0).to_string();
        
            // 构建 JupRoute
            let jup_route = JupRoute {
                input_mint: route.input_mint.to_string(),
                in_amount: route.in_amount.to_string(),
                output_mint: route.output_mint.to_string(),
                out_amount: route.out_amount.to_string(),
                other_amount_threshold: other_amount_threshold.to_string(),
                swap_mode,
                slippage_bps: max_slippage_bps as u32,
                platform_fee: None,
                price_impact_pct,
                route_plan,
                score_report: None,
                context_slot: route.slot,
                time_taken: 0.0, // 这个值在实际场景中可能需要测量
                swap_usd_value: "0".to_string(), // 如果有USD价值可以从外部传入
                simpler_route_used: false,
            };
        
            Ok(jup_route)
        }
    

    // 辅助方法：计算最小输出金额
    fn calculate_minimum_output(&self, amount: u64, slippage: f64) -> u64 {
        ((amount as f64) * (1.0 - slippage)).floor() as u64
    }

    
}

fn request_jup_swap(
    jup_url: &str,
    route: &JupRoute,
) -> anyhow::Result<JupSwap> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(jup_url.to_owned()+"/swap-instructions")
        .json(route)
        .send().unwrap();

    // let jup_swap_response: anyhow::Result<JupSwapResponse> =
    // router_lib::utils::http_error_handling(response).await;

    // let jup_swap_response = match jup_swap_response {
    //     Ok(r) => {
    //         r
    //     },
    //     Err(e) => {
    //         return Err(anyhow::anyhow!("Jupiter swap-instructions error: {}", e));
    //     }
    // };

    // let swap = jup_swap_response.data;
    // Ok(swap)

    if response.status().is_success() {
        let response: JupSwapResponse = response.json()?;
        if let Some(error) = response.error {
            return Err(anyhow::anyhow!("Jupiter error: {}", error));
        }
        let swap = response.data;
        Ok(swap)
    } else {
        Err(anyhow::anyhow!(
            "Failed to request Jupiter swap: {}",
            response.status()
        ))
    }
}

fn transfer_to_jupswap(jup_swap:JupSwap)-> anyhow::Result<Swap>{
    

    let swap_instruction = jup_swap.swap_instruction.expect("Swap instruction is missing in the response");
  //  let compute_budget_instructions = jup_swap.compute_budget_instructions;
    let setup_instructions = jup_swap.setup_instructions;

    let other_instructions = jup_swap.other_instructions;
    let address_lookup_table_addresses = jup_swap.address_lookup_table_addresses.iter()
        .map(|s| Pubkey::from_str(s).unwrap())
        .collect::<Vec<_>>();
   // let prioritization_fee_lamports = jup_swap.prioritization_fee_lamports;
    let compute_unit_limit = jup_swap.compute_unit_limit;
    let cleanup_instructions = jup_swap.cleanup_instruction.into_iter().collect::<Vec<_>>();
    let swap = Swap {
        setup_instructions,
        swap_instruction,
        cleanup_instructions,
        cu_estimate: compute_unit_limit,
    };
    Ok(swap)
}


impl SwapInstructionsBuilder  for JupSwapStepInstructionBuilder {
    fn build_ixs(&self,
        wallet_pk: &Pubkey,
        route: &Route,
       // wrap_and_unwrap_sol: bool,
       // auto_create_out: bool,
      source_atas:  &HashMap <Pubkey, Pubkey>,
        max_slippage_bps: i32,
        other_amount_threshold: u64,
        swap_mode: SwapMode) -> anyhow::Result<Swap> {
      
        let jup_route = self.transfer_to_jup_route(route, max_slippage_bps, other_amount_threshold, swap_mode)?;
        let jup_url = self.jup_url.clone();
        let jup_swap = request_jup_swap(&jup_url, &jup_route)?;
        let swap = transfer_to_jupswap(jup_swap)?; 

        swap.setup_instructions.iter()
            .filter(|ix| {
                let mut keep = true;
                if ix.program_id == *ATA_PROGRAM_ID && ix.accounts[0].pubkey == *wallet_pk {
                    if source_atas.contains_key(&ix.accounts[1].pubkey) {
                            keep = false;
                    }
                }
                keep
            });

        Ok(swap)

        
    }

}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JupSwapResponse {
    pub data: JupSwap,
    pub error: Option<String>,
}   


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupSwap {
    pub token_ledger_instruction: Option<Instruction>,
    pub compute_budget_instructions: Vec<Instruction>,
    pub setup_instructions: Vec<Instruction>,
    pub swap_instruction: Option<Instruction>,
    pub cleanup_instruction: Option<Instruction>,
    pub other_instructions: Vec<Instruction>,
    pub address_lookup_table_addresses: Vec<String>,
    pub prioritization_fee_lamports: u64,
    pub compute_unit_limit: u32,
    pub prioritization_type: PrioritizationType,
    pub simulation_slot: Option<u64>,
    pub dynamic_slippage_report: Option<()>,
    pub simulation_error: Option<String>,
    pub addresses_by_lookup_table_address: Option<HashMap<String, Vec<String>>>,
    pub blockhash_with_metadata: BlockhashWithMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrioritizationType {
    pub compute_budget: ComputeBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputeBudget {
    pub micro_lamports: u64,
    pub estimated_micro_lamports: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockhashWithMetadata {
    pub blockhash: Vec<u8>,
    pub last_valid_block_height: u64,
    pub fetched_at: FetchedAt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedAt {
    pub secs_since_epoch: u64,
    pub nanos_since_epoch: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::Edge;
    use crate::test_utils::*;
    use router_feed_lib::router_rpc_client::RouterRpcClient;
    use router_lib::dex::{
        AccountProviderView, DexEdge, DexEdgeIdentifier, DexInterface, DexSubscriptionMode, Quote, SwapInstruction,
    };
    use std::any::Any;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use test_case::test_case;

    use crate::test_utils::*;

    struct MockSwapStepInstructionBuilder {}
    struct MockDex {}
    struct MockId {}

    impl DexEdgeIdentifier for MockId {
        fn key(&self) -> Pubkey {
            todo!()
        }

        fn desc(&self) -> String {
            todo!()
        }

        fn input_mint(&self) -> Pubkey {
            todo!()
        }

        fn output_mint(&self) -> Pubkey {
            todo!()
        }

        fn accounts_needed(&self) -> usize {
            todo!()
        }

        fn as_any(&self) -> &dyn Any {
            //todo!()
            self
        }
    }

    #[async_trait::async_trait]
    impl DexInterface for MockDex {
        async fn initialize(
            _rpc: &mut RouterRpcClient,
            _options: HashMap<String, String>,
            take_all_mints: bool,
            mints: &Vec<String>,
        ) -> anyhow::Result<Arc<dyn DexInterface>>
        where
            Self: Sized,
        {
            todo!()
        }

        fn name(&self) -> String {
            todo!()
        }

        fn subscription_mode(&self) -> DexSubscriptionMode {
            todo!()
        }

        fn edges_per_pk(&self) -> HashMap<Pubkey, Vec<Arc<dyn DexEdgeIdentifier>>> {
            todo!()
        }

        fn program_ids(&self) -> HashSet<Pubkey> {
            todo!()
        }

        fn load(
            &self,
            _id: &Arc<dyn DexEdgeIdentifier>,
            _chain_data: &AccountProviderView,
        ) -> anyhow::Result<Arc<dyn DexEdge>> {
            todo!()
        }

        fn quote(
            &self,
            _id: &Arc<dyn DexEdgeIdentifier>,
            _edge: &Arc<dyn DexEdge>,
            _chain_data: &AccountProviderView,
            _in_amount: u64,
        ) -> anyhow::Result<Quote> {
            todo!()
        }

        fn build_swap_ix(
            &self,
            _id: &Arc<dyn DexEdgeIdentifier>,
            _chain_data: &AccountProviderView,
            _wallet_pk: &Pubkey,
            _in_amount: u64,
            _out_amount: u64,
            _max_slippage_bps: i32,
        ) -> anyhow::Result<SwapInstruction> {
            todo!()
        }

        fn supports_exact_out(&self, _id: &Arc<dyn DexEdgeIdentifier>) -> bool {
            todo!()
        }

        fn quote_exact_out(
            &self,
            _id: &Arc<dyn DexEdgeIdentifier>,
            _edge: &Arc<dyn DexEdge>,
            _chain_data: &AccountProviderView,
            _out_amount: u64,
        ) -> anyhow::Result<Quote> {
            todo!()
        }
    }

    #[tokio::test]
    async fn should_build_ixs() {
        let builder = JupSwapStepInstructionBuilder::new("".to_string());
        let wallet = Pubkey::from_str("HRcxj9Vnfj86WEKDBYesHMViMWK4BLyhcLCSgh9fxC2d").unwrap();
        let input_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
        let output_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();

        let ixs = builder
            .build_ixs(
                &wallet,
                &Route {
                    input_mint: input_mint,
                    output_mint: output_mint,
                    in_amount: 1000,
                    out_amount: 2000,
                    price_impact_bps: 0,
                    slot: 0,
                    accounts: None,
                    steps: vec![RouteStep {
                        edge: Arc::new(Edge {
                            input_mint: input_mint,
                            output_mint: output_mint,
                            input_mint_symbol:1.to_string(),
                            output_mint_symbol:2.to_string(),
                            accounts_needed: 1,
                            dex: Arc::new(MockDex {}),
                            id: Arc::new(MockId {}),
                            state: Default::default(),
                        }),
                        in_amount: 1000,
                        out_amount: 2000,
                        fee_amount: 0,
                        fee_mint: Default::default(),
                    }],
                },
                &HashMap::new(),
                //false,
                0,
                0,
                SwapMode::ExactIn,
            )
            .unwrap();
    }

    #[test]
    fn test_calculate_minimum_output() {
        let builder = JupSwapStepInstructionBuilder::new("".to_string());
        let amount = 1_000_000;
        let slippage = 0.01; // 1%

        let min_output = builder.calculate_minimum_output(amount, slippage);
        assert_eq!(min_output, 990_000); // 1_000_000 * 0.99
    }

    #[test]
    fn test_transfer_to_jupswap() {
        let jup_swap = JupSwap {
            token_ledger_instruction: None,
            compute_budget_instructions: vec![],
            setup_instructions: vec![],
            swap_instruction: Some(Instruction {
                program_id: Pubkey::new_unique(),
                accounts: vec![],
                data: vec![],
            }),
            cleanup_instruction: None,
            other_instructions: vec![],
            address_lookup_table_addresses: vec![],
            prioritization_fee_lamports: 1,
            compute_unit_limit: 1400000,
            prioritization_type: PrioritizationType {
                compute_budget: ComputeBudget {
                    micro_lamports: 1,
                    estimated_micro_lamports: 1,
                },
            },
            simulation_slot: None,
            dynamic_slippage_report: None,
            simulation_error: None,
            addresses_by_lookup_table_address: None,
            blockhash_with_metadata: BlockhashWithMetadata {
                blockhash: vec![1,2,3,4],
                last_valid_block_height: 100,
                fetched_at: FetchedAt {
                    secs_since_epoch: 1000,
                    nanos_since_epoch: 0,
                },
            },
        };

        let result = transfer_to_jupswap(jup_swap);
        assert!(result.is_ok(), "JupSwap 转换应该成功");
        
        let swap = result.unwrap();
        assert_eq!(swap.cu_estimate, 1400000);
    }
}