use std::collections::HashSet;

use crate::account_write::AccountWrite;
use solana_client::rpc_config::RpcProgramAccountsConfig;
use solana_client::rpc_request::TokenAccountsFilter;
use solana_client::rpc_response::RpcKeyedAccount;
use solana_client::client_error::Result;
use solana_sdk::account::Account;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;

#[async_trait::async_trait]
pub trait RouterRpcClientTrait: Sync + Send {
    async fn get_account(&mut self, pubkey: &Pubkey) -> anyhow::Result<Option<Account>>;

    async fn get_multiple_accounts(
        &mut self,
        pubkeys: &HashSet<Pubkey>,
    ) -> anyhow::Result<Vec<(Pubkey, Account)>>;

    async fn get_program_accounts_with_config(
        &mut self,
        pubkey: &Pubkey,
        config: RpcProgramAccountsConfig,
    ) -> anyhow::Result<Vec<AccountWrite>>;

    async fn get_token_accounts_by_owner_with_commitment(
        &self,
        owner: &Pubkey,
        token_account_filter: TokenAccountsFilter,
        commitment_config: CommitmentConfig,
    ) -> Result<Vec<RpcKeyedAccount>>;

    fn is_gpa_compression_enabled(&self) -> bool;
}

pub struct RouterRpcClient {
    pub rpc: Box<dyn RouterRpcClientTrait + Send + Sync + 'static>,
    pub gpa_compression_enabled: bool,
}

#[async_trait::async_trait]
impl RouterRpcClientTrait for RouterRpcClient {
    async fn get_account(&mut self, pubkey: &Pubkey) -> anyhow::Result<Option<Account>> {
        self.rpc.get_account(pubkey).await
    }

    async fn get_multiple_accounts(
        &mut self,
        pubkeys: &HashSet<Pubkey>,
    ) -> anyhow::Result<Vec<(Pubkey, Account)>> {
        self.rpc.get_multiple_accounts(pubkeys).await
    }

    async fn get_program_accounts_with_config(
        &mut self,
        pubkey: &Pubkey,
        config: RpcProgramAccountsConfig,
    ) -> anyhow::Result<Vec<AccountWrite>> {
        self.rpc
            .get_program_accounts_with_config(pubkey, config)
            .await
    }

    async fn get_token_accounts_by_owner_with_commitment(
        &self,
        owner: &Pubkey,
        token_account_filter: TokenAccountsFilter,
        commitment_config: CommitmentConfig,
    ) -> Result<Vec<RpcKeyedAccount>> {
        self.rpc
            .get_token_accounts_by_owner_with_commitment(owner, token_account_filter,commitment_config)
            .await
    }

    fn is_gpa_compression_enabled(&self) -> bool {
        self.gpa_compression_enabled
    }
}
