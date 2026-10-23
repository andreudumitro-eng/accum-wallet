//! ACCUM Wallet — CLI wallet for the ACCUM node

mod wallet;
mod transaction;

use clap::{Parser, Subcommand};
use colored::*;
use std::env;
use wallet::Wallet;

const DEFAULT_RPC: &str = "http://127.0.0.1:8545";

#[derive(Parser)]
#[command(name = "accum-wallet")]
#[command(version = "0.1.0")]
#[command(about = "ACCUM Wallet — CLI wallet")]
struct Cli {
    #[arg(long, global = true)]
    rpc: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new wallet (shows 12-word mnemonic)
    Create,
    /// Recover wallet from 12-word mnemonic
    Recover { phrase: String },
    /// Show wallet balance
    Balance,
    /// Show miner stats
    Info,
    /// List connected peers
    Peers,
    /// Send LYT to an address
    Send { address: String, amount: u64 },
    /// Show your receiving address
    Receive,
    /// Show transaction history
    History,
    /// List active miners
    Miners,
    /// Show block by height
    Block { height: u64 },
    /// List your UTXOs
    Utxos,
    /// Show bond status
    BondStatus,
    /// Delegate bond to a miner
    BondDelegate { miner_id: String, amount: u64 },
}

fn rpc_url(cli_rpc: &Option<String>) -> String {
    if let Some(url) = cli_rpc {
        return url.clone();
    }
    if let Ok(url) = env::var("ACCUM_RPC") {
        return url;
    }
    DEFAULT_RPC.to_string()
}

fn fetch_json(base: &str, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", base, path);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;
    let resp = client.get(&url).send()
        .map_err(|e| format!("HTTP error: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}: {}", resp.status(), url));
    }
    resp.json().map_err(|e| format!("JSON error: {}", e))
}

fn post_json(base: &str, path: &str, body: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", base, path);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;
    let resp = client.post(&url).body(body.to_string()).send()
        .map_err(|e| format!("HTTP error: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}: {}", resp.status(), url));
    }
    resp.json().map_err(|e| format!("JSON error: {}", e))
}

fn header(title: &str) {
    println!("{}", format!("=== {} ===", title).bold().cyan());
}

fn row(label: &str, value: &str) {
    println!("  {:<16} {}", label, value);
}

fn cmd_create() -> Result<(), String> {
    let path = Wallet::default_path()?;
    if path.exists() {
        println!("{} Wallet already exists at {}", "Note:".yellow(), path.display());
        return Ok(());
    }
    let mnemonic = Wallet::new_mnemonic();
    let wallet = Wallet::from_mnemonic(&mnemonic)?;
    wallet.save(&path)?;

    header("New Wallet Created");
    row("Address:", &wallet.address.green().bold().to_string());
    println!();
    println!("{}", "SAVE THIS MNEMONIC — it's the only way to recover:".yellow().bold());
    println!();
    println!("  {}", mnemonic.green().bold());
    println!();
    println!("Wallet saved to: {}", path.display());

    Ok(())
}

fn cmd_recover(phrase: &str) -> Result<(), String> {
    let path = Wallet::default_path()?;
    if path.exists() {
        println!("{} Wallet already exists at {}", "Note:".yellow(), path.display());
        return Ok(());
    }
    let wallet = Wallet::from_mnemonic(phrase)?;
    wallet.save(&path)?;

    header("Wallet Recovered");
    row("Address:", &wallet.address.green().bold().to_string());
    println!("Wallet saved to: {}", path.display());

    Ok(())
}

fn cmd_balance(base: &str) -> Result<(), String> {
    let path = Wallet::default_path()?;
    let wallet = Wallet::load_or_create(&path)?;

    header("ACCUM Wallet");
    row("Address:", &wallet.address.green().to_string());

    match fetch_json(base, &format!("/balance/{}", wallet.address)) {
        Ok(bal) => {
            let balance = bal.get("balance").and_then(|b| b.as_u64()).unwrap_or(0);
            let utxo = bal.get("utxo_count").and_then(|u| u.as_u64()).unwrap_or(0);
            row("Balance:", &format!("{} LYT", balance.to_string().green().bold()));
            row("UTXO:", &utxo.to_string());
        }
        Err(e) => {
            row("Balance:", &format!("(node offline: {})", e).yellow().to_string());
        }
    }

    Ok(())
}

