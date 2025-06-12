use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Swap {
   // pub compute_budget_instructions: Vec<Instruction>,
    pub setup_instructions: Vec<Instruction>,
    pub swap_instruction: Instruction,
    pub cleanup_instructions: Vec<Instruction>,
    pub cu_estimate: u32,
    pub address_lookup_table_addresses: Vec<String>,
}

impl Swap {
    pub fn accounts(&self) -> HashSet<Pubkey> {
        let mut transaction_addresses = HashSet::new();

        for ix in self
            .setup_instructions
            .iter()
            .chain(self.cleanup_instructions.iter())
            .chain([&self.swap_instruction].into_iter())
        {
            for acc in &ix.accounts {
                transaction_addresses.insert(acc.pubkey);
            }
        }

        transaction_addresses
    }
}
