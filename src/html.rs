use anyhow::Result;
use chrono::Utc;
use log::info;
use tera::{Context, Tera};
use serde::{Serialize, Deserialize};
use rust_decimal::prelude::ToPrimitive;

use crate::database::Database;

//  Template for the HTML report
#[derive(Serialize, Deserialize)]
struct TemplateWalletSummary {
    pub wallet_address: String,
    pub total_burned: f64,
    pub total_minted: f64,
    pub burn_count: i64,
    pub mint_count: i64,
    pub first_burn: Option<String>,
    pub last_mint: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct TemplateStatistics {
    pub total_records: i64,
    pub total_burned_amount: f64,
    pub total_minted_amount: f64,
    pub unique_wallets: i64,
    pub pending_mints: i64,
    pub successful_mints: i64,
}

#[derive(Serialize, Deserialize)]
struct TemplateBurnRecord {
    pub id: Option<i64>,
    pub signature: String,
    pub burner: String,
    pub amount_decimal: f64, // Convert to f64 for display
    pub memo: Option<String>,
    pub token: Option<String>,
    pub timestamp: Option<String>,
    pub memo_checked: Option<String>,
    pub created_at: String,
    pub is_minted: bool,
    pub minted_time: Option<String>,
    pub minted_signature: Option<String>,
}

pub struct HtmlGenerator<'a> {
    db: &'a Database,
}

