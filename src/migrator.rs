use anyhow::Result;
use chrono::{DateTime, Utc, NaiveDateTime};
use log::{info, warn};
use rusqlite::{params, Connection, Row, OptionalExtension}; // 添加 OptionalExtension
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::str::FromStr;

use crate::config::Config;

pub struct DatabaseMigrator {
    config: Config,
}

impl DatabaseMigrator {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
    
    pub async fn migrate(&self, specific_burner: Option<&str>) -> Result<usize> {
        if !self.config.source_db_path.exists() {
            return Err(anyhow::anyhow!("Source database not found: {:?}", self.config.source_db_path));
        }
        
        if let Some(burner) = specific_burner {
            info!("Starting migration from {:?} for specific burner: {}", self.config.source_db_path, burner);
        } else {
            info!("Starting migration from {:?}", self.config.source_db_path);
        }
        info!("Minimum burn amount: {} solXEN", self.config.min_burn_amount as f64 / 1_000_000.0);
        
        // Open source database
        let source_conn = Connection::open(&self.config.source_db_path)?;
        
        // Create destination database
        let dest_path = self.config.database_url.strip_prefix("sqlite:").unwrap_or(&self.config.database_url);
        if let Some(parent) = std::path::Path::new(dest_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let dest_conn = Connection::open(dest_path)?;
        
        // Create destination table
        self.create_destination_table(&dest_conn)?;
        
        // Migrate data
        let migrated_count = self.migrate_data(&source_conn, &dest_conn, specific_burner).await?;
        
        info!("Migration completed: {} records migrated", migrated_count);
        Ok(migrated_count)
    }
    
    fn create_destination_table(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS burn_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                signature TEXT UNIQUE NOT NULL,
                burner TEXT NOT NULL,
                amount DECIMAL(20,6) NOT NULL,
                memo TEXT,
                token TEXT,
                timestamp DATETIME,
                memo_checked CHAR(1),
                created_at DATETIME NOT NULL,
                is_minted BOOLEAN DEFAULT FALSE NOT NULL,
                minted_time DATETIME,
                minted_signature TEXT
            )
            "#,
            [],
        )?;
        
        // Create indexes
        conn.execute("CREATE INDEX IF NOT EXISTS idx_signature ON burn_records(signature)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_burner ON burn_records(burner)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_amount ON burn_records(amount)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_is_minted ON burn_records(is_minted)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_timestamp ON burn_records(timestamp)", [])?;
        
