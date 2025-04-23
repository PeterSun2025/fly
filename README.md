# fly

Fly is a aggregator for swaps on Solana.
## Usage
Generate a new bot wallet address and extract the private key into a raw 32-byte format.
Deploy the included BundleExecutor.sol to Ethereum, from a secured account, with the address of the newly created wallet as the constructor argument
Transfer WETH to the newly deployed BundleExecutor
It is important to keep both the bot wallet private key and bundleExecutor owner private key secure. The bot wallet attempts to not lose WETH inside an arbitrage, but a malicious user would be able to drain the contract.