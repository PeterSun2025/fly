
use std::collections::HashMap;
use std::sync::Arc;
use axum::async_trait;
// Import Serialize and Deserialize macros from serde.
use serde::{Serialize, Deserialize};

use crate::{routing_types::Route, server::{alt_provider::AltProvider, client_provider::ClientProvider, hash_provider::HashProvider}, swap::Swap}; // Import Swap if it exists in another module or define it below.
use std::str::FromStr; // Import FromStr trait for parsing strings.
use solana_sdk::{transaction::VersionedTransaction, signer::keypair::Keypair};

use super::jito_ix_sender::JitoIxSender;

#[derive(Clone, Copy, Hash, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SendMode {
    #[default]
    JitoBundle = 0,
}

#[derive(Debug)]
pub struct ParseSendModeError;

impl FromStr for SendMode {
    type Err = ParseSendModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "JitoBundle" {
            Ok(Self::JitoBundle)
        } else {
            Err(ParseSendModeError)
        }
    }
}

impl ToString for SendMode {
    fn to_string(&self) -> String {
        match &self {
            SendMode::JitoBundle => "JitoBundle".to_string(),
        }
    }
}

#[async_trait]
pub trait IxSender {
    async fn instructuin_extend(&self, swap: Arc<Swap>,route:Arc<Route>) -> anyhow::Result<HashMap<String, Vec<VersionedTransaction>>>;
    async fn send_tx(&self, transactions: HashMap<String, Vec<VersionedTransaction>>) -> anyhow::Result<()>;
}

pub fn generate_ix_sender<THashProvider, TAltProvider>(mode: SendMode,
    name: String,
    keypair: Keypair,
    alt_provider: Arc<TAltProvider>,
    compute_unit_price_micro_lamports: u64,
    jito_tip_bps: f32,
    jito_max_tip: u64,
    jito_regions: Vec<String>,
    region_send_type: String,
    hash_provider: Arc<THashProvider>,
    client_provider: Arc<ClientProvider>) -> anyhow::Result<Arc<Box<dyn IxSender + Send + Sync + 'static>>> 
    where 
    THashProvider: HashProvider + Send + Sync + 'static,
    TAltProvider: AltProvider + Send + Sync + 'static
    {
    match mode {
        SendMode::JitoBundle => {
            let sender = JitoIxSender::new(
                name,
                keypair,
                alt_provider,
                compute_unit_price_micro_lamports,
                jito_tip_bps,
                jito_max_tip,
                jito_regions,
                region_send_type,
                hash_provider,
                client_provider,
            );
            Ok(Arc::new(Box::new(sender)))
        }
    }
}

