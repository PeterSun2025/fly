use axum::Json;
use router_lib::model::swap_response::{InstructionResponse, SwapIxResponse};
use serde_json::Value;
use solana_program::address_lookup_table::AddressLookupTableAccount;
use solana_program::message::VersionedMessage;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::signature::NullSigner;
use solana_sdk::transaction::VersionedTransaction;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

use crate::alt::alt_optimizer;
use crate::ix_builder::SwapInstructionsBuilder;
use crate::sender::ix_sender::IxSender;
use crate::prelude::*;
use crate::routing_types::Route;
use crate::server::alt_provider::AltProvider;
use crate::server::errors::AppError;
use crate::server::hash_provider::HashProvider;
use crate::swap::Swap;
use crate::util::tokio_spawn;

use router_config_lib::Config;
use router_lib::dex::{AccountProvider, SwapMode};

// make sure the transaction can be executed
const MAX_ACCOUNTS_PER_TX: usize = 64;
const MAX_TX_SIZE: usize = 1232;
const DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS: u64 = 10_000;

#[derive(Default)]
struct SenderExecutorState {
    // 是否准备好
    pub is_ready: bool,
}

pub struct SenderExecutor<
    THashProvider: HashProvider + Send + Sync + 'static,
    TAltProvider: AltProvider + Send + Sync + 'static,
    TAccountProvider: AccountProvider + Send + Sync + 'static,
    TIxBuilder: SwapInstructionsBuilder + Send + Sync + 'static,
    TIxSender: IxSender + Send + Sync + 'static,
> {
    // 准备就绪信号发送器
    // ready_sender: async_channel::Sender<()>,
    wallet_pk: Pubkey,

    swap_mode: SwapMode,

    wrap_and_unwrap_sol: bool,

    compute_unit_price_micro_lamports: u64,

    auto_create_out_ata: bool,

    slippage_bps: i32,

    alt_accounts: Vec<AddressLookupTableAccount>,

    hash_provider: Arc<THashProvider>,

    alt_provider: Arc<TAltProvider>,

    account_provider: Arc<TAccountProvider>,

    ix_builder: Arc<TIxBuilder>,

    ix_sender: Arc<TIxSender>,

    pub state: SenderExecutorState,
}

impl<
        THashProvider: HashProvider + Send + Sync + 'static,
        TAltProvider: AltProvider + Send + Sync + 'static,
        TAccountProvider: AccountProvider + Send + Sync + 'static,
        TIxBuilder: SwapInstructionsBuilder + Send + Sync + 'static,
        TIxSender: IxSender + Send + Sync + 'static,
    > SenderExecutor<THashProvider, TAltProvider, TAccountProvider, TIxBuilder,TIxSender>
{
    pub async fn new(
        config: &Config,
        hash_provider: Arc<THashProvider>,
        alt_provider: Arc<TAltProvider>,
        account_provider: Arc<TAccountProvider>,
        ix_builder: Arc<TIxBuilder>,
        ix_sender: Arc<TIxSender>,
    ) -> Self {
        let state = SenderExecutorState::default();
        let address_lookup_table_addresses = match &config.sender.lookup_tables {
            Some(lookup_tables) => lookup_tables
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>(),
            None => vec![],
        };

        let alt_accounts =
            Self::load_all_alts(address_lookup_table_addresses.clone(), alt_provider.clone()).await;

        let wallet_pk = match Pubkey::from_str(&config.sender.wallet_pk) {
            Ok(pk) => pk,
            Err(e) => {
                panic!("Failed to parse wallet public key: {:?}", e);
            }
        }; //TODO 需要改为输入私钥和密钥

        let swap_mode = SwapMode::ExactIn; //固定输入模式

        let wrap_and_unwrap_sol = config.sender.wrap_and_unwrap_sol.unwrap_or(false); // 是否需要包裹和解包SOL

        let compute_unit_price_micro_lamports = config
            .sender
            .compute_unit_price_micro_lamports
            .unwrap_or(DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS); // 计算单元价格
        let auto_create_out_ata = config.sender.auto_create_out_ata.unwrap_or(false); // 是否自动创建输出ATA
        let slippage_bps = config.sender.slippage_bps.unwrap_or(0); // 滑点设置

        Self {
            wallet_pk,
            swap_mode,
            wrap_and_unwrap_sol,
            compute_unit_price_micro_lamports,
            auto_create_out_ata,
            slippage_bps,
            alt_accounts,
            hash_provider,
            alt_provider,
            account_provider,
            ix_builder,
            ix_sender,
            state,
        }
    }

    async fn build_swap_tx(&self, route: &Route) -> anyhow::Result<Swap> {

        let ixs = self.ix_builder.build_ixs(
            &self.wallet_pk,
            &route,
            self.wrap_and_unwrap_sol,
            self.auto_create_out_ata,
            self.slippage_bps, //input.quote_response.slippage_bps,
            0,                 // input.quote_response.other_amount_threshold.parse()?,
            self.swap_mode,
        );

        ixs
        // let transaction_addresses = ixs.accounts().into_iter().collect();
        // //let all_alts = Self::load_all_alts(self.address_lookup_table_addresses, self.alt_provider).await;
        // let alts = alt_optimizer::get_best_alt(&self.alt_accounts, &transaction_addresses)?;

        // let swap_ix = InstructionResponse::from_ix(ixs.swap_instruction)?;
        // let setup_ixs: anyhow::Result<Vec<_>> = ixs
        //     .setup_instructions
        //     .into_iter()
        //     .map(|x| InstructionResponse::from_ix(x))
        //     .collect();
        // let cleanup_ixs: anyhow::Result<Vec<_>> = ixs
        //     .cleanup_instructions
        //     .into_iter()
        //     .map(|x| InstructionResponse::from_ix(x))
        //     .collect();

        // let compute_budget_ixs = vec![
        //     InstructionResponse::from_ix(ComputeBudgetInstruction::set_compute_unit_price(
        //         self.compute_unit_price_micro_lamports,
        //     ))?,
        //     InstructionResponse::from_ix(ComputeBudgetInstruction::set_compute_unit_limit(
        //         ixs.cu_estimate,
        //     ))?,
        // ];

        // let json_response = serde_json::json!(SwapIxResponse {
        //     token_ledger_instruction: None,
        //     compute_budget_instructions: Some(compute_budget_ixs),
        //     setup_instructions: Some(setup_ixs?),
        //     swap_instruction: swap_ix,
        //     cleanup_instructions: Some(cleanup_ixs?),
        //     address_lookup_table_addresses: Some(alts.iter().map(|x| x.key.to_string()).collect()),
        // });

        // Ok(Json(json_response))
    }

    async fn load_all_alts(
        address_lookup_table_addresses: Vec<String>,
        alt_provider: Arc<TAltProvider>,
    ) -> Vec<AddressLookupTableAccount> {
        let mut all_alts = vec![];
        for alt in address_lookup_table_addresses {
            match alt_provider.get_alt(Pubkey::from_str(&alt).unwrap()).await {
                Ok(alt) => all_alts.push(alt),
                Err(_) => {}
            }
        }
        all_alts
    }
}

