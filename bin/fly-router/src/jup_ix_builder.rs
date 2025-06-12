use anchor_spl::metadata::mpl_token_metadata::instructions::Print;
use anyhow::Ok;
use axum::async_trait;
use axum::http::request;
use base64::engine::general_purpose;
use base64::Engine;
use router_lib::dex::SwapMode;
use serde::{Deserialize, Serialize};
use solana_client::client_error::reqwest::{self, Client, ClientBuilder};
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use warp::filters::body::json;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, trace, warn};

use crate::ix_builder::SwapInstructionsBuilder;
use crate::routing_types::{Route, RouteStep};
use crate::swap::{self, Swap};

lazy_static::lazy_static! {
    static ref ATA_PROGRAM_ID: Pubkey = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap();
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct JupRequest{
//     pub data: JupRequestData,
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupRequestData {
    pub user_public_key: String, // 用户公钥
    pub as_legacy_transaction: bool, // 是否使用旧版交易
    pub wrap_and_unwrap_sol: bool, // 是否包装和解包 SOL
    pub use_shared_accounts: bool, // 是否使用共享账户
    pub compute_unit_price_micro_lamports: u64, // 计算单元价格（微 lamports）
    pub dynamic_compute_unit_limit: bool, // 动态计算单元限制
    pub skip_user_accounts_rpc_calls: bool, // 是否跳过用户账户 RPC 调用
    pub quote_response: JupRoute,     // .
}


/// 交换信息结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupRouteStep {
    pub amm_key: String, // JSON 中的 "ammKey"
    pub label: String,
    pub input_mint: String,  // JSON 中的 "inputMint"
    pub output_mint: String, // JSON 中的 "outputMint"
    pub in_amount: String,   // JSON 中的 "inAmount"
    pub out_amount: String,  // JSON 中的 "outAmount"
    pub fee_amount: String,  // JSON 中的 "feeAmount"
    pub fee_mint: String,    // JSON 中的 "feeMint"
}

/// 路由计划条目结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupRoutePlanEntry {
    pub swap_info: JupRouteStep, // JSON 中的 "swapInfo"
    pub percent: u8,                  // 百分比（示例中为 100）
}

/// 主 JupSwap 结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupRoute {
    pub input_mint: String,                 // JSON 中的 "inputMint"
    pub in_amount: String,                  // JSON 中的 "inAmount"
    pub output_mint: String,                // JSON 中的 "outputMint"
    pub out_amount: String,                 // JSON 中的 "outAmount"
    pub other_amount_threshold: String,     // JSON 中的 "otherAmountThreshold"
    pub swap_mode: SwapMode,                // JSON 中的 "swapMode"
    pub slippage_bps: u32,                  // JSON 中的 "slippageBps"
    pub platform_fee: Option<()>,           // JSON 中的 "platformFee"（示例为 null）
    pub price_impact_pct: String,           // JSON 中的 "priceImpactPct"
    pub route_plan: Vec<JupRoutePlanEntry>, // JSON 中的 "routePlan"
    pub score_report: Option<()>,           // JSON 中的 "scoreReport"（示例为 null）
    pub context_slot: u64,                  // JSON 中的 "contextSlot"
    pub time_taken: f64,                    // JSON 中的 "timeTaken"
    pub swap_usd_value: String,             // JSON 中的 "swapUsdValue"
    pub simpler_route_used: bool,           // JSON 中的 "simplerRouteUsed"
}

pub struct JupSwapStepInstructionBuilder {
    pub jup_url: String,
    pub client: Arc<reqwest::Client>,
}

impl JupSwapStepInstructionBuilder {
    pub fn new(jup_url: String) -> Self {
        let client = ClientBuilder::new()
            // 空闲连接存活时间延长至 1 分钟（默认 30s）
            .pool_idle_timeout(Duration::from_secs(60))
            // 启用 TCP keep-alive（每 30s 发送心跳包）
            .tcp_keepalive(Some(Duration::from_secs(30)))
            // 添加超时设置
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        let client = Arc::new(client);
        Self { jup_url, client }
    }

