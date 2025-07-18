use anyhow::Result;
use clap::{Parser, Subcommand};
use log::{error, info};

mod config;
mod database;
mod html;
mod migrator;
mod minter;
mod types;

use config::Config;
use database::Database;
use migrator::DatabaseMigrator;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Migrate data from burns.db to new database
    Migrate {
        /// Only migrate the latest record for this specific burner address
        #[arg(long)]
        burner: Option<String>,
    },
    /// Process minting operations
    Mint,
    /// Generate HTML report
    Generate,
    /// Run full pipeline (migrate -> mint -> generate)
    Run {
        /// Only migrate the latest record for this specific burner address
        #[arg(long)]
        burner: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let cli = Cli::parse();
    let config = Config::load()?;
    
    match cli.command {
        Some(Commands::Migrate { burner }) => {
            info!("Starting data migration");
            let migrator = DatabaseMigrator::new(config);
            migrator.migrate(burner.as_deref()).await?;
        }
        Some(Commands::Mint) => {
            info!("Starting minting process");
            let db = Database::new(&config.database_url).await?;
            let mut minter = minter::TokenMinter::new(&config, &db).await?;
            minter.process_pending_mints().await?;
        }
        Some(Commands::Generate) => {
            info!("Generating HTML report");
            let db = Database::new(&config.database_url).await?;
            let generator = html::HtmlGenerator::new(&db);
            generator.generate().await?;
        }
        Some(Commands::Run { burner }) => {
            info!("Running full pipeline");
            
            // Step 1: Migrate data
            info!("Step 1: Migrating data from burns.db");
            let migrator = DatabaseMigrator::new(config.clone());
            match migrator.migrate(burner.as_deref()).await {
                Ok(count) => {
                    info!("Migrated {} records", count);
                }
                Err(e) => {
                    error!("Migration failed: {}", e);
                    return Err(e);
                }
            }
            
            let db = Database::new(&config.database_url).await?;
            
            // Step 2: Process minting
            info!("Step 2: Processing minting operations");
            let mut minter = minter::TokenMinter::new(&config, &db).await?;
            if let Err(e) = minter.process_pending_mints().await {
                error!("Minting failed: {}", e);
            }
            
            // Step 3: Generate HTML
            info!("Step 3: Generating HTML report");
            let generator = html::HtmlGenerator::new(&db);
            if let Err(e) = generator.generate().await {
                error!("HTML generation failed: {}", e);
            }
        }
        None => {
            info!("Running full pipeline");
            
            // Step 1: Migrate data (no specific burner)
            info!("Step 1: Migrating data from burns.db");
            let migrator = DatabaseMigrator::new(config.clone());
            match migrator.migrate(None).await {
                Ok(count) => {
                    info!("Migrated {} records", count);
                }
                Err(e) => {
                    error!("Migration failed: {}", e);
                    return Err(e);
                }
            }
            
            let db = Database::new(&config.database_url).await?;
            
            // Step 2: Process minting
            info!("Step 2: Processing minting operations");
            let mut minter = minter::TokenMinter::new(&config, &db).await?;
            if let Err(e) = minter.process_pending_mints().await {
                error!("Minting failed: {}", e);
            }
            
            // Step 3: Generate HTML
            info!("Step 3: Generating HTML report");
            let generator = html::HtmlGenerator::new(&db);
            if let Err(e) = generator.generate().await {
                error!("HTML generation failed: {}", e);
            }
        }
    }
    
    info!("Process completed successfully");
    Ok(())
}
