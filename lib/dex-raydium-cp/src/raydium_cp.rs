use crate::edge::{swap_base_input, swap_base_output, RaydiumCpEdge, RaydiumCpEdgeIdentifier};
use crate::raydium_cp_ix_builder;
use anchor_lang::{AccountDeserialize, Discriminator, Id};
use anchor_spl::token::spl_token::state::AccountState;
use anchor_spl::token::{spl_token, Token};
use anchor_spl::token_2022::spl_token_2022;
use anyhow::Context;
use async_trait::async_trait;
use itertools::Itertools;
use raydium_cp_swap::program::RaydiumCpSwap;
use raydium_cp_swap::states::{AmmConfig, PoolState, PoolStatusBitIndex};
use router_feed_lib::router_rpc_client::{RouterRpcClient, RouterRpcClientTrait};
use router_lib::dex::{
    AccountProviderView, DexEdge, DexEdgeIdentifier, DexInterface, DexSubscriptionMode,
    MixedDexSubscription, Quote, SwapInstruction,
};
use router_lib::utils;
use solana_account_decoder::UiAccountEncoding;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter::{Memcmp, RpcFilterType};
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use solana_sdk::account::ReadableAccount;
use solana_sdk::clock::Clock;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::sysvar::SysvarId;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::u64;


use tracing::{debug, error, info, trace, warn};

pub struct RaydiumCpDex {
    pub edges: HashMap<Pubkey, Vec<Arc<dyn DexEdgeIdentifier>>>,
    pub needed_accounts: HashSet<Pubkey>,
}

#[async_trait]
impl DexInterface for RaydiumCpDex {
    async fn initialize(
        rpc: &mut RouterRpcClient,
        _options: HashMap<String, String>,
        take_all_mints: bool,
        mints: &Vec<String>,
    ) -> anyhow::Result<Arc<dyn DexInterface>>
    where
        Self: Sized,
    {
        info!( "Initializing RaydiumCpDex");
        let pools =
            fetch_raydium_account::<PoolState>(rpc, RaydiumCpSwap::id(), PoolState::LEN).await?;
        
        info!( "RaydiumCpDex Found {} pools", pools.len());

        info!( "Fetching vaults accounts");
        let vaults = pools
            .iter()
            //增加对池的过滤，避免请求过多的账户，如果不是加载所有mints，只加载mints中包含的池
            .filter(|(_pool_pk, pool)| {
                let keep = take_all_mints
                    || (mints.contains(&pool.token_0_mint.to_string()) && mints.contains(&pool.token_1_mint.to_string()));
                keep
            })
            .flat_map(|x| [x.1.token_0_vault, x.1.token_1_vault])
            .collect::<HashSet<_>>();
        //TODO 这里如果加载全部，有24万个vaults，想要将vaults拆分并发执行
        //但是拆分后会导致rpc的请求数过多，可能会被rpc拒绝，怎么优化？
        let vaults = rpc.get_multiple_accounts(&vaults).await?;
        info!( "RaydiumCpDex Found {} vaults", vaults.len());
        
        let banned_vaults = vaults
            .iter()
            .filter(|x| {
                x.1.owner == Token::id()
                    && spl_token::state::Account::unpack(x.1.data()).unwrap().state
                        == AccountState::Frozen
            })
            .map(|x| x.0)
            .collect::<HashSet<_>>();
        info!( "RaydiumCpDex Found {} banned vaults", banned_vaults.len());
        let pools = pools
            .iter()
            .filter(|(_pool_pk, pool)| {
                let keep = take_all_mints
                    || (mints.contains(&pool.token_0_mint.to_string()) && mints.contains(&pool.token_1_mint.to_string()));
                keep
            })
            .filter(|(_pool_pk, pool)| {
                pool.token_0_program == Token::id() && pool.token_1_program == Token::id()
                // TODO Remove filter when 2022 are working
            })
            .filter(|(_pool_pk, pool)| {
                !banned_vaults.contains(&pool.token_0_vault)
                    && !banned_vaults.contains(&pool.token_1_vault)
            })
            .collect_vec();

        let edge_pairs = pools
            .iter()
            .map(|(pool_pk, pool)| {
                (
                    Arc::new(RaydiumCpEdgeIdentifier {
                        pool: *pool_pk,
                        mint_a: pool.token_0_mint,
                        mint_b: pool.token_1_mint,
                        is_a_to_b: true,
                    }),
                    Arc::new(RaydiumCpEdgeIdentifier {
                        pool: *pool_pk,
                        mint_a: pool.token_1_mint,
                        mint_b: pool.token_0_mint,
                        is_a_to_b: false,
                    }),
                )
            })
            .collect_vec();

        let mut needed_accounts = HashSet::new();

        let edges_per_pk = {
            let mut map = HashMap::new();
            for ((pool_pk, pool), (edge_a_to_b, edge_b_to_a)) in pools.iter().zip(edge_pairs.iter())
            {
                let entry = vec![
                    edge_a_to_b.clone() as Arc<dyn DexEdgeIdentifier>,
                    edge_b_to_a.clone(),
                ];

                utils::insert_or_extend(&mut map, pool_pk, &entry);
                utils::insert_or_extend(&mut map, &pool.amm_config, &entry);
                utils::insert_or_extend(&mut map, &pool.token_0_vault, &entry);
                utils::insert_or_extend(&mut map, &pool.token_1_vault, &entry);

                needed_accounts.insert(*pool_pk);
                needed_accounts.insert(pool.amm_config);
                needed_accounts.insert(pool.token_0_vault);
                needed_accounts.insert(pool.token_1_vault);
                // TODO Uncomment for Token-2022
                // needed_accounts.insert(pool.token_0_mint);
                // needed_accounts.insert(pool.token_1_mint);
            }
            map
        };
        info!( "RaydiumCpDex Found {} edges", edges_per_pk.len());
        Ok(Arc::new(RaydiumCpDex {
            edges: edges_per_pk,
            needed_accounts,
        }))
    }

