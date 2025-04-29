
use std::collections::HashMap;
// Import Serialize and Deserialize macros from serde.
use serde::{Serialize, Deserialize};

use crate::{server::hash_provider::HashProvider, swap::Swap}; // Import Swap if it exists in another module or define it below.
use std::str::FromStr; // Import FromStr trait for parsing strings.
use solana_sdk::transaction::Transaction;

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

pub trait IxSender {
    async fn instructuin_extend(&self, swap: &Swap) -> anyhow::Result<HashMap<String, Vec<Transaction>>>;
    async fn send_tx(&self, swaps: HashMap<String, Vec<Transaction>>) -> anyhow::Result<()>;
}

