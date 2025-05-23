use solana_program::pubkey::Pubkey;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::warn;

pub type Decimals = u8;

//#[derive(Clone, Copy)]
#[derive(Debug, Clone)]
pub struct Token {
    pub mint: Pubkey,
    pub decimals: Decimals,
    pub symbol: String,
}

#[derive(Clone)]
pub struct TokenCache {
    tokens: Arc<HashMap<Pubkey, Token>>,
}

impl TokenCache {
    pub fn new(data: HashMap<Pubkey, Token>) -> Self {
        Self {
            tokens: Arc::new(data),
        }
    }

    // use Result over Option to be compatible
    pub fn token(&self, mint: Pubkey) -> anyhow::Result<Token> {
        self.tokens
            .get(&mint)
            .map(|token| Token { mint, decimals: token.decimals, symbol: token.symbol.clone() })
            .ok_or_else(|| {
                // this should never happen
                warn!("Token not found in cache: {}", mint);
                anyhow::anyhow!("Token not found in cache")
            })
    }

    pub fn tokens(&self) -> HashSet<Pubkey> {
        self.tokens
            .iter()
            .map(|(k, _)| *k)
            .collect::<HashSet<Pubkey>>()
    }

    pub fn string_tokens(&self) -> Vec<String> {
        self.tokens
            .iter()
            .map(|(k, _)| k.to_string())
            .collect::<Vec<String>>()
    }

    pub fn get_symbol_by_mint(&self, mint: Pubkey) -> Option<String> {
        self.tokens.get(&mint).map(|token| token.symbol.clone())
    }
}
