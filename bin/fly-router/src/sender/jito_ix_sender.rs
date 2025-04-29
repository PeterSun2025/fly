use anchor_lang::prelude::AccountMeta;
use anchor_spl::token::spl_token::{self, instruction::{close_account, transfer}};
use rand::Rng;
use solana_program::pubkey::Pubkey;

use solana_program::instruction::Instruction;

// use spl_token::{
//     instruction::{close_account, initialize_account3, transfer},
//     state::Account as TokenAccount,
// };
use anchor_spl::associated_token::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tokio::{
    sync::mpsc,
    time::{self, Instant},
};
use serde::{Deserialize, Serialize};

use crate::{server::hash_provider::HashProvider, swap::Swap};

use super::ix_sender::IxSender;


const NATIVE_MINT: Pubkey = Pubkey::from_str(String::from("So"))?;
const TOKEN_PROGRAM_ID: Pubkey = spl_token::ID;
const MEMO_PROGRAM_ID: Pubkey = solana_sdk::memo::id();
const DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS: u64 = 10_000;
const JITO_TIP_ACCOUNTS: [Pubkey; 8] = [
    Pubkey::from_str("3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT")?,
    Pubkey::from_str("96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5")?,
    Pubkey::from_str("ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49")?,
    Pubkey::from_str("Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY")?,
    Pubkey::from_str("HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe")?,
    Pubkey::from_str("DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh")?,
    Pubkey::from_str("ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt")?,
    Pubkey::from_str("DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL")?,
];

pub struct JitoIxSender <THashProvider: HashProvider + Send + Sync + 'static,> {
    name: String,
    keypair: Keypair,
    compute_unit_price_micro_lamports: u64,
    jito_tip_bps: i16,
    jito_max_tip: u64,
    jito_regions: Vec<String>,
    region_send_type: String,
    jito_urls: Vec<String>,
    hash_provider: Arc<THashProvider>,
}

impl<THashProvider: HashProvider + Send + Sync + 'static> IxSender for JitoIxSender<THashProvider> {

    async fn  instructuin_extend(&self, swap: &Swap) -> anyhow::Result<HashMap<String, Vec<Transaction>>> {
        let instructions_start = Instant::now();

        let mut ixs1 = vec![];


        let testr = String::from("So");
        let testr_pubkey = Pubkey::from_str(&testr).unwrap();
        

        let compute_budget_ixs = vec![
            ComputeBudgetInstruction::set_compute_unit_price(self.compute_unit_price_micro_lamports),
            ComputeBudgetInstruction::set_compute_unit_limit(swap.cu_estimate + 20000),
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
        ixs1.extend(swap.setup_instructions);

        // 1.4 添加交换指令
        // if let Some(swap_ix) = self.format_instruction(&swap.swap_instruction) {
        //     ixs1.push(swap_ix);
        // }
        ixs1.push(swap.swap_instruction);

        // 1.5 添加闪电贷还款指令
        // if let Some(repay_ix) = flash_repay_instruction {
        //     ixs1.push(repay_ix);
        // }

        // 1.6 创建目标钱包和ATA
        let destination_keypair = Keypair::new();
        let destination_ata = get_associated_token_address(
            &NATIVE_MINT,
            &destination_keypair.pubkey()
        );

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
            task_message.jito_tip + 10000,
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
        let random_index = rand::thread_rng().gen_range(0..JITO_TIP_ACCOUNTS.len());
        ixs2.push(system_instruction::transfer(
            &destination_keypair.pubkey(),
            &JITO_TIP_ACCOUNTS[random_index],
            task_message.jito_tip,
        ));

        // 2.4 返还SOL
        ixs2.push(system_instruction::transfer(
            &destination_keypair.pubkey(), 
            &self.keypair.pubkey(),
            2039280 * 2 + 10000,
        ));

        // 获取区块hash
        let recent_blockhash = self.hash_provider.get_latest_hash().await?;

        // 根据发送类型构建交易
        let mut transactions = HashMap::new();
        
        if self.region_send_type == "serial" {
            // 添加memo
            if !self.name.is_empty() {
                ixs1.push(self.create_memo_instruction(&self.name));
            }

            let mut tx1 = Transaction::new_with_payer(&ixs1, Some(&self.keypair.pubkey()));
            let mut tx2 = Transaction::new_with_payer(&ixs2, Some(&destination_keypair.pubkey()));
            
            tx1.partial_sign(&[&self.keypair], recent_blockhash);
            tx2.sign(&[&destination_keypair], recent_blockhash);

            transactions.insert("serial".to_string(), vec![tx1, tx2]);
        } else {
            // parallel sending logic
            // ...
        }

        Ok(transactions)
    }

    fn create_memo_instruction(&self, memo: &str) -> Instruction {
        Instruction::new_with_bytes(
            MEMO_PROGRAM_ID,
            memo.as_bytes(),
            vec![],
        )
    }

    async fn send_tx(&self, swaps: HashMap<String, Vec<Transaction>>) -> anyhow::Result<()> {
        todo!()
    }
    
    // 其他辅助方法...
}