        info!("Destination table created");
        Ok(())
    }
    
    async fn migrate_data(&self, source_conn: &Connection, dest_conn: &Connection, specific_burner: Option<&str>) -> Result<usize> {
        let mut migrated_count = 0;
        let mut skipped_count = 0;
        let mut below_minimum_count = 0;
        
        // Handle specific burner case
        if let Some(burner) = specific_burner {
            info!("Searching for burn records for burner: {}", burner);
            
            // First, let's check if this burner exists at all
            let burner_count: i64 = source_conn.query_row(
                "SELECT COUNT(*) FROM burns WHERE burner = ?1",
                params![burner],
                |row| row.get(0),
            )?;
            
            info!("Found {} total records for burner {}", burner_count, burner);
            
            if burner_count == 0 {
                warn!("No records found for burner: {}", burner);
                return Ok(0);
            }
            
            // Query all records for this burner, ordered by timestamp DESC
            // We'll process them one by one until we find one that meets the minimum amount
            let mut stmt = source_conn.prepare(
                "SELECT signature, burner, amount, memo, token, timestamp, memo_checked, created_at 
                 FROM burns 
                 WHERE burner = ?1 
                 ORDER BY timestamp DESC"
            )?;
            
            let record_iter = stmt.query_map(params![burner], |row| {
                self.row_to_burn_record(row)
            })?;
            
            let mut found_valid_record = false;
            
            for record_result in record_iter {
                let record = record_result?;
                
                info!(
                    "Checking record: burner={}, amount={}, signature={}", 
                    record.burner,
                    record.amount,
                    &record.signature[..std::cmp::min(8, record.signature.len())]
                );
                
                // Check if this record already exists in destination
                let exists: i64 = dest_conn.query_row(
                    "SELECT COUNT(*) FROM burn_records WHERE signature = ?1",
                    params![record.signature],
                    |row| row.get(0),
                )?;
                
                if exists > 0 {
                    skipped_count += 1;
                    info!("Record {} already exists, checking next record", record.signature);
                    continue;
                }
                
                // Check if amount meets minimum requirement
                let raw_amount = record.amount.to_u64().unwrap_or(0);
                
                info!("Raw amount: {}, Min required: {}", raw_amount, self.config.min_burn_amount);
                
                if raw_amount < self.config.min_burn_amount {
                    info!("Amount {} below minimum, checking next record", raw_amount);
                    continue; // Keep looking for a record that meets the requirement
                }
                
                // Found a valid record, process it
                info!(
                    "Found valid record: burner={}, amount={} ({}), signature={}", 
                    record.burner,
                    raw_amount,
                    record.amount,
                    &record.signature[..std::cmp::min(8, record.signature.len())]
                );
                
                // Insert the record
                dest_conn.execute(
                    r#"
                    INSERT INTO burn_records (
                        signature, burner, amount, memo, token, timestamp, memo_checked, 
                        created_at, is_minted, minted_time, minted_signature
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                    "#,
                    params![
                        record.signature,
                        record.burner,
                        raw_amount as i64,
                        record.memo,
                        record.token,
                        record.timestamp.map(|t| t.to_rfc3339()),
                        record.memo_checked,
                        record.created_at.to_rfc3339(),
                        false, // is_minted default to false
                        None::<String>, // minted_time
                        None::<String>, // minted_signature
                    ],
                )?;
                
                migrated_count += 1;
                found_valid_record = true;
                info!("Successfully migrated 1 record for burner {}", burner);
                break; // Only migrate one record per burner
            }
            
            if !found_valid_record {
                warn!("No records found for burner {} that meet the minimum amount requirement (420 solXEN)", burner);
            }
            
        } else {
            info!("Migrating all burn records");
            
            // Query all records
            let mut stmt = source_conn.prepare(
                "SELECT signature, burner, amount, memo, token, timestamp, memo_checked, created_at 
                 FROM burns 
                 ORDER BY timestamp DESC"
            )?;
            
            let record_iter = stmt.query_map([], |row| {
                self.row_to_burn_record(row)
            })?;
            
            for record_result in record_iter {
                let record = record_result?;
                migrated_count += self.process_single_record(record, dest_conn, &mut skipped_count, &mut below_minimum_count).await?;
            }
        }
        
        if skipped_count > 0 {
            info!("Skipped {} existing records", skipped_count);
        }
        
        if below_minimum_count > 0 {
            info!("Skipped {} records below minimum burn amount (420 solXEN)", below_minimum_count);
        }
        
        if specific_burner.is_some() && migrated_count == 0 && skipped_count == 0 && below_minimum_count == 0 {
            warn!("No qualifying records found for burner: {}", specific_burner.unwrap());
        }
        
        Ok(migrated_count)
    }
    
    // 新增辅助方法来处理单个记录
    async fn process_single_record(
        &self, 
        record: BurnRecordSource, 
        dest_conn: &Connection, 
        skipped_count: &mut usize, 
        below_minimum_count: &mut usize
    ) -> Result<usize> {
        info!(
            "Processing record: burner={}, amount={}, signature={}", 
            record.burner,
            record.amount,
            &record.signature[..std::cmp::min(8, record.signature.len())]
        );
        
        // Check if record already exists
        let exists: i64 = dest_conn.query_row(
            "SELECT COUNT(*) FROM burn_records WHERE signature = ?1",
            params![record.signature],
            |row| row.get(0),
        )?;
        
        if exists > 0 {
            *skipped_count += 1;
            info!("Record {} already exists, skipping", record.signature);
            return Ok(0);
        }
        
        // 直接使用原始amount值，不做任何转换
        let raw_amount = record.amount.to_u64().unwrap_or(0);
        
        info!("Raw amount: {}, Min required: {}", raw_amount, self.config.min_burn_amount);
        
        // 最小值检查：420 solXEN = 420000000 (按6位小数计算)
        if raw_amount < self.config.min_burn_amount {
            *below_minimum_count += 1;
            info!("Skipping burn with amount {} (below minimum of 420 solXEN)", record.amount);
            return Ok(0);
        }
        
        info!(
            "Migrating record: burner={}, amount={} ({}), signature={}", 
            record.burner,
            raw_amount,
            record.amount,
            &record.signature[..std::cmp::min(8, record.signature.len())]
        );
        
        // Insert the record with original amount (直接复制原始值)
        dest_conn.execute(
            r#"
            INSERT INTO burn_records (
                signature, burner, amount, memo, token, timestamp, memo_checked, 
                created_at, is_minted, minted_time, minted_signature
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                record.signature,
                record.burner,
                raw_amount as i64, // 直接存储：420690000 -> 420690000
                record.memo,
                record.token,
                record.timestamp.map(|t| t.to_rfc3339()),
                record.memo_checked,
                record.created_at.to_rfc3339(),
                false, // is_minted default to false
                None::<String>, // minted_time
                None::<String>, // minted_signature
            ],
        )?;
        
        info!("Successfully migrated 1 record");
        Ok(1)
    }
    
    fn row_to_burn_record(&self, row: &Row) -> rusqlite::Result<BurnRecordSource> {
        let signature: String = row.get("signature")?;
        let burner: String = row.get("burner")?;
        
        // Handle amount - could be stored as TEXT, REAL, or INTEGER
        let amount = if let Ok(amount_str) = row.get::<_, String>("amount") {
            Decimal::from_str(&amount_str).unwrap_or(Decimal::ZERO)
        } else if let Ok(amount_f64) = row.get::<_, f64>("amount") {
            Decimal::from_f64_retain(amount_f64).unwrap_or(Decimal::ZERO)
        } else if let Ok(amount_i64) = row.get::<_, i64>("amount") {
            Decimal::from(amount_i64)
        } else {
            Decimal::ZERO
        };
        
        let memo: Option<String> = row.get("memo").ok();
        let token: Option<String> = row.get("token").ok();
        let memo_checked: Option<String> = row.get("memo_checked").ok();
        
        // Handle timestamp - could be stored as INTEGER (unix timestamp) or STRING
        let timestamp = if let Ok(timestamp_int) = row.get::<_, i64>("timestamp") {
            // Convert unix timestamp to DateTime
            Some(DateTime::from_timestamp(timestamp_int, 0).unwrap_or(Utc::now()))
        } else if let Ok(timestamp_str) = row.get::<_, String>("timestamp") {
            // Parse string timestamp
            self.parse_datetime(&timestamp_str).ok()
        } else {
            None
        };
        
        // Handle created_at - could be stored as INTEGER or STRING
        let created_at = if let Ok(created_at_int) = row.get::<_, i64>("created_at") {
            // Convert unix timestamp to DateTime
            DateTime::from_timestamp(created_at_int, 0).unwrap_or(Utc::now())
        } else if let Ok(created_at_str) = row.get::<_, String>("created_at") {
            // Parse string timestamp
            self.parse_datetime(&created_at_str).unwrap_or_else(|_| Utc::now())
        } else {
            Utc::now()
        };
        
        Ok(BurnRecordSource {
            signature,
            burner,
            amount,
            memo,
            token,
            timestamp,
            memo_checked,
            created_at,
        })
    }
    
    fn parse_datetime(&self, date_str: &str) -> Result<DateTime<Utc>> {
        // Try different datetime formats
        if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
            return Ok(dt.with_timezone(&Utc));
        }
        
        if let Ok(dt) = DateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S") {
            return Ok(dt.with_timezone(&Utc));
        }
        
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        Err(anyhow::anyhow!("Unable to parse datetime: {}", date_str))
    }
}

// Temporary structure for source data
#[derive(Debug)]
struct BurnRecordSource {
    signature: String,
    burner: String,
    amount: Decimal,
    memo: Option<String>,
    token: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    memo_checked: Option<String>,
    created_at: DateTime<Utc>,
}