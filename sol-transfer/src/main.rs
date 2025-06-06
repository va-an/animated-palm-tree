use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::HashMap;
use std::fs;
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio;

// Solana SDK imports
use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    system_instruction,
    transaction::Transaction,
};

// Configuration structures
#[derive(Debug, Deserialize)]
struct Config {
    solana_rpc_url: String,
    sender_wallets: Vec<SenderWallet>,
    recipient_addresses: Vec<String>,
    amount_sol: f64,
}

#[derive(Debug, Deserialize, Clone)]
struct SenderWallet {
    address: String,
    private_key: String, // Base58 encoded private key
}

// JSON RPC structures
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

// Blockhash result structure
#[derive(Debug, Deserialize)]
struct BlockhashResult {
    value: BlockhashValue,
}

#[derive(Debug, Deserialize)]
struct BlockhashValue {
    blockhash: String,
    #[serde(rename = "lastValidBlockHeight")]
    last_valid_block_height: u64,
}

// Transaction status structures
#[derive(Debug, Deserialize)]
struct SignatureStatusResult {
    value: Option<SignatureStatus>,
}

#[derive(Debug, Deserialize)]
struct SignatureStatus {
    slot: u64,
    confirmations: Option<u64>,
    err: Option<serde_json::Value>,
    #[serde(rename = "confirmationStatus")]
    confirmation_status: Option<String>,
}

#[derive(Debug)]
struct TransferResult {
    from_address: String,
    to_address: String,
    signature: String,
    status: Option<SignatureStatus>,
    processing_time: Duration,
    error: Option<String>,
}

pub struct SolTransfer {
    client: Client,
    rpc_url: String,
}

impl SolTransfer {
    pub fn new(rpc_url: String) -> Self {
        Self {
            client: Client::new(),
            rpc_url,
        }
    }

    // Convert SOL to lamports
    fn sol_to_lamports(sol: f64) -> u64 {
        (sol * 1_000_000_000.0) as u64
    }