    fn name(&self) -> String {
        "RaydiumCp".to_string()
    }

    fn subscription_mode(&self) -> DexSubscriptionMode {
        DexSubscriptionMode::Mixed(MixedDexSubscription {
            accounts: Default::default(),
            programs: HashSet::from([RaydiumCpSwap::id()]),
            token_accounts_for_owner: HashSet::from([Pubkey::from_str(
                "GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL",
            )
            .unwrap()]),
        })
    }

    fn program_ids(&self) -> HashSet<Pubkey> {
        [RaydiumCpSwap::id()].into_iter().collect()
    }

    fn edges_per_pk(&self) -> HashMap<Pubkey, Vec<Arc<dyn DexEdgeIdentifier>>> {
        self.edges.clone()
    }

    fn load(
        &self,
        id: &Arc<dyn DexEdgeIdentifier>,
        chain_data: &AccountProviderView,
    ) -> anyhow::Result<Arc<dyn DexEdge>> {
        let id = id
            .as_any()
            .downcast_ref::<RaydiumCpEdgeIdentifier>()
            .unwrap();

        let pool_account = chain_data.account(&id.pool)?;
        let pool = PoolState::try_deserialize(&mut pool_account.account.data())?;
        let config_account = chain_data.account(&pool.amm_config)?;
        let config = AmmConfig::try_deserialize(&mut config_account.account.data())?;

        let vault_0_account = chain_data.account(&pool.token_0_vault)?;
        let vault_0 = spl_token_2022::state::Account::unpack(vault_0_account.account.data())?;

        let vault_1_account = chain_data.account(&pool.token_1_vault)?;
        let vault_1 = spl_token_2022::state::Account::unpack(vault_1_account.account.data())?;

        let transfer_0_fee = None;
        let transfer_1_fee = None;

        // TODO Uncomment for Token-2022
        // let mint_0_account = chain_data.account(&pool.token_0_mint)?;
        // let mint_1_account = chain_data.account(&pool.token_1_mint)?;
        // let transfer_0_fee = crate::edge::get_transfer_config(mint_0_account)?;
        // let transfer_1_fee = crate::edge::get_transfer_config(mint_1_account)?;

        Ok(Arc::new(RaydiumCpEdge {
            pool,
            config,
            vault_0_amount: vault_0.amount,
            vault_1_amount: vault_1.amount,
            mint_0: transfer_0_fee,
            mint_1: transfer_1_fee,
        }))
    }

