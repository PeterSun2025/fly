use super::ix_sender::IxSender;
use crate::{
    routing_types::Route,
    server::{client_provider::ClientProvider, hash_provider::HashProvider},
    swap::Swap,
};
use anchor_lang::prelude::AccountMeta;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token::spl_token::{
    self,
    instruction::{close_account, transfer},
};
use anyhow::{anyhow, Context, Result};
use axum::async_trait;
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Local};
use clap::error;
use once_cell::sync::Lazy;
use rand::Rng;
use serde_json::{json, Value};
use solana_program::{instruction::Instruction, pubkey::Pubkey};
use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    compute_budget::ComputeBudgetInstruction,
    message::VersionedMessage,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::{Transaction, VersionedTransaction},
};
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime},
};
use tokio::time::Instant;
pub use tracing::{debug, error, info, trace, warn};

static NATIVE_MINT: Lazy<Pubkey> =
    Lazy::new(|| Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap());
const TOKEN_PROGRAM_ID: Pubkey = spl_token::ID;
static MEMO_PROGRAM_ID: Lazy<Pubkey> =
    Lazy::new(|| Pubkey::from_str("Memo1UhkJRfHyvLMcVucJwxXeuD728EqVDDwQDxFMNo").unwrap());
//const DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS: u64 = 10_000;
static JITO_TIP_ACCOUNTS: Lazy<[Pubkey; 8]> = Lazy::new(|| {
    [
        Pubkey::from_str("3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT").unwrap(),
        Pubkey::from_str("96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5").unwrap(),
        Pubkey::from_str("ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49").unwrap(),
        Pubkey::from_str("Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY").unwrap(),
        Pubkey::from_str("HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe").unwrap(),
        Pubkey::from_str("DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh").unwrap(),
        Pubkey::from_str("ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt").unwrap(),
        Pubkey::from_str("DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL").unwrap(),
    ]
});
//const JITO_MAX_TIP: u64 = 10_000_000;

