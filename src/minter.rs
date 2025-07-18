use anyhow::Result;
use log::{error, info, warn};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_instruction,
    transaction::Transaction,
};
use spl_token_2022::{
    instruction as token_instruction,
    state::Mint,
};
use spl_associated_token_account::{
    get_associated_token_address_with_program_id,
    instruction::create_associated_token_account,
};
use std::fs::File;
use std::io::Read;
use std::str::FromStr;
use std::time::Duration;

use crate::config::Config;
use crate::database::Database;
use crate::types::BurnRecord;

pub struct TokenMinter<'a> {
    config: &'a Config,
    db: &'a Database,
    rpc_client: RpcClient,
    mint_authority: Option<Keypair>,
    token_mint: Pubkey,
}

impl<'a> TokenMinter<'a> {
    pub async fn new(config: &'a Config, db: &'a Database) -> Result<Self> {
        let rpc_client = RpcClient::new_with_commitment(
            config.x1_rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );
        
        match rpc_client.get_version() {
            Ok(version) => info!("Connected to X1 testnet, version: {}", version.solana_core),
            Err(e) => {
                error!("Failed to connect to X1 testnet: {}", e);
                return Err(e.into());
            }
        }

        let mint_authority = Self::load_keypair(&config.keypair_path)?;
        let token_mint = Pubkey::from_str(&config.token_mint)?;
        
        // 验证这是一个 Token 2022 铸造账户
        match rpc_client.get_account(&token_mint) {
            Ok(mint_account) => {
                info!("Token mint found on X1 testnet");
                info!("Token mint address: {}", token_mint);
                info!("Token account owner: {}", mint_account.owner);
                info!("Token account lamports: {}", mint_account.lamports);
                
                // 检查是否为 Token 2022 程序
                let token_2022_program_id = spl_token_2022::id();
                if mint_account.owner == token_2022_program_id {
                    info!("✅ Confirmed: This is a Token 2022 mint");
                } else {
                    warn!("⚠️  Warning: Token mint owner is not Token 2022 program");
                    warn!("   Expected: {}", token_2022_program_id);
                    warn!("   Actual: {}", mint_account.owner);
                }
                
                // 尝试解析 Token 2022 mint 数据
                if mint_account.data.len() >= 82 { // 最小 Token 2022 mint 大小
                    info!("Mint account data length: {} bytes", mint_account.data.len());
                } else {
                    warn!("Mint account data seems too small for Token 2022");
                }
            }
            Err(e) => {
                error!("Failed to find token mint {} on X1 testnet: {}", token_mint, e);
                return Err(e.into());
            }
        }
        
        if let Some(ref keypair) = mint_authority {
            info!("Loaded mint authority: {}", keypair.pubkey());
            
            match rpc_client.get_balance(&keypair.pubkey()) {
                Ok(balance) => {
                    let balance_sol = balance as f64 / 1_000_000_000.0;
                    info!("Mint authority balance: {:.4} SOL", balance_sol);
                    
                    if balance_sol < 0.01 {
                        warn!("Low SOL balance! May not be enough for Token 2022 transactions");
                    }
                }
                Err(e) => {
                    warn!("Could not check mint authority balance: {}", e);
                }
            }
        } else {
            warn!("No mint authority loaded - running in simulation mode");
        }

        Ok(TokenMinter {
            config,
            db,
            rpc_client,
            mint_authority,
            token_mint,
        })
    }
    
    fn load_keypair(path: &std::path::Path) -> Result<Option<Keypair>> {
        if !path.exists() {
            warn!("Keypair file not found: {:?}", path);
            return Ok(None);
        }
        
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        
        let keypair_bytes: Vec<u8> = serde_json::from_slice(&data)?;
        let keypair = Keypair::from_bytes(&keypair_bytes)?;
        
        Ok(Some(keypair))
    }
    
    pub async fn process_pending_mints(&mut self) -> Result<()> {
        let pending_records = self.db.get_pending_mints(self.config.min_burn_amount).await?;
        
        if pending_records.is_empty() {
            info!("✅ No pending mint operations found");
            return Ok(());
        }
        
        info!("🚀 Starting mint operations");
        info!("   Found {} pending mint operations", pending_records.len());
        info!("   Minimum burn amount: {} solXEN", self.config.min_burn_amount as f64 / 1_000_000.0);
        info!("   Token mint address: {}", self.config.token_mint);
        
        if self.mint_authority.is_some() {
            info!("   Mode: REAL MINTING");
        } else {
            info!("   Mode: SIMULATION (no keypair loaded)");
        }
        
        println!(""); // Add blank line for readability
        
        for record in pending_records {
            info!(
                "Processing mint: {} -> {} solXEN (raw: {})",
                record.burner, 
                record.amount_as_decimal(),
                record.amount
            );
            
            match self.mint_tokens(&record).await {
                Ok(signature) => {
                    info!("✅ Mint transaction successful!");
                    info!("   Burner: {}", record.burner);
                    info!("   Amount: {} solXEN ({} raw units)", record.amount_as_decimal(), record.amount);
                    info!("   Burn Signature: {}", record.signature);
                    info!("   Mint Signature: {}", signature);
                    info!("   X1 Explorer: https://explorer.x1-testnet.xen.network/tx/{}", signature);
                    
                    if let Err(e) = self.db.mark_as_minted(&record.signature, &signature).await {
                        error!("❌ Failed to update mint status in database: {}", e);
                    } else {
                        info!("✅ Database updated successfully");
                    }
                }
                Err(e) => {
                    error!("❌ Mint failed for {}: {}", record.burner, e);
                    error!("   Burn Signature: {}", record.signature);
                    error!("   Amount: {} solXEN ({} raw units)", record.amount_as_decimal(), record.amount);
                }
            }
            
            // Wait between transactions to avoid rate limiting
            tokio::time::sleep(Duration::from_secs(2)).await;
            println!(""); // Add blank line between transactions
        }
        
        info!("🏁 Mint operations completed");
        
        // Get updated statistics
        if let Ok(stats) = self.db.get_statistics().await {
            info!("📊 Updated Statistics:");
            info!("   Total records: {}", stats.total_records);
            info!("   Pending mints: {}", stats.pending_mints);
            info!("   Successful mints: {}", stats.successful_mints);
            info!("   Total minted: {} solXEN", stats.total_minted_amount);
        }
        
        Ok(())
    }
    
