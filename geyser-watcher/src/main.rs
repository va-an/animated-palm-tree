use {
    futures::{sink::SinkExt, stream::StreamExt},
    serde::{Deserialize, Serialize},
    // solana_client::rpc_client::RpcClient,
    // solana_sdk::{
    //     commitment_config::CommitmentConfig,
    //     native_token::LAMPORTS_PER_SOL,
    //     pubkey::Pubkey,
    //     signature::{Keypair, Signer},
    //     system_instruction,
    //     transaction::Transaction,
    // },
    std::{collections::HashMap, fs, time::Duration},
    tonic::transport::channel::ClientTlsConfig,
    yellowstone_grpc_client::{GeyserGrpcClient, GeyserGrpcClientError},
    yellowstone_grpc_proto::{
        geyser::{
            SubscribeRequest, SubscribeRequestFilterBlocks, SubscribeRequestPing,
            subscribe_update::UpdateOneof,
        },
        tonic::service::Interceptor,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    // /// Private key of the sender (base58 encoded)
    // sender_private_key: String,
    // /// Recipient wallet address
    // recipient_address: String,
    // /// Amount to transfer in SOL
    // transfer_amount: f64,
    // /// Solana RPC endpoint
    // solana_rpc_url: String,
    /// Geyser gRPC endpoint
    geyser_endpoint: String,
    /// X-Token for Geyser authentication
    geyser_x_token: String,
}

impl Config {
    fn load_from_file(path: &str) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;

        let mut config: Config = serde_yaml::from_str(&content)?;
        let geyser_x_token =
            std::env::var("GEYSER_X_TOKEN").expect("env GEYSER_X_TOKEN must be set");
        config.geyser_x_token = geyser_x_token;

        Ok(config)
    }

    // fn get_sender_keypair(&self) -> anyhow::Result<Keypair> {
    //     let private_key_bytes = bs58::decode(&self.sender_private_key).into_vec()?;
    //     Ok(Keypair::from_bytes(&private_key_bytes)?)
    // }

    // fn get_recipient_pubkey(&self) -> anyhow::Result<Pubkey> {
    //     Ok(Pubkey::from_str(&self.recipient_address)?)
    // }

    // fn get_transfer_amount_lamports(&self) -> u64 {
    //     (self.transfer_amount * LAMPORTS_PER_SOL as f64) as u64
    // }
}

struct SolTransferBot {
    config: Config,
    // solana_client: RpcClient,
}

impl SolTransferBot {
    fn new(config: Config) -> anyhow::Result<Self> {
        // let solana_client = RpcClient::new_with_commitment(
        //     config.solana_rpc_url.clone(),
        //     CommitmentConfig::confirmed(),
        // );

        Ok(Self {
            config,
            // solana_client,
        })
    }

    async fn connect_geyser(&self) -> anyhow::Result<GeyserGrpcClient<impl Interceptor>> {
        let client = GeyserGrpcClient::build_from_shared(self.config.geyser_endpoint.clone())?
            .x_token(Some(self.config.geyser_x_token.clone()))?
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(10))
            .tls_config(ClientTlsConfig::new().with_native_roots())?
            .max_decoding_message_size(1024 * 1024 * 1024)
            .connect()
            .await?;