pub struct JitoIxSender<THashProvider: HashProvider + Send + Sync + 'static> {
    name: String,
    keypair: Keypair,
    public_key: Pubkey,
    source_ata: Pubkey,
    alt_accounts: Vec<AddressLookupTableAccount>,
    compute_unit_price_micro_lamports: u64,
    jito_tip_bps: f32,
    jito_max_tip: u64,
    jito_regions: Vec<String>,
    region_send_type: String,
    jito_urls: Vec<String>,
    hash_provider: Arc<THashProvider>,
    client_provider: Arc<ClientProvider>,
    //send_counter: RwLock<SendCounter>,
}
#[async_trait]
impl<THashProvider: HashProvider + Send + Sync + 'static> IxSender for JitoIxSender<THashProvider> {
    async fn instructuin_extend(
        &self,
        swap: Arc<Swap>,
        route: Arc<Route>,
    ) -> anyhow::Result<HashMap<String, Vec<VersionedTransaction>>> {
        let instructions_start = Instant::now();

        let mut ixs1 = vec![];

        //let new_profit = route.out_amount - route.in_amount;
        let profit = route
            .out_amount
            .checked_sub(route.in_amount)
            .ok_or_else(|| anyhow::anyhow!("Profit calculation overflow"))?;

        let compute_unit_limit = swap.cu_estimate + 20000;

        let jito_tip = self.calculate_tip(profit, compute_unit_limit);

        let compute_budget_ixs = vec![
            ComputeBudgetInstruction::set_compute_unit_price(
                self.compute_unit_price_micro_lamports,
            ),
            ComputeBudgetInstruction::set_compute_unit_limit(compute_unit_limit),
        ];

        // 1.1 设置计算单元限制
        // ixs1.push(ComputeBudgetInstruction::set_compute_unit_limit(
        //     task_message.compute_unit_limit + 20000,
        // ));
        ixs1.extend(compute_budget_ixs);

        // 1.2 闪电贷逻辑
        // let mut flash_repay_instruction = None;
        // if self.enable_flash_loan {
        //     let (flash_borrow_ix, flash_repay_ix) = self.get_flash_loan_instructions(
        //         &task_message.wallet_pubkey,
        //         task_message.in_amount,
        //     )?;

        //     ixs1.push(flash_borrow_ix);
        //     flash_repay_instruction = Some(flash_repay_ix);
        // }

        // 1.3 添加设置指令
        // let setup_instructions: Vec<Instruction> = swap.setup_instructions
        //     .iter()
        //     .filter_map(|ix| self.format_instruction(ix))
        //     .collect();
        ixs1.extend(swap.setup_instructions.clone());

        // 1.4 添加交换指令
        // if let Some(swap_ix) = self.format_instruction(&swap.swap_instruction) {
        //     ixs1.push(swap_ix);
        // }
        ixs1.push(swap.swap_instruction.clone());

        // 1.5 添加闪电贷还款指令
        // if let Some(repay_ix) = flash_repay_instruction {
        //     ixs1.push(repay_ix);
        // }

        // 1.6 创建目标钱包和ATA
        let destination_keypair = Keypair::new();
        let destination_ata =
            get_associated_token_address(&NATIVE_MINT, &destination_keypair.pubkey());

        // 1.7 创建ATA指令
        ixs1.push(create_associated_token_account_idempotent(
            &self.keypair.pubkey(),
            &destination_ata,
            &destination_keypair.pubkey(),
            &NATIVE_MINT,
        ));

        // 1.8 转移WSOL指令
        ixs1.push(transfer(
            &TOKEN_PROGRAM_ID,
            &self.source_ata,
            &destination_ata,
            &self.keypair.pubkey(),
            &[&self.keypair.pubkey()],
            jito_tip + 10000,
        )?);

        // 1.9 转移SOL指令
        ixs1.push(system_instruction::transfer(
            &self.keypair.pubkey(),
            &destination_keypair.pubkey(),
            2039280 + 5000,
        ));

        // 构建第二个交易的指令
        let mut ixs2 = vec![];

        // 2.1 CU限制
        ixs2.push(ComputeBudgetInstruction::set_compute_unit_limit(5000));

        // 2.2 关闭WSOL账户
        ixs2.push(close_account(
            &TOKEN_PROGRAM_ID,
            &destination_ata,
            &destination_keypair.pubkey(),
            &destination_keypair.pubkey(),
            &[],
        )?);

        // 2.3 发送小费
        let random_index = rand::thread_rng().gen_range(0, JITO_TIP_ACCOUNTS.len());
        ixs2.push(system_instruction::transfer(
            &destination_keypair.pubkey(),
            &JITO_TIP_ACCOUNTS[random_index],
            jito_tip,
        ));

        // 2.4 返还SOL
        ixs2.push(system_instruction::transfer(
            &destination_keypair.pubkey(),
            &self.keypair.pubkey(),
            2039280 * 2 + 10000,
        ));

        // 获取区块hash
        let recent_blockhash = self.hash_provider.get_latest_hash().await?;

        let tx2_v0_message = solana_sdk::message::v0::Message::try_compile(
            &destination_keypair.pubkey(),
            ixs2.as_slice(),
            self.alt_accounts.as_slice(),
            recent_blockhash,
        )?;
        let tx2_message = VersionedMessage::V0(tx2_v0_message);
        let tx2 = VersionedTransaction::try_new(tx2_message, &[&destination_keypair])?;
        //tx2.sign(&[&destination_keypair], recent_blockhash);

        // 根据发送类型构建交易
        let mut transactions = HashMap::new();

        if self.region_send_type == "serial" {
            // 添加memo
            if !self.name.is_empty() {
                ixs1.push(self.create_memo_instruction(&self.name));
            }

            let tx1_v0_message = solana_sdk::message::v0::Message::try_compile(
                &self.public_key,
                ixs1.as_slice(),
                self.alt_accounts.as_slice(),
                recent_blockhash,
            )?;
            let tx1_message = VersionedMessage::V0(tx1_v0_message);
            let tx1 = VersionedTransaction::try_new(tx1_message, &[&self.keypair])?;
            //tx1.partial_sign(&[&self.keypair], recent_blockhash);

            //let mut tx1 = Transaction::new_with_payer(&ixs1, Some(&self.public_key));

            //let mut tx2 = Transaction::new_with_payer(&ixs2, Some(&destination_keypair.pubkey()));
            transactions.insert("serial".to_string(), vec![tx1, tx2]);
        } else {
            for url in self.jito_urls.iter() {
                let mut ixs1_copy = ixs1.clone();
                let tx2_copy = tx2.clone();
                let mut name = String::new();
                if !self.name.is_empty() {
                    name = self.name.clone() + "-" + url;
                } else {
                    name = url.clone();
                }
                ixs1_copy.push(self.create_memo_instruction(&name));

                let tx1_v0_message = solana_sdk::message::v0::Message::try_compile(
                    &self.public_key,
                    ixs1_copy.as_slice(),
                    self.alt_accounts.as_slice(),
                    recent_blockhash,
                )?;
                let tx1_message = VersionedMessage::V0(tx1_v0_message);
                let tx1 = VersionedTransaction::try_new(tx1_message, &[&self.keypair])?;

                transactions.insert(url.clone(), vec![tx1, tx2_copy]);
            }
        }

        Ok(transactions)
    }

    async fn send_tx(
        &self,
        transactions: HashMap<String, Vec<VersionedTransaction>>,
    ) -> anyhow::Result<()> {
        let _ = self.send_transaction_to_jito(transactions).await;
        Ok(())
    }
}

