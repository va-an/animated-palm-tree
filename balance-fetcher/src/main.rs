use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::HashMap;
use std::fs;
use tokio;

#[derive(Debug, Deserialize)]
struct Config {
    solana_rpc_url: String,
    wallets: Vec<String>,
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    jsonrpc: String,
    id: u64,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Deserialize)]
struct BalanceResult {
    context: Context,
    value: u64,
}

#[derive(Debug, Deserialize)]
struct Context {
    slot: u64,
}

pub struct SolanaBalanceChecker {
    client: Client,
    rpc_url: String,
}

impl SolanaBalanceChecker {
    pub fn new(rpc_url: String) -> Self {
        Self {
            client: Client::new(),
            rpc_url,
        }
    }

    async fn get_single_balance(
        &self,
        wallet_address: &str,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "getBalance".to_string(),
            params: vec![
                serde_json::Value::String(wallet_address.to_string()),
                serde_json::json!({
                    "commitment": "confirmed"
                }),
            ],
        };

        let response = self
            .client
            .post(&self.rpc_url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let json_response: JsonRpcResponse<BalanceResult> = response.json().await?;

        if let Some(error) = json_response.error {
            return Err(format!("RPC Error: {} - {}", error.code, error.message).into());
        }

        match json_response.result {
            Some(result) => Ok(result.value),
            None => Err("No result in response".into()),
        }
    }

    pub async fn get_balances(
        &self,
        wallet_addresses: Vec<String>,
    ) -> HashMap<String, Result<u64, String>> {
        let tasks: Vec<_> = wallet_addresses
            .into_iter()
            .map(|address| {
                let checker = self;
                async move {
                    let balance_result = checker.get_single_balance(&address).await;
                    match balance_result {
                        Ok(balance) => (address, Ok(balance)),
                        Err(e) => (address, Err(e.to_string())),
                    }
                }
            })
            .collect();

        let results = futures::future::join_all(tasks).await;
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
        assert_eq!(checker.rpc_url, "https://api.mainnet-beta.solana.com");
    }
}
