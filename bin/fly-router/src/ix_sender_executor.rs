
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;


use crate::alt::alt_optimizer;
use crate::ix_builder::SwapInstructionsBuilder;
use crate::sender::ix_sender::{generate_ix_sender, IxSender, SendMode};
use crate::prelude::*;
use crate::routing_types::Route;
use crate::server::alt_provider::AltProvider;
use crate::server::client_provider::ClientProvider;
use crate::server::hash_provider::HashProvider;
use crate::swap::Swap;
use crate::util::tokio_spawn;
use crate::utils::get_source_atas;

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
    //TIxSender: IxSender + Send + Sync + 'static,
> {
    // 准备就绪信号发送器
    // ready_sender: async_channel::Sender<()>,
    //keypair: Keypair,

    wallet_pk: Pubkey,

    source_atas: HashMap <Pubkey, Pubkey>,

    swap_mode: SwapMode,

    //wrap_and_unwrap_sol: bool,

    compute_unit_price_micro_lamports: u64,

    //auto_create_out_ata: bool,

    slippage_bps: i32,

    alt_accounts: Vec<AddressLookupTableAccount>,

    hash_provider: Arc<THashProvider>,

    alt_provider: Arc<TAltProvider>,

    account_provider: Arc<TAccountProvider>,

    ix_builder: Arc<TIxBuilder>,

    ix_sender: Arc<Box<dyn IxSender + Send + Sync + 'static>>,

    pub state: SenderExecutorState,
}

impl<
        THashProvider: HashProvider + Send + Sync + 'static,
        TAltProvider: AltProvider + Send + Sync + 'static,
        TAccountProvider: AccountProvider + Send + Sync + 'static,
        TIxBuilder: SwapInstructionsBuilder + Send + Sync + 'static,
       // TIxSender: IxSender + Send + Sync + 'static,
    > SenderExecutor<THashProvider, TAltProvider, TAccountProvider, TIxBuilder>
{
    pub async fn new(
        config: &Config,
        rpc: RpcClient,
        keypair: Keypair,
        hash_provider: Arc<THashProvider>,
        alt_provider: Arc<TAltProvider>,
        account_provider: Arc<TAccountProvider>,
        ix_builder: Arc<TIxBuilder>,
        //ix_sender: Arc<TIxSender>,
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

        let wallet_pk = keypair.pubkey();

        //TODO: 这里需要ata_proviter来动态刷新ATA账户，以减少重启和计算CU
        let source_atas = get_source_atas(&rpc, &keypair.pubkey()).await.unwrap_or_default(); // 获取所有的ATA账户

        //TODO：固定输入模式，其他模式后续再支持
        let swap_mode = SwapMode::ExactIn; // 交换模式，默认ExactIn

        //let wrap_and_unwrap_sol = config.sender.wrap_and_unwrap_sol.unwrap_or(false); // 是否需要包裹和解包SOL

        let compute_unit_price_micro_lamports = config
            .sender
            .compute_unit_price_micro_lamports
            .unwrap_or(DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS); // 计算单元价格
        //let auto_create_out_ata = config.sender.auto_create_out_ata.unwrap_or(false); // 是否自动创建输出ATA
        let slippage_bps = config.sender.slippage_bps.unwrap_or(0); // 滑点设置

        let send_mode = config.sender.send_mode.clone().unwrap_or("JitoBundle".to_string()); // 发送模式
        let send_mode = SendMode::from_str(&send_mode).unwrap_or(SendMode::JitoBundle); // 发送模式
        let name= config.sender.name.clone().unwrap_or("fly_router".to_string()); // 发送器名称
        let jito_tip_bps = config.sender.jito_tip_bps.unwrap_or(0.65); // Jito小费
        let jito_max_tip = config.sender.jito_max_tip.unwrap_or(10_000_000); // Jito最大小费
        let mut jito_regions = config.sender.jito_regions.clone().unwrap_or_default(); // Jito区域
        if jito_regions.is_empty() {
            jito_regions = vec!["frankfurt".to_string()]; // 默认区域
        }
        let region_send_type = config.sender.region_send_type.clone().unwrap_or_default(); // Jito区域发送类型

        let ix_sender = generate_ix_sender(
            send_mode,
            name,
            keypair,
            alt_accounts.clone(),
            compute_unit_price_micro_lamports,
            jito_tip_bps,
            jito_max_tip,
            jito_regions,
            region_send_type,
            hash_provider.clone(),
            Arc::new(ClientProvider::new().unwrap()),
        )
        .unwrap_or_else(|_| panic!("Failed to generate ix sender for mode: {}", send_mode.to_string()));

        Self {
           // keypair,
            wallet_pk,
            source_atas,
            swap_mode,
           // wrap_and_unwrap_sol,
            compute_unit_price_micro_lamports,
          //  auto_create_out_ata,
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

    async fn build_swap_tx(&self, route: Arc<Route>) -> anyhow::Result<Swap> {

        let ixs = self.ix_builder.build_ixs(
            &self.wallet_pk,
            &route,
            &self.source_atas,
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
>(
    config: &Config,
    rpc: RpcClient,
    keypair: Keypair,
    hash_provider: Arc<THashProvider>,
    alt_provider: Arc<TAltProvider>,
    account_provider: Arc<TAccountProvider>,
    ix_builder: Arc<TIxBuilder>,
    //ix_sender: Arc<TIxSender>,
    route_receiver: async_channel::Receiver<Arc<Route>>,
    mut exit: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    let mut executor = SenderExecutor::<THashProvider, TAltProvider, TAccountProvider, TIxBuilder>::new(
        config,
        rpc,
        keypair,
        hash_provider,
        alt_provider,
        account_provider,
        ix_builder,
        //ix_sender,
    ).await;

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
                            // let gain = route.out_amount - route.in_amount;
                            let swap = executor.build_swap_tx(route.clone()).await;
                            match swap {
                                Ok(swap) => {
                                    let swap = Arc::new(swap);
                                    let transactions = executor.ix_sender.instructuin_extend(swap,route.clone()).await;
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