pub async fn spawn_sender_executor_job<
    THashProvider: HashProvider + Send + Sync + 'static,
    TAltProvider: AltProvider + Send + Sync + 'static,
    TAccountProvider: AccountProvider + Send + Sync + 'static,
    TIxBuilder: SwapInstructionsBuilder + Send + Sync + 'static,
    TIxSender: IxSender + Send + Sync + 'static,
>(
    config: &Config,
    hash_provider: Arc<THashProvider>,
    alt_provider: Arc<TAltProvider>,
    account_provider: Arc<TAccountProvider>,
    ix_builder: Arc<TIxBuilder>,
    ix_sender: Arc<TIxSender>,
    route_receiver: async_channel::Receiver<Arc<Route>>,
    mut exit: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    let mut executor = SenderExecutor::new(
        config,
        hash_provider,
        alt_provider,
        account_provider,
        ix_builder,
        ix_sender,
    )
    .await;

    // let swap_mode: SwapMode = SwapMode::from_str(&input.quote_response.swap_mode)
    // .map_err(|_| anyhow::Error::msg("Invalid SwapMode"))?;

    executor.state.is_ready = true;
    info!("sender executor is ready");

    // 生成 Tokio 任务
    let listener_job = tokio_spawn("sender_executor", async move {
        info!("sender executor is ready");

        // 主循环，处理各种更新事件
        'drain_loop: loop {
            tokio::select! {
                // 处理退出信号
                _ = exit.recv() => {
                    info!("shutting down sender executor task");
                    break;
                }
                route = route_receiver.recv() => {
                    match route {
                        Ok(route) => {
                            let swap = executor.build_swap_tx(route.as_ref()).await;
                            match swap {
                                Ok(swap) => {
                                    let transactions = executor.ix_sender.instructuin_extend(&swap);
                                    match transactions {
                                        Ok(transactions) => {
                                            // 发送交易
                                            executor.ix_sender.send_tx(transactions);
                                        }
                                        Err(e) => {
                                            error!("Failed to extend instruction: {:?}", e);
                                        }
                                    }
                                    
                                }
                                Err(e) => {
                                    error!("Failed to build swap transaction: {:?}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error on route_receiver channel {:?}", e);
                        }
                    }

                },

            }
        }

        error!("sender executor job exited..");
    });

    listener_job
}
