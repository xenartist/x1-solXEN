use anyhow::Result;
use dirs::home_dir;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub source_db_path: PathBuf,
    pub x1_rpc_url: String,
    pub token_mint: String,
    pub keypair_path: PathBuf,
    pub min_burn_amount: u64, // Changed to u64 for raw amount (420690000 = 420.69 solXEN)
}

impl Config {
    pub fn load() -> Result<Self> {
        let home = home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        
        Ok(Config {
            database_url: "sqlite:database/sol_burn_x1_mint.db".to_string(),
            source_db_path: PathBuf::from("burn-data/burns.db"),
            x1_rpc_url: "https://rpc-testnet.x1.wiki".to_string(),
            token_mint: "2oaSsGnq1eNjMavSxh1g2XFqtV7SVYwaRJZaBznMyYJT".to_string(),
            keypair_path: home.join(".config/solana/id.json"),
            min_burn_amount: 420_000_000, // 420 solXEN in raw amount (6 decimals)
        })
    }
}