    // 定义一个名为 quote 的方法，它是某个结构体的实例方法，因为使用了 &self
    fn quote(
        // &self 表示对当前结构体实例的不可变引用，用于调用该结构体的其他方法或访问其字段
        &self,
        // id 是一个对实现了 DexEdgeIdentifier 特征的类型的不可变引用，使用 Arc 智能指针实现共享所有权
        id: &Arc<dyn DexEdgeIdentifier>,
        // edge 是一个对实现了 DexEdge 特征的类型的不可变引用，同样使用 Arc 智能指针
        edge: &Arc<dyn DexEdge>,
        // chain_data 是对 AccountProviderView 类型的不可变引用，可能包含链上账户的相关数据
        chain_data: &AccountProviderView,
        // in_amount 是一个无符号 64 位整数，表示输入的数量
        in_amount: u64,
    // 该方法返回一个 anyhow::Result 类型，其中包含一个 Quote 结构体实例，可能会返回错误
    ) -> anyhow::Result<Quote> {
        // 将 id 从动态类型转换为具体的 RaydiumCpEdgeIdentifier 类型
        // as_any() 方法将 id 转换为 Any 类型，然后使用 downcast_ref 尝试将其转换为 RaydiumCpEdgeIdentifier 类型
        // unwrap() 方法用于解包结果，如果转换失败会触发 panic
        let id = id
        .as_any()
        .downcast_ref::<RaydiumCpEdgeIdentifier>()
        .unwrap();
        // 同样地，将 edge 从动态类型转换为具体的 RaydiumCpEdge 类型
        let edge = edge.as_any().downcast_ref::<RaydiumCpEdge>().unwrap();

        // 检查交易池的交换状态
        // edge.pool.get_status_by_bit(PoolStatusBitIndex::Swap) 用于获取交易池的交换状态位
        // 如果交换状态为 false，表示交换不可用
        if !edge.pool.get_status_by_bit(PoolStatusBitIndex::Swap) {
            // 返回一个 Quote 结构体实例，其中输入、输出和手续费金额都为 0
            // fee_mint 为交易池的第一个代币的铸造地址
            return Ok(Quote {
                in_amount: 0,
                out_amount: 0,
                fee_amount: 0,
                fee_mint: edge.pool.token_0_mint,
            });
        }
//TODO chain_data.account(&Clock::id())?;获取不到 SysvarC1ock11111111111111111111111111111111 时钟账户信息，需要在chain_data中实现对应账户的数据更新
        // // 从 chain_data 中获取时钟账户信息
        // // Clock::id() 返回时钟账户的 ID
        // // chain_data.account() 方法根据 ID 获取账户信息
        // // context("read clock") 为错误信息添加上下文，方便调试
        // let clock = chain_data.account(&Clock::id()).context("read clock")?;
        // // 从时钟账户数据中反序列化出 Clock 结构体实例
        // // 并获取当前的 Unix 时间戳，将其转换为无符号 64 位整数
        // let now_ts = clock.account.deserialize_data::<Clock>()?.unix_timestamp as u64;
        // // 检查交易池的开放时间是否大于当前时间
        // // 如果开放时间大于当前时间，表示交易池尚未开放
        // if edge.pool.open_time > now_ts {
        //     // 返回一个 Quote 结构体实例，其中输入、输出和手续费金额都为 0
        //     // fee_mint 为交易池的第一个代币的铸造地址
        //     return Ok(Quote {
        //         in_amount: 0,
        //         out_amount: 0,
        //         fee_amount: 0,
        //         fee_mint: edge.pool.token_0_mint,
        //     });
        // }

        // 根据 id.is_a_to_b 的值决定调用哪个方向的交换函数
        let quote = if id.is_a_to_b {
            // 调用 swap_base_input 函数进行 A 到 B 方向的交换计算
            // 传入交易池、配置、代币 0 的金库信息、代币 0 的数量、代币 0 的铸造信息
            // 代币 1 的金库信息、代币 1 的数量、代币 1 的铸造信息以及输入数量
            let result = swap_base_input(
                &edge.pool,
                &edge.config,
                edge.pool.token_0_vault,
                edge.vault_0_amount,
                &edge.mint_0,
                edge.pool.token_1_vault,
                edge.vault_1_amount,
                &edge.mint_1,
                in_amount,
            )?;

            // 根据交换结果创建一个 Quote 结构体实例
            Quote {
                in_amount: result.0,
                out_amount: result.1,
                fee_amount: result.2,
                fee_mint: edge.pool.token_0_mint,
            }
        } else {
            // 调用 swap_base_input 函数进行 B 到 A 方向的交换计算
            // 传入的参数与 A 到 B 方向相反
            let result = swap_base_input(
                &edge.pool,
                &edge.config,
                edge.pool.token_1_vault,
                edge.vault_1_amount,
                &edge.mint_1,
                edge.pool.token_0_vault,
                edge.vault_0_amount,
                &edge.mint_0,
                in_amount,
            )?;

            // 根据交换结果创建一个 Quote 结构体实例
            Quote {
                in_amount: result.0,
                out_amount: result.1,
                fee_amount: result.2,
                fee_mint: edge.pool.token_1_mint,
            }
        };
        // 返回最终的 Quote 结构体实例
        Ok(quote)
}

    fn build_swap_ix(
        &self,
        id: &Arc<dyn DexEdgeIdentifier>,
        chain_data: &AccountProviderView,
        wallet_pk: &Pubkey,
        in_amount: u64,
        out_amount: u64,
        max_slippage_bps: i32,
    ) -> anyhow::Result<SwapInstruction> {
        let id = id
            .as_any()
            .downcast_ref::<RaydiumCpEdgeIdentifier>()
            .unwrap();
        raydium_cp_ix_builder::build_swap_ix(
            id,
            chain_data,
            wallet_pk,
            in_amount,
            out_amount,
            max_slippage_bps,
        )
    }