impl<'a> HtmlGenerator<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
    
    pub async fn generate(&self) -> Result<()> {
        let records = self.db.get_all_records().await?;
        let wallet_summaries = self.db.get_wallet_summaries().await?;
        let stats = self.db.get_statistics().await?;
        
        // Convert data to template-friendly format
        let template_records: Vec<TemplateBurnRecord> = records.into_iter().map(|record| {
            // Calculate values first to avoid partial moves
            let amount_decimal = record.amount_as_decimal().to_f64().unwrap_or(0.0);
            let timestamp_str = record.timestamp.map(|t| t.format("%Y-%m-%d %H:%M").to_string());
            let created_at_str = record.created_at.format("%Y-%m-%d %H:%M").to_string();
            let minted_time_str = record.minted_time.map(|t| t.format("%Y-%m-%d %H:%M").to_string());
            
            TemplateBurnRecord {
                id: record.id,
                signature: record.signature,
                burner: record.burner,
                amount_decimal,
                memo: record.memo,
                token: record.token,
                timestamp: timestamp_str,
                memo_checked: record.memo_checked,
                created_at: created_at_str,
                is_minted: record.is_minted,
                minted_time: minted_time_str,
                minted_signature: record.minted_signature,
            }
        }).collect();
        
        let template_wallet_summaries: Vec<TemplateWalletSummary> = wallet_summaries.into_iter().map(|wallet| {
            // Calculate values first to avoid partial moves
            let total_burned_f64 = wallet.total_burned.to_f64().unwrap_or(0.0);
            let total_minted_f64 = wallet.total_minted.to_f64().unwrap_or(0.0);
            let first_burn_str = wallet.first_burn.map(|t| t.format("%Y-%m-%d %H:%M").to_string());
            let last_mint_str = wallet.last_mint.map(|t| t.format("%Y-%m-%d %H:%M").to_string());
            
            TemplateWalletSummary {
                wallet_address: wallet.wallet_address,
                total_burned: total_burned_f64,
                total_minted: total_minted_f64,
                burn_count: wallet.burn_count,
                mint_count: wallet.mint_count,
                first_burn: first_burn_str,
                last_mint: last_mint_str,
            }
        }).collect();
        
        let template_stats = TemplateStatistics {
            total_records: stats.total_records,
            total_burned_amount: stats.total_burned_amount.to_f64().unwrap_or(0.0),
            total_minted_amount: stats.total_minted_amount.to_f64().unwrap_or(0.0),
            unique_wallets: stats.unique_wallets,
            pending_mints: stats.pending_mints,
            successful_mints: stats.successful_mints,
        };
        
        let template = self.get_template();
        
        let mut context = Context::new();
        context.insert("records", &template_records);
        context.insert("wallet_summaries", &template_wallet_summaries);
        context.insert("stats", &template_stats);
        context.insert("last_updated", &Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string());
        
        let html = template.render("index", &context)?;
        
        std::fs::write("index.html", html)?;
        info!("HTML report generated: index.html");
        
        Ok(())
    }
    
    fn get_template(&self) -> Tera {
        let mut tera = Tera::new("templates/*").unwrap_or_else(|_| Tera::new("").unwrap());
        tera.add_raw_template("index", &self.get_template_content()).unwrap();
        tera
    }
    
    fn get_template_content(&self) -> String {
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>solXEN  - X1 Testnet</title>
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.0.0/css/all.min.css">
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background-color: #f5f5f5;
            color: #333;
            line-height: 1.6;
            display: flex;
            flex-direction: column;
            min-height: 100vh;
        }

        .container {
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            flex: 1;
            width: 100%;
        }

        /* Header styles */
        .header {
            text-align: center;
            margin-bottom: 30px;
            position: relative;
        }

        .header h1 {
            font-size: 3rem;
            color: #2c3e50;
            margin-bottom: 10px;
        }

        .header p {
            font-size: 1.2rem;
            color: #7f8c8d;
        }

        /* Stats Section */
        .stats-section {
            display: flex;
            justify-content: center;
            margin-bottom: 30px;
        }

        .stats-container {
            width: 100%;
            max-width: 800px;
        }

        .stats-grid {
            display: grid;
            grid-template-columns: repeat(4, 1fr);
            gap: 12px;
            padding: 12px;
            background-color: white;
            border-radius: 10px;
            box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
        }

        .stat-item {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            padding: 15px;
            border-radius: 6px;
            transition: all 0.3s ease;
            border: 1px solid transparent;
            min-height: 80px;
            background-color: #f8f9fa;
        }

        .stat-item.burn {
            background-color: #d4edda;
            color: #155724;
            border-color: #c3e6cb;
        }

        .stat-item.mint {
            background-color: #cce5ff;
            color: #004085;
            border-color: #b3d9ff;
        }

        .stat-item.pending {
            background-color: #fff3cd;
            color: #856404;
            border-color: #ffeaa7;
        }

        .stat-item.wallet {
            background-color: #e2e3e5;
            color: #383d41;
            border-color: #d6d8db;
        }

        .stat-icon {
            font-size: 24px;
            margin-bottom: 8px;
        }

        .stat-label {
            font-size: 12px;
            font-weight: 500;
            text-align: center;
            margin-bottom: 4px;
        }

        .stat-value {
            font-size: 18px;
            font-weight: bold;
            text-align: center;
        }

        /* Results sections */
        .results {
            background-color: white;
            border-radius: 10px;
            box-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
            padding: 30px;
            margin-bottom: 40px;
            width: 100%;
        }

        .result-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 20px;
            padding-bottom: 15px;
            border-bottom: 2px solid #eee;
        }

        .result-header h2 {
            color: #2c3e50;
            font-size: 1.5rem;
            display: flex;
            align-items: center;
            gap: 10px;
        }

        .result-type {
            background-color: #3498db;
            color: white;
            padding: 8px 16px;
            border-radius: 20px;
            font-size: 0.9rem;
            font-weight: 500;
        }

        /* Search Container */
        .search-container {
            margin-bottom: 20px;
        }

        .search-box {
            position: relative;
            max-width: 400px;
        }

        .search-box input {
            width: 100%;
            padding: 12px 40px 12px 16px;
            border: 2px solid #ddd;
            border-radius: 25px;
            font-size: 14px;
            transition: border-color 0.3s;
        }

        .search-box input:focus {
            outline: none;
            border-color: #3498db;
        }

        .search-box i {
            position: absolute;
            right: 15px;
            top: 50%;
            transform: translateY(-50%);
            color: #999;
        }

        /* Table styles */
        .table-container {
            overflow-x: auto;
            border-radius: 8px;
            box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
        }

        table {
            width: 100%;
            border-collapse: collapse;
            background-color: white;
        }

        thead {
            background-color: #f8f9fa;
        }

        th, td {
            padding: 15px;
            text-align: left;
            border-bottom: 1px solid #dee2e6;
        }

        th {
            font-weight: 600;
            color: #495057;
            text-transform: uppercase;
            font-size: 0.85rem;
            letter-spacing: 0.5px;
        }

        tbody tr:hover {
            background-color: #f8f9fa;
        }

        .address-link {
            color: #3498db;
            text-decoration: none;
            font-family: 'Courier New', monospace;
            font-weight: 500;
        }

        .address-link:hover {
            text-decoration: underline;
        }

        .amount {
            font-weight: 600;
            color: #27ae60;
            text-align: right;
        }

        .status-badge {
            padding: 6px 12px;
            border-radius: 12px;
            font-size: 0.8rem;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        .status-badge.success {
            background-color: #d4edda;
            color: #155724;
        }

        .status-badge.pending {
            background-color: #fff3cd;
            color: #856404;
        }

        .status-badge.error {
            background-color: #f8d7da;
            color: #721c24;
        }

        /* Footer */
        .footer {
            background-color: #2c3e50;
            color: white;
            text-align: center;
            padding: 20px;
            margin-top: auto;
        }

        .footer-content p {
            margin-bottom: 5px;
        }

        /* Responsive design */
        @media (max-width: 768px) {
            .container {
                padding: 10px;
            }

            .header h1 {
                font-size: 2rem;
            }

            .stats-grid {
                grid-template-columns: repeat(2, 1fr);
            }

            .result-header {
                flex-direction: column;
                gap: 10px;
                align-items: flex-start;
            }

            .search-box {
                max-width: 100%;
            }

            th, td {
                padding: 10px 8px;
                font-size: 0.9rem;
            }
        }
    </style>
</head>
<body>
    <main class="container">
        <!-- Header -->
        <div class="header">
            <h1><i class="fas fa-exchange-alt"></i> solXEN is The Second Best</h1>
            <p>solXEN rises anew on X1 Blockchain.</p>
        </div>

        <!-- Statistics Section -->
        <div class="stats-section">
            <div class="stats-container">
                <div class="stats-grid">
                    <div class="stat-item burn">
                        <div class="stat-icon"><i class="fas fa-fire"></i></div>
                        <div class="stat-label">Total Burned (Solana)</div>
                        <div class="stat-value">{{ stats.total_burned_amount | round(precision=2) }}</div>
                    </div>
                    <div class="stat-item mint">
                        <div class="stat-icon"><i class="fas fa-coins"></i></div>
                        <div class="stat-label">Total Minted (X1)</div>
                        <div class="stat-value">{{ stats.total_minted_amount | round(precision=2) }}</div>
                    </div>
                    <div class="stat-item pending">
                        <div class="stat-icon"><i class="fas fa-clock"></i></div>
                        <div class="stat-label">Pending Mints (X1)</div>
                        <div class="stat-value">{{ stats.pending_mints }}</div>
                    </div>
                    <div class="stat-item wallet">
                        <div class="stat-icon"><i class="fas fa-wallet"></i></div>
                        <div class="stat-label">Unique Wallets</div>
                        <div class="stat-value">{{ stats.unique_wallets }}</div>
                    </div>
                </div>
            </div>
        </div>

        <!-- Wallet Summary Section -->
        <div class="results">
            <div class="result-header">
                <h2><i class="fas fa-chart-pie"></i> Wallet Summary</h2>
                <span class="result-type">{{ wallet_summaries | length }} wallets</span>
            </div>
            <div class="table-container">
                <table>
                    <thead>
                        <tr>
                            <th>Wallet Address</th>
                            <th>Total Burned (Solana)</th>
                            <th>Total Minted (X1)</th>
                            <th>Transactions</th>
                            <th>Status</th>
                        </tr>
                    </thead>
                    <tbody>
                        {% for wallet in wallet_summaries %}
                        <tr>
                            <td>
                                <a href="https://solscan.io/account/{{ wallet.wallet_address }}" 
                                   target="_blank" class="address-link">
                                    {{ wallet.wallet_address | truncate(length=12) }}
                                </a>
                            </td>
                            <td class="amount">{{ wallet.total_burned | round(precision=2) }}</td>
                            <td class="amount">{{ wallet.total_minted | round(precision=2) }}</td>
                            <td>{{ wallet.burn_count }}</td>
                            <td>
                                {% if wallet.mint_count > 0 %}
                                <span class="status-badge success">Complete</span>
                                {% else %}
                                <span class="status-badge pending">Pending</span>
                                {% endif %}
                            </td>
                        </tr>
                        {% endfor %}
                    </tbody>
                </table>
            </div>
        </div>

        <!-- Transaction Records Section -->
        <div class="results">
            <div class="result-header">
                <h2><i class="fas fa-list"></i> Transaction Records</h2>
                <span class="result-type">{{ records | length }} transactions</span>
            </div>
            
            <div class="search-container">
                <div class="search-box">
                    <input type="text" id="searchInput" placeholder="Search by address or signature..." onkeyup="searchRecords()">
                    <i class="fas fa-search"></i>
                </div>
            </div>
            
            <div class="table-container">
                <table id="recordsTable">
                    <thead>
                        <tr>
                            <th>Time</th>
                            <th>Wallet</th>
                            <th>Amount</th>
                            <th>Solana Tx</th>
                            <th>Status</th>
                            <th>X1 Tx</th>
                        </tr>
                    </thead>
                    <tbody>
                        {% for record in records %}
                        <tr>
                            <td>{{ record.timestamp }}</td>
                            <td>
                                <a href="https://solscan.io/account/{{ record.burner }}" 
                                   target="_blank" class="address-link">
                                    {{ record.burner | truncate(length=12) }}
                                </a>
                            </td>
                            <td class="amount">{{ record.amount_decimal | round(precision=2) }}</td>
                            <td>
                                <a href="https://solscan.io/tx/{{ record.signature }}" 
                                   target="_blank" class="address-link">
                                    {{ record.signature | truncate(length=12) }}
                                </a>
                            </td>
                            <td>
                                {% if record.is_minted %}
                                <span class="status-badge success">Minted</span>
                                {% else %}
                                <span class="status-badge pending">Pending</span>
                                {% endif %}
                            </td>
                            <td>
                                {% if record.minted_signature %}
                                <a href="https://explorer.x1-testnet.xen.network/tx/{{ record.minted_signature }}" 
                                   target="_blank" class="address-link">
                                    {{ record.minted_signature | truncate(length=12) }}
                                </a>
                                {% else %}
                                <span class="status-badge pending">Waiting</span>
                                {% endif %}
                            </td>
                        </tr>
                        {% endfor %}
                    </tbody>
                </table>
            </div>
        </div>
    </main>

    <footer class="footer">
        <div class="footer-content">
            <p>solXEN is The Second Best - Last updated: {{ last_updated }}</p>
            <p>Powered by Rust and X1 Testnet</p>
        </div>
    </footer>

    <script>
        function searchRecords() {
            const input = document.getElementById('searchInput');
            const filter = input.value.toUpperCase();
            const table = document.getElementById('recordsTable');
            const rows = table.getElementsByTagName('tr');

            for (let i = 1; i < rows.length; i++) {
                const cells = rows[i].getElementsByTagName('td');
                let found = false;
                
                for (let j = 0; j < cells.length; j++) {
                    if (cells[j].textContent.toUpperCase().indexOf(filter) > -1) {
                        found = true;
                        break;
                    }
                }
                
                rows[i].style.display = found ? '' : 'none';
            }
        }
    </script>
</body>
</html>"#.to_string()
    }
}