    // Get recent blockhash
    async fn get_recent_blockhash(&self) -> Result<Hash, Box<dyn std::error::Error>> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "getLatestBlockhash".to_string(),
            params: vec![serde_json::json!({
                "commitment": "confirmed"
            })],
        };

        let response = self
            .client
            .post(&self.rpc_url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let json_response: JsonRpcResponse<BlockhashResult> = response.json().await?;

        if let Some(error) = json_response.error {
            return Err(format!("RPC Error: {} - {}", error.code, error.message).into());
        }

        match json_response.result {
            Some(result) => {
                let blockhash_str = &result.value.blockhash;
                let blockhash = Hash::from_str(blockhash_str)?;
                Ok(blockhash)
            }
            None => Err("No result in response".into()),
        }
    }

    // Create a real transfer transaction
    fn create_transfer_transaction(
        &self,
        sender_keypair: &Keypair,
        recipient_pubkey: &Pubkey,
        lamports: u64,
        recent_blockhash: Hash,
    ) -> Result<Transaction, Box<dyn std::error::Error>> {
        let instruction =
            system_instruction::transfer(&sender_keypair.pubkey(), recipient_pubkey, lamports);

        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&sender_keypair.pubkey()),
            &[sender_keypair],
            recent_blockhash,
        );

        Ok(transaction)
    }

    // Send a transaction
    async fn send_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let serialized_transaction = bincode::serialize(transaction)?;
        let encoded_transaction = base64::encode(serialized_transaction);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "sendTransaction".to_string(),
            params: vec![
                serde_json::Value::String(encoded_transaction),
                serde_json::json!({
                    "encoding": "base64",
                    "preflightCommitment": "confirmed",
                    "skipPreflight": false
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

        let json_response: JsonRpcResponse<String> = response.json().await?;

        if let Some(error) = json_response.error {
            return Err(format!("RPC Error: {} - {}", error.code, error.message).into());
        }

        match json_response.result {
            Some(signature) => Ok(signature),
            None => Err("No signature in response".into()),
        }
    }

    // Check transaction status
    async fn get_signature_status(
        &self,
        signature: &str,
    ) -> Result<Option<SignatureStatus>, Box<dyn std::error::Error>> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "getSignatureStatus".to_string(),
            params: vec![
                serde_json::Value::String(signature.to_string()),
                serde_json::json!({
                    "searchTransactionHistory": true
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

        let json_response: JsonRpcResponse<SignatureStatusResult> = response.json().await?;

        if let Some(error) = json_response.error {
            return Err(format!("RPC Error: {} - {}", error.code, error.message).into());
        }

        match json_response.result {
            Some(result) => Ok(result.value),
            None => Err("No result in response".into()),
        }
    }

    // Parse private key from base58
    fn parse_keypair(private_key_base58: &str) -> Result<Keypair, Box<dyn std::error::Error>> {
        let private_key_bytes = bs58::decode(private_key_base58).into_vec()?;
        if private_key_bytes.len() != 64 {
            return Err(format!(
                "Invalid private key length: expected 64 bytes, got {}",
                private_key_bytes.len()
            )
            .into());
        }
        Ok(Keypair::from_bytes(&private_key_bytes)?)
    }

    // Execute all transfers concurrently
    pub async fn execute_transfers(
        &self,
        sender_wallets: Vec<SenderWallet>,
        recipients: Vec<String>,
        amount_lamports: u64,
    ) -> Vec<TransferResult> {
        // Get recent blockhash
        let blockhash = match self.get_recent_blockhash().await {
            Ok(hash) => hash,
            Err(e) => {
                println!("❌ Failed to get blockhash: {}", e);
                return vec![];
            }
        };

        println!("✅ Using blockhash: {}", blockhash);
        println!(
            "🚀 Starting {} transfers...\n",
            sender_wallets.len() * recipients.len()
        );

        let mut tasks = Vec::new();

        // Create transfer tasks for each sender-recipient pair
        for sender in &sender_wallets {
            for recipient in &recipients {
                let sender_clone = sender.clone();
                let recipient_clone = recipient.clone();
                let blockhash_clone = blockhash;
                let transfer_client = &self;

                let task = async move {
                    let start_time = Instant::now();

                    // Parse sender keypair
                    let sender_keypair = match Self::parse_keypair(&sender_clone.private_key) {
                        Ok(keypair) => keypair,
                        Err(e) => {
                            let processing_time = start_time.elapsed();
                            return TransferResult {
                                from_address: sender_clone.address,
                                to_address: recipient_clone,
                                signature: String::new(),
                                status: None,
                                processing_time,
                                error: Some(format!("Failed to parse keypair: {}", e)),
                            };
                        }
                    };

                    // Parse recipient pubkey
                    let recipient_pubkey = match Pubkey::from_str(&recipient_clone) {
                        Ok(pubkey) => pubkey,
                        Err(e) => {
                            let processing_time = start_time.elapsed();
                            return TransferResult {
                                from_address: sender_clone.address,
                                to_address: recipient_clone,
                                signature: String::new(),
                                status: None,
                                processing_time,
                                error: Some(format!("Invalid recipient address: {}", e)),
                            };
                        }
                    };

                    // Create transaction
                    let transaction = match transfer_client.create_transfer_transaction(
                        &sender_keypair,
                        &recipient_pubkey,
                        amount_lamports,
                        blockhash_clone,
                    ) {
                        Ok(tx) => tx,
                        Err(e) => {
                            let processing_time = start_time.elapsed();
                            return TransferResult {
                                from_address: sender_clone.address,
                                to_address: recipient_clone,
                                signature: String::new(),
                                status: None,
                                processing_time,
                                error: Some(format!("Failed to create transaction: {}", e)),
                            };
                        }
                    };

                    // Send transaction
                    let signature = match transfer_client.send_transaction(&transaction).await {
                        Ok(sig) => sig,
                        Err(e) => {
                            let processing_time = start_time.elapsed();
                            return TransferResult {
                                from_address: sender_clone.address,
                                to_address: recipient_clone,
                                signature: String::new(),
                                status: None,
                                processing_time,
                                error: Some(format!("Failed to send transaction: {}", e)),
                            };
                        }
                    };

                    // Wait for confirmation
                    tokio::time::sleep(Duration::from_millis(2000)).await;

                    // Check status
                    let status = match transfer_client.get_signature_status(&signature).await {
                        Ok(status) => status,
                        Err(e) => {
                            println!("⚠️  Warning: Failed to get status for {}: {}", signature, e);
                            None
                        }
                    };

                    let processing_time = start_time.elapsed();

                    TransferResult {
                        from_address: sender_clone.address,
                        to_address: recipient_clone,
                        signature,
                        status,
                        processing_time,
                        error: None,
                    }
                };

                tasks.push(task);
            }
        }

        // Execute all transfers concurrently
        futures::future::join_all(tasks).await
    }

    // Print transfer statistics
    pub fn print_statistics(&self, results: &[TransferResult]) {
        let mut successful = 0;
        let mut failed = 0;
        let mut total_time = Duration::new(0, 0);
        let mut min_time = Duration::from_secs(u64::MAX);
        let mut max_time = Duration::new(0, 0);

        println!("\n=== Transfer Results ===\n");

        for result in results {
            if let Some(error) = &result.error {
                failed += 1;
                println!("❌ FAILED TRANSFER");
                println!("From: {}", result.from_address);
                println!("To: {}", result.to_address);
                println!("Error: {}", error);
                println!("Processing Time: {:?}", result.processing_time);
                println!("---");
                continue;
            }

            successful += 1;
            total_time += result.processing_time;
            min_time = min_time.min(result.processing_time);
            max_time = max_time.max(result.processing_time);

            let status_str = if let Some(status) = &result.status {
                if status.err.is_some() {
                    "❌ TRANSACTION FAILED"
                } else {
                    "✅ SUCCESS"
                }
            } else {
                "⏳ PENDING"
            };

            println!("From: {}", result.from_address);
            println!("To: {}", result.to_address);
            println!("Signature: {}", result.signature);
            println!("Status: {}", status_str);
            println!("Processing Time: {:?}", result.processing_time);

            if let Some(status) = &result.status {
                println!("Slot: {}", status.slot);
                if let Some(confirmations) = status.confirmations {
                    println!("Confirmations: {}", confirmations);
                }
                if let Some(confirmation_status) = &status.confirmation_status {
                    println!("Confirmation Status: {}", confirmation_status);
                }
            }
            println!("---");
        }

        println!("\n=== Statistics ===");
        println!("Total transfers: {}", successful + failed);
        println!("Successful: {}", successful);
        println!("Failed: {}", failed);

        if successful > 0 {
            let avg_time = total_time / successful as u32;
            println!("Average processing time: {:?}", avg_time);
            if min_time != Duration::from_secs(u64::MAX) {
                println!("Min processing time: {:?}", min_time);
            }
            println!("Max processing time: {:?}", max_time);
        }
    }
}

// Load configuration from YAML
fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path)?;
    let config: Config = serde_yaml::from_str(&contents)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 SOL Transfer Tool Starting...\n");

    // Load configuration
    let config = load_config("config.yaml")?;

    // Create transfer client
    let sol_transfer = SolTransfer::new(config.solana_rpc_url);

    // Convert SOL to lamports
    let amount_lamports = SolTransfer::sol_to_lamports(config.amount_sol);

    println!("Configuration loaded:");
    println!("- Sender wallets: {}", config.sender_wallets.len());
    println!("- Recipients: {}", config.recipient_addresses.len());
    println!(
        "- Amount per transfer: {} SOL ({} lamports)",
        config.amount_sol, amount_lamports
    );
    println!(
        "- Total transfers: {}\n",
        config.sender_wallets.len() * config.recipient_addresses.len()
    );

    // Execute transfers
    let results = sol_transfer
        .execute_transfers(
            config.sender_wallets,
            config.recipient_addresses,
            amount_lamports,
        )
        .await;

    // Print results and statistics
    sol_transfer.print_statistics(&results);

    println!("\n🎉 Transfer process completed!");

    Ok(())
}