impl<THashProvider: HashProvider + Send + Sync + 'static> JitoIxSender<THashProvider> {
    pub fn new(
        name: String,
        keypair: Keypair,
        alt_accounts: Vec<AddressLookupTableAccount>,
        compute_unit_price_micro_lamports: u64,
        jito_tip_bps: f32,
        jito_max_tip: u64,
        jito_regions: Vec<String>,
        region_send_type: String,
        hash_provider: Arc<THashProvider>,
        client_provider: Arc<ClientProvider>,
    ) -> Self {
        let source_ata = get_associated_token_address(&keypair.pubkey(), &NATIVE_MINT);
        let public_key = keypair.pubkey();
        let jito_urls = jito_regions
            .iter()
            .map(|region| {
                format!(
                    "https://{}.mainnet.block-engine.jito.wtf/api/v1/bundles",
                    region
                )
            })
            .collect::<Vec<_>>();

        Self {
            name,
            keypair,
            public_key,
            source_ata,
            alt_accounts,
            compute_unit_price_micro_lamports,
            jito_tip_bps,
            jito_max_tip,
            jito_regions,
            region_send_type,
            jito_urls,
            hash_provider,
            client_provider,
            //send_counter: RwLock::new(SendCounter::new(keypair.pubkey().to_string(), 10, jito_urls.clone())),
        }
    }

    // 发送交易到JITO

    // async fn send_request(&self, endpoint: &str, method: &str, params: Option<Value>) -> Result<Value, reqwest::Error> {
    //     let url = format!("{}{}", self.base_url, endpoint);

    //     let data = json!({
    //         "jsonrpc": "2.0",
    //         "id": 1,
    //         "method": method,
    //         "params": params.unwrap_or(json!([]))
    //     });

    //     // println!("Sending request to: {}", url);
    //     // println!("Request body: {}", serde_json::to_string_pretty(&data).unwrap());

    //     let response = self.client
    //         .post(&url)
    //         .header("Content-Type", "application/json")
    //         .json(&data)
    //         .send()
    //         .await?;

    //     let status = response.status();
    //     println!("Response status: {}", status);

    //     let body = response.json::<Value>().await?;
    //     println!("Response body: {}", serde_json::to_string_pretty(&body).unwrap());

    //     Ok(body)
    // }

    async fn send_transaction_to_jito(
        &self,
        transactions: HashMap<String, Vec<VersionedTransaction>>,
    ) -> anyhow::Result<()> {
        let send_start = Instant::now();

        match self.region_send_type.as_str() {
            "serial" => {
                if let Some(txs) = transactions.get("serial") {
                    let body = json!({
                        "id": 1,
                        "jsonrpc": "2.0",
                        "method": "sendBundle",
                        "params": [
                            txs.iter()
                                .map(|tx| general_purpose::STANDARD.encode(bincode::serialize(&tx).unwrap()))
                                .collect::<Vec<_>>(),
                            {"encoding": "base64"}
                        ]
                    });

                    let jito_url = &self.jito_urls[self.get_next_jito_url_index()];

                    // 异步发送请求
                    //let send_counter = self.send_counter.clone();

                    let clinet_index = self.client_provider.get_next_clinet_index();
                    let client = self.client_provider.get_next_client_by_index(clinet_index);
                    let url = jito_url.clone();
                    tokio::spawn(async move {
                        match client.post(&url).json(&body).send().await {
                            Ok(response) => {
                                //let duration = send_start.elapsed();
                                // let mut send_counter = self.send_counter.write().unwrap();
                                // send_counter.send_success(duration.as_nanos() as u64, &url).await;

                                let status = response.status();
                                let body = response.json::<Value>().await.unwrap();
                                info!(
                                    "Response status: {} || Response body: {}",
                                    status,
                                    serde_json::to_string_pretty(&body).unwrap(),
                                );
                                let bundle_uuid = body["result"]
                                    .as_str()
                                    .ok_or_else(|| {
                                        //error!("Failed to get bundle UUID from response: {}", body);
                                        anyhow!("Failed to get bundle UUID from response")
                                    })
                                    .unwrap();
                                info!("Response status: {} || Bundle sent with UUID: {} || sender time {}", 
                                        status,bundle_uuid,send_start.elapsed().as_millis());
                            }
                            Err(e) => {
                                // let mut send_counter = self.send_counter.write().unwrap();
                                // let duration = send_start.elapsed();
                                // send_counter.send_error(e, duration.as_nanos() as u64, &url).await;
                                if let Some(status) = e.status() {
                                    match status.as_u16() {
                                        429 => {
                                            // 处理429错误
                                            error!(
                                                "Received 429 Too Many Requests from Jito: {}",
                                                e
                                            );
                                        }
                                        400 => {
                                            // 处理400错误
                                            error!("Received 400 Bad Request from Jito: {}", e);
                                        }
                                        _ => {
                                            // 处理其他错误
                                            error!("Received {} from Jito: {}", status, e);
                                        }
                                    }
                                } else {
                                    error!("Error sending transaction to Jito: {}", e);
                                }
                            }
                        }
                    });
                }
            }
            "parallel" => {
                for url in &self.jito_urls {
                    if let Some(txs) = transactions.get(url) {
                        let body = json!({
                            "id": 1,
                            "jsonrpc": "2.0",
                            "method": "sendBundle",
                            "params": [
                                txs.iter()
                                    .map(|tx| general_purpose::STANDARD.encode(bincode::serialize(&tx).unwrap()))
                                    .collect::<Vec<_>>(),
                                {"encoding": "base64"}
                            ]
                        });

                        let clinet_index = self.client_provider.get_next_clinet_index();
                        let client = self.client_provider.get_next_client_by_index(clinet_index);

                        let url = url.clone();
                        tokio::spawn(async move {
                            match client.post(&url).json(&body).send().await {
                                Ok(response) => {
                                    // let duration = send_start.elapsed();
                                    // let mut send_counter = self.send_counter.write().unwrap();
                                    // send_counter.send_success(duration.as_nanos() as u64, &url).await;
                                    let status = response.status();
                                    let body = response.json::<Value>().await.unwrap();
                                    let bundle_uuid = body["result"]
                                        .as_str()
                                        .ok_or_else(|| {
                                            error!(
                                                "Failed to get bundle UUID from response: {}",
                                                body
                                            );
                                            anyhow!("Failed to get bundle UUID from response")
                                        })
                                        .unwrap();
                                    info!("Response status: {} || Bundle sent with UUID: {} || sender time {}", 
                                        status,bundle_uuid,send_start.elapsed().as_millis());
                                }
                                Err(e) => {
                                    // let duration = send_start.elapsed();
                                    // let mut send_counter = self.send_counter.write().unwrap();
                                    // send_counter.send_error(e, duration.as_nanos() as u64, &url).await;

                                    if let Some(status) = e.status() {
                                        match status.as_u16() {
                                            429 => {
                                                // 处理429错误
                                                error!(
                                                    "Received 429 Too Many Requests from Jito: {}",
                                                    e
                                                );
                                            }
                                            400 => {
                                                // 处理400错误
                                                error!("Received 400 Bad Request from Jito: {}", e);
                                            }
                                            _ => {
                                                // 处理其他错误
                                                error!("Received {} from Jito: {}", status, e);
                                            }
                                        }
                                    } else {
                                        error!("Error sending transaction to Jito: {}", e);
                                    }
                                }
                            }
                        });
                    }
                }
            }
            _ => panic!("Invalid region send type"),
        }
        Ok(())
    }

    // 格式化指令
    // fn format_instruction(&self, instruction: &RawInstruction) -> Option<Instruction> {
    //     // 对于ATA创建指令的特殊处理
    //     if instruction.program_id == "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
    //        && instruction.data == "AQ=="
    //        && instruction.accounts[0].pubkey == self.keypair.pubkey().to_string() {

    //         if self.ata_list.contains(&instruction.accounts[1].pubkey) {
    //             return None;
    //         }
    //     }

    //     Some(Instruction {
    //         program_id: Pubkey::from(&instruction.program_id).unwrap(),
    //         accounts: instruction.accounts.iter().map(|acc| {
    //             AccountMeta {
    //                 pubkey: Pubkey::from(&acc.pubkey).unwrap(),
    //                 is_signer: acc.is_signer,
    //                 is_writable: acc.is_writable,
    //             }
    //         }).collect(),
    //         data: base64::engine::general_purpose::STANDARD.decode(&instruction.data).unwrap(),
    //     })
    // }

    fn get_next_jito_url_index(&self) -> usize {
        static JITO_URL_INDEX: AtomicUsize = AtomicUsize::new(0);
        let current = JITO_URL_INDEX.fetch_add(1, Ordering::Relaxed);
        current % self.jito_urls.len()
    }

    fn calculate_tip(&self, profit: u64, compute_unit_limit: u32) -> u64 {
        // JITO_TIP_BPS 是从配置中获取的常量
        if self.jito_tip_bps == 0.0 {
            // 默认使用 CU 的 4.5 倍作为基准
            let cu_tip = (compute_unit_limit as f64 * 4.5) as u64;
            let profit_tip = (profit as f64 * 0.65) as u64;
            cu_tip.min(profit_tip)
        } else {
            // 使用配置的 TIP 比例
            let compute_tip = (profit as f64 * self.jito_tip_bps as f64) as u64;
            compute_tip.min(self.jito_max_tip)
        }
    }

    fn create_memo_instruction(&self, memo: &str) -> Instruction {
        Instruction::new_with_bytes(*MEMO_PROGRAM_ID, memo.as_bytes(), vec![])
    }
}