    async fn mint_tokens(&self, record: &BurnRecord) -> Result<String> {
        if self.mint_authority.is_none() {
            return self.simulate_mint(record).await;
        }

        let mint_authority = self.mint_authority.as_ref().unwrap();
        let recipient = Pubkey::from_str(&record.burner)?;
        
        info!(
            "Minting {} raw units ({} solXEN) to {} on X1 testnet using Token 2022", 
            record.amount,
            record.amount_as_decimal(),
            record.burner
        );
        
        // Token 2022 程序 ID
        let token_program_id = spl_token_2022::id();
        info!("Using Token 2022 program ID: {}", token_program_id);
        
        // 获取关联代币账户地址（使用 Token 2022 程序 ID）
        let recipient_token_account = get_associated_token_address_with_program_id(
            &recipient,
            &self.token_mint,
            &token_program_id,
        );
        
        info!("Recipient token account: {}", recipient_token_account);
        
        // 检查关联代币账户是否存在
        let mut instructions = Vec::new();
        
        match self.rpc_client.get_account(&recipient_token_account) {
            Ok(account) => {
                info!("Associated token account already exists");
                info!("Account owner: {}", account.owner);
                info!("Account lamports: {}", account.lamports);
            }
            Err(_) => {
                info!("Creating associated token account for recipient using Token 2022");
                let create_ata_ix = create_associated_token_account(
                    &mint_authority.pubkey(), // payer
                    &recipient,               // wallet
                    &self.token_mint,         // mint
                    &token_program_id,        // token program (Token 2022)
                );
                instructions.push(create_ata_ix);
            }
        }
        
        // 创建 Token 2022 铸造指令
        let mint_ix = token_instruction::mint_to(
            &token_program_id,                   // Token 2022 程序 ID
            &self.token_mint,                    // mint
            &recipient_token_account,            // destination
            &mint_authority.pubkey(),            // mint authority
            &[&mint_authority.pubkey()],         // signer pubkeys
            record.amount,                       // amount (raw units with 6 decimals)
        )?;
        
        instructions.push(mint_ix);
        
        // 获取最新的区块哈希
        let recent_blockhash = self.rpc_client.get_latest_blockhash()?;
        
        // 创建并签名交易
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&mint_authority.pubkey()),
            &[mint_authority],
            recent_blockhash,
        );
        
        // 发送交易
        info!("📤 Sending Token 2022 mint transaction...");
        info!("   From: {} (mint authority)", mint_authority.pubkey());
        info!("   To: {} (recipient)", recipient);
        info!("   Token Account: {}", recipient_token_account);
        info!("   Amount: {} solXEN ({} raw units)", record.amount_as_decimal(), record.amount);
        info!("   Token Program: {} (Token 2022)", token_program_id);
        info!("   Mint Address: {}", self.token_mint);
        
        let signature = self.rpc_client.send_and_confirm_transaction(&transaction)?;
        
        info!("🎉 Token 2022 mint transaction confirmed!");
        info!("   Transaction Signature: {}", signature);
        info!("   X1 Testnet Explorer: https://explorer.x1-testnet.xen.network/tx/{}", signature);
        
        Ok(signature.to_string())
    }
    
    async fn simulate_mint(&self, record: &BurnRecord) -> Result<String> {
        info!("🎭 SIMULATION MODE - No real Token 2022 transaction will be sent");
        info!("   Would mint: {} raw units ({} solXEN) -> {}", 
            record.amount,
            record.amount_as_decimal(),
            record.burner
        );
        info!("   Using Token 2022 program: {}", spl_token_2022::id());
        
        // Create a deterministic but realistic-looking signature
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        record.signature.hash(&mut hasher);
        record.burner.hash(&mut hasher);
        "token2022".hash(&mut hasher);
        let hash = hasher.finish();
        
        // Format as a base58-like signature
        let mock_signature = format!("tk22{:x}mock{:x}test", hash, record.amount);
        
        // Simulate network delay
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        info!("🎭 Simulated Token 2022 mint transaction: {}", mock_signature);
        info!("   This is a MOCK signature for testing purposes");
        
        Ok(mock_signature)
    }
}