



pub struct JupSwapStepInstructionBuilder {
    pub jup_url: String,

}

impl JupSwapStepInstructionBuilder {
    pub fn new(jup_url: String) -> Self {
        Self {
            jup_url,
        }
    }

    pub fn build(&self, swap: &JupSwap) -> Result<Vec<Instruction>, ProgramError> {
        let mut instructions = vec![];
        let mut accounts = vec![];

        // Add the first instruction to the list
        let first_instruction = self.build_first_instruction(swap)?;
        instructions.push(first_instruction);

        // Add the rest of the instructions to the list
        for i in 1..swap.steps.len() {
            let step = &swap.steps[i];
            let instruction = self.build_step_instruction(step, &accounts)?;
            instructions.push(instruction);
        }

        Ok(instructions)
    }
}

impl JupSwapStepInstructionBuilder  for JupSwapStepInstructionBuilder {
    fn build_ixs(&self, swap: &JupSwap) -> Result<Vec<Instruction>, ProgramError> {
        let mut instructions = vec![];
        let mut accounts = vec![];

        // Add the first instruction to the list
        let first_instruction = self.build_first_instruction(swap)?;
        instructions.push(first_instruction);

        // Add the rest of the instructions to the list
        for i in 1..swap.steps.len() {
            let step = &swap.steps[i];
            let instruction = self.build_step_instruction(step, &accounts)?;
            instructions.push(instruction);
        }

        Ok(instructions)
    }
}