    pub fn transfer_to_jup_route(
        &self,
        route: Arc<Route>,
        max_slippage_bps: i32,
        other_amount_threshold: u64,
        swap_mode: SwapMode,
    ) -> anyhow::Result<JupRoute> {
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
                swap_info: jup_route_step,
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
            time_taken: 0.0,                 // 这个值在实际场景中可能需要测量
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

async fn request_jup_swap(
    wallet_pk: &Pubkey,
    jup_url: &str,
    route: JupRoute,
    client: Arc<Client>,
) -> anyhow::Result<JupSwap> {
    // 准备请求体
    let request_data:JupRequestData = JupRequestData {
        user_public_key: wallet_pk.to_string(), // 替换为实际用户公钥
        as_legacy_transaction: false,
        wrap_and_unwrap_sol: false,
        use_shared_accounts: false,
        compute_unit_price_micro_lamports: 1,
        dynamic_compute_unit_limit: false,
        skip_user_accounts_rpc_calls: true,
        quote_response: route,
    };

    // let request = JupRequest {
    //     data: request_data,
    // };

    // let request_json = serde_json::to_string(&request_data).unwrap();
    // println!("JupSwap request: {}", request_json);

    
    let response = client
        .post(jup_url.to_owned() + "/swap-instructions")
        .header("Content-Type", "application/json")
        .json(&request_data)
        .send()
        .await?;

    let response_result: anyhow::Result<JupSwap> =
        router_lib::utils::http_error_handling(response).await;

    let jup_swap_response = match response_result {
        std::result::Result::Ok(r) => r,
        Err(e) => {
            error!("Error requesting Jupiter swap: {}", e);
            return Err(anyhow::anyhow!("Error requesting Jupiter swap: {}", e));
        }
    };

    let swap = jup_swap_response;
    Ok(swap)

    // if response.status().is_success() {
    //     let response: JupSwapResponse = response.json()?;
    //     if let Some(error) = response.error {
    //         return Err(anyhow::anyhow!("Jupiter error: {}", error));
    //     }
    //     let swap = response.data;
    //     Ok(swap)
    // } else {
    //     Err(anyhow::anyhow!(
    //         "Failed to request Jupiter swap: {}",
    //         response.status()
    //     ))
    // }
}

fn transfer_to_swap(jup_swap: JupSwap) -> anyhow::Result<Swap> {
    let swap_instruction = match jup_swap.swap_instruction 
        {
            Some(ix) => transfer_to_instruction(ix)?,
            None => {
                return Err(anyhow::anyhow!("Swap instruction is missing"));
            }
        };
    // let compute_budget_instructions = jup_swap.compute_budget_instructions
    //     .iter()
    //     .map(|ix| transfer_to_instruction(ix.clone()))
    //     .collect::<anyhow::Result<Vec<Instruction>>>()?;
    let setup_instructions = jup_swap.setup_instructions.iter().map(|ix| {
        transfer_to_instruction(ix.clone())
    }).collect::<anyhow::Result<Vec<Instruction>>>()?;

    //let _other_instructions = jup_swap.other_instructions;
    let address_lookup_table_addresses = jup_swap.address_lookup_table_addresses;
    // let prioritization_fee_lamports = jup_swap.prioritization_fee_lamports;
    let compute_unit_limit = jup_swap.compute_unit_limit;
   // let cleanup_instructions = jup_swap.cleanup_instruction.into_iter().collect::<Vec<_>>();
    let cleanup_instructions = jup_swap.cleanup_instruction
        .iter()
        .map(|ix| transfer_to_instruction(ix.clone()))
        .collect::<anyhow::Result<Vec<Instruction>>>()?;

    let swap = Swap {
        //compute_budget_instructions,
        setup_instructions,
        swap_instruction,
        cleanup_instructions,
        cu_estimate: compute_unit_limit,
        address_lookup_table_addresses,
    };
    Ok(swap)
}

fn transfer_to_instruction(
    instruction: InstructionString,
) -> anyhow::Result<Instruction> {
    let accounts = instruction
        .accounts
        .iter()
        .map(|account| {
            Ok(solana_program::instruction::AccountMeta {
                pubkey: Pubkey::from_str(&account.pubkey).unwrap(),
                is_signer: account.is_signer,
                is_writable: account.is_writable,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let data = match general_purpose::STANDARD.decode(&instruction.data) {
        std::result::Result::Ok(decoded) => decoded,
        Err(e) => {
            error!("Failed to decode base64 data: {}", e);
            // 这里可以选择返回一个错误，或者使用默认值
            return Err(anyhow::anyhow!("Failed to decode base64 data"));
        }
    };
    let instruction = Instruction {
        program_id: Pubkey::from_str(&instruction.program_id).unwrap(),
        accounts,
        data,
    };
    Ok(instruction)
}

#[async_trait]
impl SwapInstructionsBuilder for JupSwapStepInstructionBuilder {
    async fn build_ixs(
        &self,
        wallet_pk: &Pubkey,
        route: Arc<Route>,
        // wrap_and_unwrap_sol: bool,
        // auto_create_out: bool,
        source_atas: &HashMap<Pubkey, Pubkey>,
        max_slippage_bps: i32,
        other_amount_threshold: u64,
        swap_mode: SwapMode,
    ) -> anyhow::Result<Arc<Swap>> {
        let jup_route =
            self.transfer_to_jup_route(route, max_slippage_bps, other_amount_threshold, swap_mode)?;

        info!("JupRoute: {:?}", jup_route);
        let jup_url = self.jup_url.clone();
        let mut jup_swap = request_jup_swap(wallet_pk,&jup_url, jup_route, self.client.clone()).await?;
        info!("jup_swap: {:?}", jup_swap);
        

        if !jup_swap.setup_instructions.is_empty() {
            jup_swap.setup_instructions = jup_swap
                .setup_instructions
                .iter()
                .filter(|ix| {
                    let mut keep = true;
                    if ix.program_id == ATA_PROGRAM_ID.to_string() && ix.accounts[0].pubkey == wallet_pk.to_string() && ix.data == "AQ==".to_string() {
                        let ata = Pubkey::from_str(&ix.accounts[1].pubkey).unwrap();
                        if source_atas.contains_key(&ata) {
                            keep = false;
                        }
                    }
                    keep
                })
                .cloned()
                .collect();
        }

        info!("jup_swap: {:?}", jup_swap);

        let mut swap = transfer_to_swap(jup_swap)?;
        

        info!("swap 2 : {:?}", swap);
        Ok(Arc::new(swap))
    }
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// struct JupSwapResponse {
//     pub data: JupSwap,
//     pub error: Option<String>,
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupSwap {
    pub token_ledger_instruction: Option<InstructionString>,
    pub compute_budget_instructions: Vec<InstructionString>,
    pub setup_instructions: Vec<InstructionString>,
    pub swap_instruction: Option<InstructionString>,
    pub cleanup_instruction: Option<InstructionString>,
    pub other_instructions: Vec<InstructionString>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionString {
    pub program_id: String,
    pub accounts: Vec<InstructionAccountString>,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionAccountString {
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::Edge;
    use crate::test_utils::*;
    use router_feed_lib::router_rpc_client::RouterRpcClient;
    use router_lib::dex::{
        AccountProviderView, DexEdge, DexEdgeIdentifier, DexInterface, DexSubscriptionMode, Quote,
        SwapInstruction,
    };
    use std::any::Any;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use test_case::test_case;

    use crate::test_utils::*;

    #[tokio::test]
    async fn test_request_jup_swap_with_real_url() {
        // 使用实际的 Jupiter URL
        let jupiter_url = "http://5.10.219.2:9001";
        let wallet_pk = Pubkey::from_str("HRcxj9Vnfj86WEKDBYesHMViMWK4BLyhcLCSgh9fxC2d").unwrap();

        // 构建测试输入参数 JupRoute
        let route = JupRoute {
            input_mint: "So11111111111111111111111111111111111111112".to_string(),
            in_amount: "1000000000".to_string(),
            output_mint: "So11111111111111111111111111111111111111112".to_string(),
            out_amount: "999982411".to_string(),
            other_amount_threshold: "999982411".to_string(),
            swap_mode: SwapMode::ExactIn,
            slippage_bps: 0,
            platform_fee: None,
            price_impact_pct: "0".to_string(),
            route_plan: vec![
                JupRoutePlanEntry {
                    swap_info: JupRouteStep {
                        amm_key: "AHhiY6GAKfBkvseQDQbBC7qp3fTRNpyZccuEdYSdPFEf".to_string(),
                        label: "SolFi".to_string(),
                        input_mint: "So11111111111111111111111111111111111111112".to_string(),
                        output_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                        in_amount: "1000000000".to_string(),
                        out_amount: "146692258".to_string(),
                        fee_amount: "0".to_string(),
                        fee_mint: "So11111111111111111111111111111111111111112".to_string(),
                    },
                    percent: 100,
                },
                JupRoutePlanEntry {
                    swap_info: JupRouteStep {
                        amm_key: "AvBSC1KmFNceHpD6jyyXBV6gMXFxZ8BJJ3HVUN8kCurJ".to_string(),
                        label: "Obric V2".to_string(),
                        input_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                        output_mint: "So11111111111111111111111111111111111111112".to_string(),
                        in_amount: "146692258".to_string(),
                        out_amount: "999949840".to_string(),
                        fee_amount: "49999".to_string(),
                        fee_mint: "So11111111111111111111111111111111111111112".to_string(),
                    },
                    percent: 100,
                },
            ],
            score_report: None,
            context_slot: 337940453,
            time_taken: 0.00071139,
            swap_usd_value: "146.69775289849087620660735481".to_string(),
            simpler_route_used: false,
        };

        // 创建 reqwest 客户端
        let client = Arc::new(reqwest::Client::new());

        // 调用 request_jup_swap 方法
        let result = request_jup_swap(&wallet_pk,jupiter_url, route, client).await;

        // 验证结果
        match result {
            std::result::Result::Ok(jup_swap) => {
                println!("JupSwap Response: {:?}", jup_swap);

                // 验证返回的 JupSwap 数据
                assert!(
                    jup_swap.swap_instruction.is_some(),
                    "Swap instruction should exist"
                );
                assert!(
                    !jup_swap.compute_budget_instructions.is_empty(),
                    "Compute budget instructions should exist"
                );
            }
            Err(e) => {
                panic!("Failed to request Jupiter swap: {:?}", e);
            }
        }
    }
}
