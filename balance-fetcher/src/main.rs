use futures::future::join_all;
use serde::Deserialize;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::fs;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct Config {
    solana_rpc_url: String,
    wallets: Vec<String>,
}

pub struct SolanaBalanceChecker {
    client: RpcClient,
}

impl SolanaBalanceChecker {
    pub fn new(rpc_url: String) -> Self {
        Self {
            client: RpcClient::new(rpc_url),
        }
    }

    pub async fn get_balances(
        &self,
        wallet_addresses: Vec<String>,
    ) -> HashMap<String, Result<u64, String>> {
        let tasks: Vec<_> = wallet_addresses
            .into_iter()
            .map(|address| {
                let client = &self.client;
                async move {
                    match Pubkey::from_str(&address) {
                        Ok(pubkey) => match client.get_balance(&pubkey).await {
                            Ok(balance) => (address, Ok(balance)),
                            Err(e) => (address, Err(e.to_string())),
                        },
                        Err(e) => (address, Err(format!("Invalid pubkey: {}", e))),
                    }
                }
            })
            .collect();

        let results = join_all(tasks).await;
        results.into_iter().collect()
    }

    pub fn lamports_to_sol(lamports: u64) -> f64 {
        lamports as f64 / 1_000_000_000.0
    }
}

fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path)?;
    let config: Config = serde_yaml::from_str(&contents)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config("config.yaml")?;
    let balance_checker = SolanaBalanceChecker::new(config.solana_rpc_url);
    let balances = balance_checker.get_balances(config.wallets).await;

    println!("=== Solana Wallet Balances ===\n");

    for (wallet, balance_result) in balances {
        match balance_result {
            Ok(lamports) => {
                let sol_balance = SolanaBalanceChecker::lamports_to_sol(lamports);
                println!("Wallet: {}", wallet);
                println!("Balance: {} lamports ({:.9} SOL)", lamports, sol_balance);
                println!("---");
            }
            Err(error) => {
                println!("Wallet: {}", wallet);
                println!("Error: {}", error);
                println!("---");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lamports_to_sol_conversion() {
        assert_eq!(SolanaBalanceChecker::lamports_to_sol(1_000_000_000), 1.0);
        assert_eq!(SolanaBalanceChecker::lamports_to_sol(500_000_000), 0.5);
        assert_eq!(SolanaBalanceChecker::lamports_to_sol(0), 0.0);
    }

    #[tokio::test]
    async fn test_balance_checker_creation() {
        let checker = SolanaBalanceChecker::new("https://api.mainnet-beta.solana.com".to_string());
        assert!(!checker.client.url().is_empty());
    }

    #[test]
    fn test_pubkey_validation() {
        assert!(Pubkey::from_str("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM").is_ok());
        assert!(Pubkey::from_str("invalid_pubkey").is_err());
    }
}