impl JitoIxSender {
    // 发送交易到JITO
    async fn send_transaction_to_jito(&self, transactions: HashMap<String, Vec<Transaction>>) {
        let send_start = Instant::now();

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .unwrap();

        match self.region_send_type.as_str() {
            "serial" => {
                if let Some(txs) = transactions.get("serial") {
                    let body = json!({
                        "id": 1,
                        "jsonrpc": "2.0", 
                        "method": "sendBundle",
                        "params": [
                            txs.iter()
                                .map(|tx| base64::encode(tx.serialize()))
                                .collect::<Vec<_>>(),
                            {"encoding": "base64"}
                        ]
                    });

                    let jito_url = &self.jito_urls[self.get_next_jito_url_index()];

                    // 异步发送请求
                    let send_counter = self.send_counter.clone();
                    let url = jito_url.clone();
                    tokio::spawn(async move {
                        match client.post(&url).json(&body).send().await {
                            Ok(_) => {
                                let duration = send_start.elapsed();
                                send_counter.send_success(duration.as_nanos() as u64, &url).await;
                            }
                            Err(e) => {
                                let duration = send_start.elapsed();
                                send_counter.send_error(e, duration.as_nanos() as u64, &url).await;
                            }
                        }
                    });
                }
            }
            "parallel" => {
                for url in &self.jito_urls {
                    for i in 0..self.jito_region_per_send {
                        if let Some(txs) = transactions.get(&format!("{}{}", url, i)) {
                            let body = json!({
                                "id": 1,
                                "jsonrpc": "2.0",
                                "method": "sendBundle",
                                "params": [
                                    txs.iter()
                                        .map(|tx| base64::encode(tx.serialize()))
                                        .collect::<Vec<_>>(),
                                    {"encoding": "base64"}
                                ]
                            });

                            let client = client.clone();
                            let send_counter = self.send_counter.clone();
                            let url = url.clone();
                            tokio::spawn(async move {
                                match client.post(&url).json(&body).send().await {
                                    Ok(_) => {
                                        let duration = send_start.elapsed();
                                        send_counter.send_success(duration.as_nanos() as u64, &url).await;
                                    }
                                    Err(e) => {
                                        let duration = send_start.elapsed();
                                        send_counter.send_error(e, duration.as_nanos() as u64, &url).await;
                                    }
                                }
                            });
                        }
                    }
                }
            }
            _ => panic!("Invalid region send type")
        }
    }

    // 格式化指令
    fn format_instruction(&self, instruction: &RawInstruction) -> Option<Instruction> {
        // 对于ATA创建指令的特殊处理
        if instruction.program_id == "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL" 
           && instruction.data == "AQ=="
           && instruction.accounts[0].pubkey == self.keypair.pubkey().to_string() {
            
            if self.ata_list.contains(&instruction.accounts[1].pubkey) {
                return None;
            }
        }

        Some(Instruction {
            program_id: Pubkey::from(&instruction.program_id).unwrap(),
            accounts: instruction.accounts.iter().map(|acc| {
                AccountMeta {
                    pubkey: Pubkey::from(&acc.pubkey).unwrap(),
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                }
            }).collect(),
            data: base64::engine::general_purpose::STANDARD.decode(&instruction.data).unwrap(),
        })
    }

    fn get_next_jito_url_index(&self) -> usize {
        static JITO_URL_INDEX: AtomicUsize = AtomicUsize::new(0);
        let current = JITO_URL_INDEX.fetch_add(1, Ordering::Relaxed);
        current % self.jito_urls.len()
    }
}

#[derive(Debug, Deserialize)]
struct RawInstruction {
    program_id: String,
    accounts: Vec<RawAccountMeta>,
    data: String,
}

#[derive(Debug, Deserialize)]
struct RawAccountMeta {
    pubkey: String,
    is_signer: bool,
    is_writable: bool,
}



// 结构体定义
#[derive(Debug)]
struct SendCounter {
    address: String,
    interval: u64,
    opportunity: u64,
    total_time: u64,
    quote_time: u64,
    swap_inst_time: u64,
    instruction_time: u64,
    send_url_map: HashMap<String, SendTimes>,
    mutex: Arc<Mutex<()>>,
}

#[derive(Debug, Default)]
struct SendTimes {
    send_time: u64,
    send_err: u64,
    send_429_err: u64,
    send_400_err: u64, 
    send_other_err: u64,
    send_count: u64,
}

impl SendCounter {
    fn new(address: String, interval: u64,jito_urls: Vec<String>,) -> Self {
        let mut send_url_map = HashMap::new();
        
        // 初始化 JITO URLs 的统计数据
        for url in jito_urls.iter() {
            send_url_map.insert(url.to_string(), SendTimes::default());
        }

        Self {
            address,
            interval,
            opportunity: 0,
            total_time: 0,
            quote_time: 0,
            swap_inst_time: 0,
            instruction_time: 0,
            send_url_map,
            mutex: Arc::new(Mutex::new(())),
        }
    }

    async fn add_opportunity(&mut self, total_time: u64, quote_time: u64, swap_inst_time: u64, instruction_time: u64) {
        let _lock = self.mutex.lock().unwrap();
        self.opportunity += 1;
        self.total_time += total_time;
        self.quote_time += quote_time; 
        self.swap_inst_time += swap_inst_time;
        self.instruction_time += instruction_time;
    }

    async fn send_success(&mut self, time: u64, url: &str) {
        let _lock = self.mutex.lock().unwrap();
        if let Some(times) = self.send_url_map.get_mut(url) {
            times.send_count += 1;
            times.send_time += time;
        }
    }

    async fn send_error(&mut self, error: reqwest::Error, time: u64, url: &str) {
        let _lock = self.mutex.lock().unwrap();
        if let Some(times) = self.send_url_map.get_mut(url) {
            times.send_time += time;
            times.send_err += 1;
            times.send_count += 1;

            if let Some(status) = error.status() {
                match status.as_u16() {
                    429 => times.send_429_err += 1,
                    400 => times.send_400_err += 1,
                    _ => times.send_other_err += 1,
                }
            } else {
                times.send_other_err += 1;
            }
        }
    }
}

// 工具函数
fn date_format(format: &str, date: DateTime<Local>) -> String {
    date.format(format).to_string()
}