fn cmd_info(base: &str) -> Result<(), String> {
    let info = fetch_json(base, "/info")?;

    header("ACCUM Miner Info");
    row("Height:", &info.get("height").and_then(|v| v.as_u64()).unwrap_or(0).to_string());
    row("Epoch:", &info.get("epoch").and_then(|v| v.as_u64()).unwrap_or(0).to_string());
    row("Peers:", &info.get("peers").and_then(|v| v.as_u64()).unwrap_or(0).to_string());
    row("Difficulty:", &format!("{:.4}", info.get("difficulty").and_then(|v| v.as_f64()).unwrap_or(0.0)));
    row("Hashrate:", &format!("{} H/s", info.get("hash_rate").and_then(|v| v.as_u64()).unwrap_or(0)));
    row("Shares found:", &info.get("shares_found").and_then(|v| v.as_u64()).unwrap_or(0).to_string());
    row("Blocks found:", &info.get("blocks_found").and_then(|v| v.as_u64()).unwrap_or(0).to_string());
    row("Bond:", &format!("{} LYT", info.get("bond").and_then(|v| v.as_u64()).unwrap_or(0).to_string().yellow()));
    row("Uptime:", info.get("uptime_formatted").and_then(|v| v.as_str()).unwrap_or("—"));

    Ok(())
}

fn cmd_peers(base: &str) -> Result<(), String> {
    let peers = fetch_json(base, "/peers")?;
    let list = peers.get("peers").and_then(|p| p.as_array()).ok_or("No peers field")?;

    header(&format!("ACCUM Peers ({})", list.len()));
    for peer in list {
        if let Some(arr) = peer.as_array() {
            let addr = arr.get(0).and_then(|a| a.as_str()).unwrap_or("?");
            let height = arr.get(1).and_then(|h| h.as_u64()).unwrap_or(0);
            println!("  {} — height {}", addr, height);
        }
    }

    Ok(())
}

fn cmd_receive() -> Result<(), String> {
    let path = Wallet::default_path()?;
    let wallet = Wallet::load_or_create(&path)?;

    header("Your Receiving Address");
    println!("  {}", wallet.address.green().bold());
    println!();
    println!("  Send LYT to this address to receive funds.");

    Ok(())
}

fn cmd_history(base: &str) -> Result<(), String> {
    let path = Wallet::default_path()?;
    let wallet = Wallet::load_or_create(&path)?;

    match fetch_json(base, &format!("/history/{}", wallet.address)) {
        Ok(h) => {
            header("Transaction History");
            if let Some(txs) = h.get("transactions").and_then(|t| t.as_array()) {
                if txs.is_empty() {
                    println!("  No transactions yet.");
                } else {
                    for tx in txs {
                        let txid = tx.get("txid").and_then(|v| v.as_str()).unwrap_or("?");
                        let short = if txid.len() > 16 { &txid[..16] } else { txid };
                        let amount = tx.get("amount").and_then(|v| v.as_i64()).unwrap_or(0);
                        let height = tx.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
                        let dir = if amount >= 0 { "IN " } else { "OUT" };
                        println!("  {} {} {} LYT at height {}", dir.cyan(), short, amount, height);
                    }
                }
            }
        }
        Err(_) => {
            header("Transaction History");
            println!("  {} Node does not expose /history yet.", "Note:".yellow());
        }
    }

    Ok(())
}

fn cmd_miners(base: &str) -> Result<(), String> {
    let miners = fetch_json(base, "/miners")?;
    let list = miners.get("miners").and_then(|m| m.as_array()).ok_or("No miners field")?;

    header(&format!("ACCUM Miners ({})", list.len()));
    for m in list {
        let id = m.get("miner_id").and_then(|v| v.as_str()).unwrap_or("?");
        let short = if id.len() > 16 { &id[..16] } else { id };
        let shares = m.get("shares").and_then(|v| v.as_u64()).unwrap_or(0);
        let bond = m.get("bond").and_then(|v| v.as_u64()).unwrap_or(0);
        let loyalty = m.get("loyalty").and_then(|v| v.as_u64()).unwrap_or(0);
        println!("  {} shares={} bond={} loyalty={}", short.cyan(), shares, bond, loyalty);
    }

    Ok(())
}

