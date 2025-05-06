
use anyhow::Context;
use itertools::Itertools;
use solana_client::nonblocking::rpc_client::RpcClient;
use router_lib::mango::mango_fetcher::MangoMetadata;
use solana_account_decoder::{parse_token::UiTokenAccount, UiAccountData};
use solana_client::rpc_request::TokenAccountsFilter;
use solana_program::pubkey::Pubkey;
use std::collections::HashSet;
use std::collections::HashMap;
use std::str::FromStr;
use solana_sdk::{
    account::ReadableAccount,
    commitment_config::CommitmentConfig,
};
use spl_associated_token_account::get_associated_token_address;
use anchor_spl::token::spl_token;

pub fn get_configured_mints(
    mango_metadata: &Option<MangoMetadata>,
    enabled: bool,
    add_mango_tokens: bool,
    configured_mints: &Vec<String>,
) -> anyhow::Result<HashSet<Pubkey>> {
    if !enabled {
        return Ok(HashSet::new());
    }

    let mut mints = configured_mints
        .iter()
        .map(|s| Pubkey::from_str(s).context(format!("mint {s}")))
        .collect::<anyhow::Result<Vec<Pubkey>>>()?;

    if add_mango_tokens {
        match mango_metadata.as_ref() {
            None => anyhow::bail!("Failed to init dex - missing mango metadata"),
            Some(m) => mints.extend(m.mints.clone()),
        };
    }

    let mints = mints
        .into_iter()
        .collect::<HashSet<Pubkey>>()
        .into_iter()
        .collect();

    Ok(mints)
}

// note used ATM
pub(crate) fn filter_pools_and_mints<T, F>(
    pools: Vec<(Pubkey, T)>,
    mints: &HashSet<Pubkey>,
    take_all_mints: bool,
    mints_getter: F,
) -> Vec<(Pubkey, T)>
where
    F: Fn(&T) -> (Pubkey, Pubkey),
{
    pools
        .into_iter()
        .filter(|(_pool_pk, pool)| {
            let keys = mints_getter(&pool);
            take_all_mints || mints.contains(&keys.0) && mints.contains(&keys.1)
        })
        .collect_vec()
}

// 获取指定所有者的所有 ATA 账户
pub async fn get_source_atas(
    client: &RpcClient,
    owner: &Pubkey,
) -> anyhow::Result<HashMap<Pubkey, Pubkey>> {
    
    // 查询所有者的所有代币账户
    let token_accounts = client.get_token_accounts_by_owner_with_commitment(
            owner,
            TokenAccountsFilter::ProgramId(spl_token::ID),
            CommitmentConfig::confirmed(),
        ).await?;  
    let source_atas = token_accounts.value
        .into_iter()
        .filter_map(|kv| {
            // 链上账户地址
            let ata_pubkey = Pubkey::from_str(&kv.pubkey).ok()?;
            // 解析账户数据，提取 mint
            if let UiAccountData::Json(parsed) = kv.account.data {
                if let Ok(UiTokenAccount { mint, .. }) = serde_json::from_value::<UiTokenAccount>(parsed.parsed.pointer("/info").unwrap().clone()) {
                    let mint_pubkey = Pubkey::from_str(&mint).ok()?;   
                    // 派生官方 ATA
                    let derived = get_associated_token_address(owner, &mint_pubkey);
                    // 仅保留与官方派生地址相同的账户
                    if ata_pubkey == derived {
                        return Some((mint_pubkey, ata_pubkey));
                    }
                }
            }
            None
        })
        .collect();

    Ok(source_atas)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_source_atas() {
        let client = solana_client::nonblocking::rpc_client::RpcClient::new("https://solana-rpc.publicnode.com".to_string());
        
        let owner = Pubkey::from_str("6JEzdPsh749qUUHUGMWuD7PtR4EDUqMZDG87dMHJm6aD").unwrap(); // 替换为实际测试地址
        
        match get_source_atas(&client, &owner).await {
            Ok(atas) => {
                println!("Found {} ATA accounts:", atas.len());
                for (mint, ata) in atas {
                    println!("Mint: {},ATA: {}", mint,ata);
                }
            }
            Err(e) => panic!("Error: {:?}", e),
        }
    }
}