    fn supports_exact_out(&self, _id: &Arc<dyn DexEdgeIdentifier>) -> bool {
        true
    }

    fn quote_exact_out(
        &self,
        id: &Arc<dyn DexEdgeIdentifier>,
        edge: &Arc<dyn DexEdge>,
        chain_data: &AccountProviderView,
        out_amount: u64,
    ) -> anyhow::Result<Quote> {
        let id = id
            .as_any()
            .downcast_ref::<RaydiumCpEdgeIdentifier>()
            .unwrap();
        let edge = edge.as_any().downcast_ref::<RaydiumCpEdge>().unwrap();

        if !edge.pool.get_status_by_bit(PoolStatusBitIndex::Swap) {
            return Ok(Quote {
                in_amount: u64::MAX,
                out_amount: 0,
                fee_amount: 0,
                fee_mint: edge.pool.token_0_mint,
            });
        }

        let clock = chain_data.account(&Clock::id()).context("read clock")?;
        let now_ts = clock.account.deserialize_data::<Clock>()?.unix_timestamp as u64;
        if edge.pool.open_time > now_ts {
            return Ok(Quote {
                in_amount: u64::MAX,
                out_amount: 0,
                fee_amount: 0,
                fee_mint: edge.pool.token_0_mint,
            });
        }

        let quote = if id.is_a_to_b {
            let result = swap_base_output(
                &edge.pool,
                &edge.config,
                edge.pool.token_0_vault,
                edge.vault_0_amount,
                &edge.mint_0,
                edge.pool.token_1_vault,
                edge.vault_1_amount,
                &edge.mint_1,
                out_amount,
            )?;

            Quote {
                in_amount: result.0,
                out_amount: result.1,
                fee_amount: result.2,
                fee_mint: edge.pool.token_0_mint,
            }
        } else {
            let result = swap_base_output(
                &edge.pool,
                &edge.config,
                edge.pool.token_1_vault,
                edge.vault_1_amount,
                &edge.mint_1,
                edge.pool.token_0_vault,
                edge.vault_0_amount,
                &edge.mint_0,
                out_amount,
            )?;

            Quote {
                in_amount: result.0,
                out_amount: result.1,
                fee_amount: result.2,
                fee_mint: edge.pool.token_1_mint,
            }
        };
        Ok(quote)
    }
}

async fn fetch_raydium_account<T: Discriminator + AccountDeserialize>(
    rpc: &mut RouterRpcClient,
    program_id: Pubkey,
    len: usize,
) -> anyhow::Result<Vec<(Pubkey, T)>> {
    let config = RpcProgramAccountsConfig {
        filters: Some(vec![
            RpcFilterType::DataSize(len as u64),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(0, T::DISCRIMINATOR.to_vec())),
        ]),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::finalized()),
            ..Default::default()
        },
        ..Default::default()
    };

    let snapshot = rpc
        .get_program_accounts_with_config(&program_id, config)
        .await?;

    let result = snapshot
        .iter()
        .map(|account| {
            let pool: T = T::try_deserialize(&mut account.data.as_slice()).unwrap();
            (account.pubkey, pool)
        })
        .collect_vec();

    Ok(result)
}



// async fn get_multiple_accounts_batched(
//     rpc: &mut RouterRpcClient,
//     all_vaults: &HashSet<Pubkey>,
//     batch_size: usize,
//     max_concurrent_requests : usize,
//     // 这里的batch_size是每个请求的大小，max_concurrent_requests是并发请求的数量
// ) -> anyhow::Result<Vec<(Pubkey, Account)>> {

//     let semaphore = Arc::new(Semaphore::new(max_concurrent_requests));

//     // 分批次处理 vaults
//     let mut tasks = Vec::new();
//     let mut all_results = Vec::new();
//     let mut batch = HashSet::new();

//     for vault in all_vaults.iter() {
//         batch.insert(*vault);
//         if batch.len() >= batch_size {
//             let rpc_clone = rpc.clone(); // Clone the rpc client to ensure proper ownership
//             let permit = semaphore.clone().acquire_owned().await?;
//             let batch_clone = batch.clone();
//             let task = task::spawn(async move {
//                 let _permit_guard = permit;
//                 let result = rpc_clone.get_multiple_accounts(&batch_clone).await?;
//                 Ok(result) as anyhow::Result<Vec<(Pubkey, Account)>>
//             });
//             tasks.push(task);
//             batch.clear();
//         }
       
//     }

//     // 等待所有任务完成并收集结果
//     for task in tasks {
//         let result = task.await??;
//         all_results.extend(result);
//     }
//     Ok(all_results)
// }
