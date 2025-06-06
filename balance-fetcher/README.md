# Solana Balance Fetcher

Fetch Solana wallet balances concurrently using JSON RPC API.

## Setup

1. Edit `config.yaml` with your wallet addresses:
   ```yaml
   solana_rpc_url: "https://api.mainnet-beta.solana.com"
   wallets:
     - "YOUR_WALLET_ADDRESS_1"
     - "YOUR_WALLET_ADDRESS_2"
   ```

2. Run:
   ```bash
   cargo run
   ```

## Output
```
=== Solana Wallet Balances ===

Wallet: 9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM
Balance: 1234567890 lamports (1.234567890 SOL)
---
```