// // 结构体定义
// #[derive(Clone, Debug, Default)]
// struct SendCounter {
//     address: String,
//     interval: u64,
//     opportunity: u64,
//     total_time: u64,
//     quote_time: u64,
//     swap_inst_time: u64,
//     instruction_time: u64,
//     send_url_map: HashMap<String, SendTimes>,
//     mutex: Arc<Mutex<()>>,
// }

// #[derive(Debug, Default)]
// struct SendTimes {
//     send_time: u64,
//     send_err: u64,
//     send_429_err: u64,
//     send_400_err: u64,
//     send_other_err: u64,
//     send_count: u64,
// }

// impl SendCounter {
//     fn new(address: String, interval: u64,jito_urls: Vec<String>,) -> Self {
//         let mut send_url_map = HashMap::new();

//         // 初始化 JITO URLs 的统计数据
//         for url in jito_urls.iter() {
//             send_url_map.insert(url.to_string(), SendTimes::default());
//         }

//         Self {
//             address,
//             interval,
//             opportunity: 0,
//             total_time: 0,
//             quote_time: 0,
//             swap_inst_time: 0,
//             instruction_time: 0,
//             send_url_map,
//             mutex: Arc::new(Mutex::new(())),
//         }
//     }

//     async fn add_opportunity(&mut self, total_time: u64, quote_time: u64, swap_inst_time: u64, instruction_time: u64) {
//         let _lock = self.mutex.lock().unwrap();
//         self.opportunity += 1;
//         self.total_time += total_time;
//         self.quote_time += quote_time;
//         self.swap_inst_time += swap_inst_time;
//         self.instruction_time += instruction_time;
//     }

//     async fn send_success(&mut self, time: u64, url: &str) {
//         let _lock = self.mutex.lock().unwrap();
//         if let Some(times) = self.send_url_map.get_mut(url) {
//             times.send_count += 1;
//             times.send_time += time;
//         }
//     }

//     async fn send_error(&mut self, error: reqwest::Error, time: u64, url: &str) {
//         let _lock = self.mutex.lock().unwrap();
//         if let Some(times) = self.send_url_map.get_mut(url) {
//             times.send_time += time;
//             times.send_err += 1;
//             times.send_count += 1;

//             if let Some(status) = error.status() {
//                 match status.as_u16() {
//                     429 => times.send_429_err += 1,
//                     400 => times.send_400_err += 1,
//                     _ => times.send_other_err += 1,
//                 }
//             } else {
//                 times.send_other_err += 1;
//             }
//         }
//     }
// }

// // 工具函数
// fn date_format(format: &str, date: DateTime<Local>) -> String {
//     date.format(format).to_string()
// }