        Ok(client)
    }

    fn create_block_subscription_request(&self) -> SubscribeRequest {
        let mut blocks = HashMap::new();

        blocks.insert(
            "blocks".to_owned(),
            SubscribeRequestFilterBlocks {
                account_include: vec![],
                include_transactions: Some(false),
                include_accounts: Some(false),
                include_entries: Some(false),
            },
        );

        SubscribeRequest {
            accounts: HashMap::default(),
            slots: HashMap::default(),
            transactions: HashMap::default(),
            transactions_status: HashMap::default(),
            blocks,
            blocks_meta: HashMap::default(),
            entry: HashMap::default(),
            commitment: Some(yellowstone_grpc_proto::geyser::CommitmentLevel::Confirmed as i32),
            accounts_data_slice: Vec::default(),
            ping: None,
            from_slot: None,
        }
    }

    // async fn transfer_sol(&self) -> anyhow::Result<String> {
    //     let sender_keypair = self.config.get_sender_keypair()?;
    //     let recipient_pubkey = self.config.get_recipient_pubkey()?;
    //     let amount_lamports = self.config.get_transfer_amount_lamports();

    //     println!(
    //         "Transferring {} SOL from {} to {}",
    //         self.config.transfer_amount,
    //         sender_keypair.pubkey(),
    //         recipient_pubkey
    //     );

    //     // Get recent blockhash
    //     let recent_blockhash = self.solana_client.get_latest_blockhash()?;

    //     // Create transfer instruction
    //     let transfer_instruction = system_instruction::transfer(
    //         &sender_keypair.pubkey(),
    //         &recipient_pubkey,
    //         amount_lamports,
    //     );

    //     // Create and sign transaction
    //     let transaction = Transaction::new_signed_with_payer(
    //         &[transfer_instruction],
    //         Some(&sender_keypair.pubkey()),
    //         &[&sender_keypair],
    //         recent_blockhash,
    //     );

    //     // Send transaction
    //     let signature = self
    //         .solana_client
    //         .send_and_confirm_transaction(&transaction)?;

    //     println!("Transfer successful! Signature: {}", signature);
    //     Ok(signature.to_string())
    // }

    async fn run(&self) -> anyhow::Result<()> {
        let mut geyser_client = self.connect_geyser().await?;
        let request = self.create_block_subscription_request();
        let (mut subscribe_tx, mut stream) =
            geyser_client.subscribe_with_request(Some(request)).await?;

        println!("Subscribed to new blocks. Waiting for blocks...");

        while let Some(message) = stream.next().await {
            match message {
                Ok(msg) => match msg.update_oneof {
                    Some(UpdateOneof::Block(block_update)) => {
                        println!(
                            "🆕 New block detected! Slot: {}, Hash: {}, Height: {:?}",
                            block_update.slot, block_update.blockhash, block_update.block_height
                        );

                        // Execute SOL transfer (commented out)
                        // match self.transfer_sol().await {
                        //     Ok(signature) => {
                        //         println!("✅ SOL transfer completed: {}", signature);
                        //     }
                        //     Err(e) => {
                        //         println!("❌ Failed to transfer SOL: {}", e);
                        //     }
                        // }
                    }
                    Some(UpdateOneof::Ping(_)) => {
                        subscribe_tx
                            .send(SubscribeRequest {
                                ping: Some(SubscribeRequestPing { id: 1 }),
                                ..Default::default()
                            })
                            .await?;
                    }
                    Some(UpdateOneof::Pong(_)) => {
                        // Pong received, connection is healthy
                    }
                    None => {
                        println!("❌ Empty update received");
                        break;
                    }
                    _ => {
                        // Other update types (slots, transactions, etc.)
                    }
                },
                Err(error) => {
                    println!("❌ Stream error: {:?}", error);
                    println!("🔄 Attempting to reconnect...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    break;
                }
            }
        }

        println!("Block subscription stream closed");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = Config::load_from_file("config.yaml")?;
    println!("Configuration loaded from config.yaml");

    // Validate configuration (commented out)
    // config.get_sender_keypair()?;
    // config.get_recipient_pubkey()?;

    // println!("Sender address: {}", config.get_sender_keypair()?.pubkey());
    // println!("Recipient address: {}", config.recipient_address);
    // println!("Transfer amount: {} SOL", config.transfer_amount);

    // Create and run the bot
    let bot = SolTransferBot::new(config)?;

    loop {
        if let Err(e) = bot.run().await {
            println!("❌ Bot error: {}. Restarting in 10 seconds...", e);
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }
}