fn cmd_block(base: &str, height: u64) -> Result<(), String> {
    let block = fetch_json(base, &format!("/block/{}", height))?;

    if block.get("error").is_some() {
        return Err(format!("Block {} not found", height));
    }

    header(&format!("Block #{}", height));
    row("Hash:", block.get("hash").and_then(|v| v.as_str()).unwrap_or("?"));
    row("Prev hash:", block.get("prev_hash").and_then(|v| v.as_str()).unwrap_or("?"));
    row("Timestamp:", &block.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0).to_string());
    row("Nonce:", &block.get("nonce").and_then(|v| v.as_u64()).unwrap_or(0).to_string());
    row("Difficulty:", &format!("{:.4}", block.get("difficulty").and_then(|v| v.as_f64()).unwrap_or(0.0)));
    row("Transactions:", &block.get("transactions").and_then(|v| v.as_u64()).unwrap_or(0).to_string());

    Ok(())
}

fn cmd_utxos(base: &str) -> Result<(), String> {
    let path = Wallet::default_path()?;
    let wallet = Wallet::load_or_create(&path)?;

    let bal = fetch_json(base, &format!("/balance/{}", wallet.address))?;
    let utxo = bal.get("utxo_count").and_then(|u| u.as_u64()).unwrap_or(0);
    let balance = bal.get("balance").and_then(|b| b.as_u64()).unwrap_or(0);

    header(&format!("UTXOs for {}", wallet.address));
    row("Count:", &utxo.to_string());
    row("Total:", &format!("{} LYT", balance));

    Ok(())
}

