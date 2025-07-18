use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use rusqlite::{params, Connection, Row};

use crate::types::{BurnRecord, Statistics, WalletSummary};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub async fn new(database_path: &str) -> Result<Self> {
        // Remove sqlite: prefix if present
        let path = database_path.strip_prefix("sqlite:").unwrap_or(database_path);
        
        // Ensure database directory exists
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let conn = Connection::open(path)?;
        Ok(Database { conn })
    }

    pub async fn get_pending_mints(&self, min_amount: u64) -> Result<Vec<BurnRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, signature, burner, amount, memo, token, timestamp, memo_checked, 
                    created_at, is_minted, minted_time, minted_signature 
             FROM burn_records 
             WHERE is_minted = FALSE AND amount >= ?1 
             ORDER BY timestamp ASC"
        )?;

        let record_iter = stmt.query_map(params![min_amount as i64], |row| {
            self.row_to_burn_record(row)
        })?;

        let mut records = Vec::new();
        for record in record_iter {
            records.push(record?);
        }

        Ok(records)
    }

    pub async fn mark_as_minted(&self, signature: &str, minted_signature: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE burn_records SET is_minted = TRUE, minted_time = ?1, minted_signature = ?2 WHERE signature = ?3",
            params![Utc::now().to_rfc3339(), minted_signature, signature],
        )?;
        Ok(())
    }

    pub async fn get_all_records(&self) -> Result<Vec<BurnRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, signature, burner, amount, memo, token, timestamp, memo_checked, 
                    created_at, is_minted, minted_time, minted_signature 
             FROM burn_records 
             ORDER BY timestamp DESC"
        )?;

        let record_iter = stmt.query_map([], |row| {
            self.row_to_burn_record(row)
        })?;

        let mut records = Vec::new();
        for record in record_iter {
            records.push(record?);
        }

        Ok(records)
    }

    pub async fn get_wallet_summaries(&self) -> Result<Vec<WalletSummary>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                burner,
                SUM(CASE WHEN is_minted = FALSE THEN amount ELSE 0 END) as total_burned,
                SUM(CASE WHEN is_minted = TRUE THEN amount ELSE 0 END) as total_minted,
                COUNT(*) as burn_count,
                SUM(CASE WHEN is_minted = TRUE THEN 1 ELSE 0 END) as mint_count,
                MIN(timestamp) as first_burn,
                MAX(minted_time) as last_mint
            FROM burn_records 
            GROUP BY burner 
            ORDER BY total_burned DESC
            "#
        )?;

        let summary_iter = stmt.query_map([], |row| {
            let wallet_address: String = row.get(0)?;
            let total_burned_raw: i64 = row.get(1)?;
            let total_minted_raw: i64 = row.get(2)?;
            let burn_count: i64 = row.get(3)?;
            let mint_count: i64 = row.get(4)?;
            let first_burn_str: Option<String> = row.get(5)?;
            let last_mint_str: Option<String> = row.get(6)?;

            let total_burned = Decimal::from(total_burned_raw) / Decimal::from(1_000_000);
            let total_minted = Decimal::from(total_minted_raw) / Decimal::from(1_000_000);

            let first_burn = first_burn_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc));
            let last_mint = last_mint_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc));

            Ok(WalletSummary {
                wallet_address,
                total_burned,
                total_minted,
                burn_count,
                mint_count,
                first_burn,
                last_mint,
            })
        })?;

        let mut summaries = Vec::new();
        for summary in summary_iter {
            summaries.push(summary?);
        }

        Ok(summaries)
    }

    pub async fn get_statistics(&self) -> Result<Statistics> {
        let total_records: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM burn_records",
            [],
            |row| row.get(0),
        )?;

        let total_burned_raw: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM burn_records",
            [],
            |row| row.get(0),
        )?;

        let total_minted_raw: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM burn_records WHERE is_minted = TRUE",
            [],
            |row| row.get(0),
        )?;

        let unique_wallets: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT burner) FROM burn_records",
            [],
            |row| row.get(0),
        )?;

        let pending_mints: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM burn_records WHERE is_minted = FALSE",
            [],
            |row| row.get(0),
        )?;

        let successful_mints: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM burn_records WHERE is_minted = TRUE",
            [],
            |row| row.get(0),
        )?;

        Ok(Statistics {
            total_records,
            total_burned_amount: Decimal::from(total_burned_raw) / Decimal::from(1_000_000),
            total_minted_amount: Decimal::from(total_minted_raw) / Decimal::from(1_000_000),
            unique_wallets,
            pending_mints,
            successful_mints,
        })
    }

    fn row_to_burn_record(&self, row: &Row) -> rusqlite::Result<BurnRecord> {
        let id: Option<i64> = row.get(0)?;
        let signature: String = row.get(1)?;
        let burner: String = row.get(2)?;
        let amount: i64 = row.get(3)?; // Get raw amount directly
        let memo: Option<String> = row.get(4)?;
        let token: Option<String> = row.get(5)?;
        let timestamp_str: Option<String> = row.get(6)?;
        let memo_checked: Option<String> = row.get(7)?;
        let created_at_str: String = row.get(8)?;
        let is_minted: bool = row.get(9)?;
        let minted_time_str: Option<String> = row.get(10)?;
        let minted_signature: Option<String> = row.get(11)?;

        let timestamp = timestamp_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let minted_time = minted_time_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        Ok(BurnRecord {
            id,
            signature,
            burner,
            amount: amount as u64, // Convert to u64, but keep original raw value
            memo,
            token,
            timestamp,
            memo_checked,
            created_at,
            is_minted,
            minted_time,
            minted_signature,
        })
    }
}
