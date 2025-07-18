use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BurnRecord {
    pub id: Option<i64>,
    pub signature: String,
    pub burner: String,
    pub amount: u64, // Raw amount with 6 decimals (for X1)
    pub memo: Option<String>,
    pub token: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub memo_checked: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_minted: bool,
    pub minted_time: Option<DateTime<Utc>>,
    pub minted_signature: Option<String>,
}

impl BurnRecord {
    /// Convert raw amount to human readable format (divide by 10^6)
    pub fn amount_as_decimal(&self) -> Decimal {
        Decimal::from(self.amount) / Decimal::from(1_000_000)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WalletSummary {
    pub wallet_address: String,
    pub total_burned: Decimal,
    pub total_minted: Decimal,
    pub burn_count: i64,
    pub mint_count: i64,
    pub first_burn: Option<DateTime<Utc>>,
    pub last_mint: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Statistics {
    pub total_records: i64,
    pub total_burned_amount: Decimal,
    pub total_minted_amount: Decimal,
    pub unique_wallets: i64,
    pub pending_mints: i64,
    pub successful_mints: i64,
}
