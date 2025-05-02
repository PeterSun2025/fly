use std::collections::HashSet;

use async_trait::async_trait;
use itertools::Itertools;
use solana_account_decoder::UiAccountEncoding;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_request::TokenAccountsFilter;
use solana_client::rpc_response::RpcKeyedAccount;
use solana_sdk::account::Account;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;

use crate::account_write::AccountWrite;
use crate::get_program_account::{
    fetch_multiple_accounts, get_compressed_program_account_rpc,
    get_uncompressed_program_account_rpc,
};
use crate::router_rpc_client::RouterRpcClientTrait;

pub struct RouterRpcWrapper {
    pub rpc: RpcClient,
    pub gpa_compression_enabled: bool,
}

#[async_trait]
impl RouterRpcClientTrait for RouterRpcWrapper {
    async fn get_account(&mut self, pubkey: &Pubkey) -> anyhow::Result<Option<Account>> {
        let response = self
            .rpc
            .get_account_with_config(
                pubkey,
                RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    data_slice: None,
                    commitment: Some(self.rpc.commitment()),
                    min_context_slot: None,
                },
            )
            .await?;

        Ok(response.value)
    }

    async fn get_multiple_accounts(
        &mut self,
        pubkeys: &HashSet<Pubkey>,
    ) -> anyhow::Result<Vec<(Pubkey, Account)>> {
        let keys = pubkeys.iter().cloned().collect_vec();
        let result = fetch_multiple_accounts(&self.rpc, keys.as_slice(), 100).await?;
        Ok(result)
    }

    async fn get_program_accounts_with_config(
        &mut self,
        pubkey: &Pubkey,
        config: RpcProgramAccountsConfig,
    ) -> anyhow::Result<Vec<AccountWrite>> {
        if self.is_gpa_compression_enabled() {
            Ok(
                get_compressed_program_account_rpc(&self.rpc, &HashSet::from([*pubkey]), config)
                    .await?
                    .1,
            )
        } else {
            Ok(
                get_uncompressed_program_account_rpc(&self.rpc, &HashSet::from([*pubkey]), config)
                    .await?
                    .1,
            )
        }
    }

    async fn get_token_accounts_by_owner_with_commitment(
        &self,
        owner: &Pubkey,
        token_account_filter: TokenAccountsFilter,
        commitment_config: CommitmentConfig,
    ) -> solana_client::client_error::Result<Vec<RpcKeyedAccount>> {
        self.rpc
            .get_token_accounts_by_owner_with_commitment(owner, token_account_filter, commitment_config)
            .await
            .map(|response| response.value)
    }

    fn is_gpa_compression_enabled(&self) -> bool {
        self.gpa_compression_enabled
    }
}