fn cmd_send(base: &str, address: &str, amount: u64) -> Result<(), String> {
    let path = Wallet::default_path()?;
    let wallet = Wallet::load_or_create(&path)?;

    let utxos_url = format!("{}/utxos/{}", base, wallet.address);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    let resp = client.get(&utxos_url).send()
        .map_err(|e| format!("Cannot reach node: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("GET /utxos failed: {}. Node says: {}", status, text));
    }

    let json: serde_json::Value = resp.json().map_err(|e| format!("Invalid JSON: {}", e))?;
    let empty = vec![];
    let utxo_list = json.get("utxos").and_then(|v| v.as_array()).unwrap_or(&empty);
    
    if utxo_list.is_empty() {
        return Err("No UTXOs found for this address".to_string());
    }

    let mut utxos: Vec<(crate::transaction::OutPoint, crate::transaction::TxOut)> = Vec::new();
    for u in utxo_list {
        let txid_hex = u.get("txid").and_then(|v| v.as_str()).ok_or("Missing txid")?;
        let txid_bytes = hex::decode(txid_hex).map_err(|e| e.to_string())?;
        let mut txid = [0u8; 32];
        txid.copy_from_slice(&txid_bytes);

        let index = u.get("index").and_then(|v| v.as_u64()).ok_or("Missing index")? as u32;
        let value = u.get("value").and_then(|v| v.as_u64()).ok_or("Missing value")?;
        
        let script_hex = u.get("script_pubkey").and_then(|v| v.as_str()).unwrap_or("");
        let script = hex::decode(script_hex).unwrap_or_default();

        let txout = crate::transaction::TxOut {
            value,
            script_pubkey: script,
        };
        utxos.push(((txid, index), txout));
    }

    let fee = 50; 
    let tx = wallet.create_simple_tx(&utxos, address, amount, fee)?;
    
    let serialized = bincode::serialize(&tx).map_err(|e| format!("Serialize error: {}", e))?;
    let tx_hex = hex::encode(serialized);

    let url = format!("{}/transaction", base);
    let resp = client.post(&url).body(tx_hex).send()
        .map_err(|e| format!("Cannot send transaction: {}", e))?;

    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    
    if status.is_success() {
        header("Send Result");
        row("Status:", &"accepted".green().to_string());
        println!("  Response: {}", text);
        Ok(())
    } else {
        Err(format!("POST /transaction failed: {}. Node says: {}", status, text))
    }
}fn cmd_bond_status(base: &str) -> Result<(), String> {
    let path = Wallet::default_path()?;
    let wallet = Wallet::load_or_create(&path)?;

    let miners = fetch_json(base, "/miners")?;
    let list = miners.get("miners").and_then(|m| m.as_array()).ok_or("No miners field")?;

    header("Bond Status");
    row("Address:", &wallet.address.green().to_string());

    let miner_id_hex = wallet.miner_id_hex()?;

    let mut found = false;
    for m in list {
        let id = m.get("miner_id").and_then(|v| v.as_str()).unwrap_or("");
        if id == miner_id_hex {
            found = true;
            let bond = m.get("bond").and_then(|v| v.as_u64()).unwrap_or(0);
            let loyalty = m.get("loyalty").and_then(|v| v.as_u64()).unwrap_or(0);
            let shares = m.get("shares").and_then(|v| v.as_u64()).unwrap_or(0);
            let blocks = m.get("blocks_found").and_then(|v| v.as_u64()).unwrap_or(0);

            row("Miner ID:", &miner_id_hex);
            row("Bond:", &format!("{:.4} ACM", bond as f64 / 10_000_000.0));
            row("Loyalty:", &loyalty.to_string());
            row("Shares:", &shares.to_string());
            row("Blocks found:", &blocks.to_string());
            break;
        }
    }
    if !found {
        println!("  Miner not found on this node.");
    }

    Ok(())
}

fn cmd_bond_delegate(base: &str, miner_id_hex: &str, amount: u64) -> Result<(), String> {
    let path = Wallet::default_path()?;
    let wallet = Wallet::load_or_create(&path)?;

    let miner_id_bytes = hex::decode(miner_id_hex).map_err(|e| format!("Invalid miner_id hex: {}", e))?;
    if miner_id_bytes.len() != 20 {
        return Err("Miner ID must be 20 bytes (40 hex chars)".to_string());
    }
    let mut target_miner_id = [0u8; 20];
    target_miner_id.copy_from_slice(&miner_id_bytes);

    let utxos_url = format!("{}/utxos/{}", base, wallet.address);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    let resp = client.get(&utxos_url).send()
        .map_err(|e| format!("Cannot reach node: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("GET /utxos failed: {}. Node says: {}", status, text));
    }

    let json: serde_json::Value = resp.json().map_err(|e| format!("Invalid JSON: {}", e))?;
    let empty = vec![];
    let utxo_list = json.get("utxos").and_then(|v| v.as_array()).unwrap_or(&empty);
    
    if utxo_list.is_empty() {
        return Err("No UTXOs found".to_string());
    }

    let mut utxos: Vec<(crate::transaction::OutPoint, crate::transaction::TxOut)> = Vec::new();
    for u in utxo_list {
        let txid_hex = u.get("txid").and_then(|v| v.as_str()).ok_or("Missing txid")?;
        let txid_bytes = hex::decode(txid_hex).map_err(|e| e.to_string())?;
        let mut txid = [0u8; 32];
        txid.copy_from_slice(&txid_bytes);

        let index = u.get("index").and_then(|v| v.as_u64()).ok_or("Missing index")? as u32;
        let value = u.get("value").and_then(|v| v.as_u64()).ok_or("Missing value")?;
        
        let script_hex = u.get("script_pubkey").and_then(|v| v.as_str()).unwrap_or("");
        let script = hex::decode(script_hex).unwrap_or_default();

        let txout = crate::transaction::TxOut { value, script_pubkey: script };
        utxos.push(((txid, index), txout));
    }

    let fee = 50; 
    let tx = wallet.create_bond_tx(&utxos, &target_miner_id, amount, fee)?;
    
    let serialized = bincode::serialize(&tx).map_err(|e| format!("Serialize error: {}", e))?;
    let tx_hex = hex::encode(serialized);

    let url = format!("{}/transaction", base);
    let resp = client.post(&url).body(tx_hex).send()
        .map_err(|e| format!("Cannot send transaction: {}", e))?;

    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    
    if status.is_success() {
        header("Bond Delegate Result");
        row("Status:", &"accepted".green().to_string());
        println!("  Response: {}", text);
        Ok(())
    } else {
        Err(format!("POST /transaction failed: {}. Node says: {}", status, text))
    }
}

fn main() {
    let cli = Cli::parse();
    let base = rpc_url(&cli.rpc);

    let result = match cli.command {
        Commands::Create => cmd_create(),
        Commands::Recover { phrase } => cmd_recover(&phrase),
        Commands::Balance => cmd_balance(&base),
        Commands::Info => cmd_info(&base),
        Commands::Peers => cmd_peers(&base),
        Commands::Receive => cmd_receive(),
        Commands::History => cmd_history(&base),
        Commands::Miners => cmd_miners(&base),
        Commands::Block { height } => cmd_block(&base, height),
        Commands::Utxos => cmd_utxos(&base),
        Commands::BondStatus => cmd_bond_status(&base),
        Commands::BondDelegate { miner_id, amount } => cmd_bond_delegate(&base, &miner_id, amount),
        Commands::Send { address, amount } => cmd_send(&base, &address, amount),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}








