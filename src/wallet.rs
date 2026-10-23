//! Wallet — key management and transaction signing for ACCUM CLI wallet
//!
//! Uses the SAME address derivation as accum-node:
//!   pubkey = secp256k1(secret_key)         — compressed, 33 bytes
//!   miner_id = RIPEMD160(SHA256(pubkey))   — 20 bytes
//!   address = Base58(0x00 || miner_id || checksum)
//!   checksum = SHA256(SHA256(0x00 || miner_id))[0..4]

use bip39::{Language, Mnemonic};
use rand::RngCore;
use ripemd::Ripemd160;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

use crate::transaction::{OutPoint, Transaction, TxIn, TxOut};

const ACCUM_DIR: &str = ".accum";
const WALLET_FILE: &str = "wallet.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct Wallet {
    /// Hex-encoded private key (32 bytes)
    pub private_key: String,
    /// ACCUM address (Base58) — derived, not stored in wallet.json
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub address: String,
}

impl Wallet {
    pub fn default_path() -> Result<PathBuf, String> {
        let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
        let dir = home.join(ACCUM_DIR);
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .map_err(|e| format!("Cannot create {}: {}", dir.display(), e))?;
        }
        Ok(dir.join(WALLET_FILE))
    }

    pub fn load_or_create(path: &PathBuf) -> Result<Wallet, String> {
        if path.exists() {
            let data = fs::read_to_string(path)
                .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;

            #[derive(Deserialize)]
            struct FileOnly {
                private_key: String,
            }

            let file: FileOnly = serde_json::from_str(&data)
                .map_err(|e| format!("Cannot parse {}: {}", path.display(), e))?;

            let key_bytes = hex::decode(&file.private_key)
                .map_err(|e| format!("Invalid private_key hex: {}", e))?;
            if key_bytes.len() != 32 {
                return Err(format!("private_key must be 32 bytes, got {}", key_bytes.len()));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&key_bytes);
            let address = derive_address(&arr)?;

            Ok(Wallet {
                private_key: file.private_key,
                address,
            })
        } else {
            let wallet = Wallet::generate()?;
            wallet.save(path)?;
            println!("Created new wallet: {}", path.display());
            Ok(wallet)
        }
    }

    pub fn generate() -> Result<Wallet, String> {
        let mut rng = rand::thread_rng();
        let mut key_bytes = [0u8; 32];
        rng.fill_bytes(&mut key_bytes);
        let private_key = hex::encode(key_bytes);
        let address = derive_address(&key_bytes)?;
        Ok(Wallet { private_key, address })
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        let data = serde_json::json!({
            "private_key": self.private_key,
        });
        let s = serde_json::to_string_pretty(&data)
            .map_err(|e| format!("Cannot serialize wallet: {}", e))?;
        fs::write(path, s)
            .map_err(|e| format!("Cannot write {}: {}", path.display(), e))?;
        Ok(())
    }

    pub fn from_mnemonic(phrase: &str) -> Result<Wallet, String> {
        let mnemonic = Mnemonic::parse_in(Language::English, phrase)
            .map_err(|e| format!("Invalid mnemonic: {}", e))?;
        let seed = mnemonic.to_seed("");
        let mut hasher = Sha256::new();
        hasher.update(&seed);
        let hash = hasher.finalize();
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&hash);
        let private_key = hex::encode(key_bytes);
        let address = derive_address(&key_bytes)?;
        Ok(Wallet { private_key, address })
    }

    pub fn new_mnemonic() -> String {
        let mut rng = rand::thread_rng();
        let mut entropy = [0u8; 16];
        rng.fill_bytes(&mut entropy);
        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
            .expect("valid entropy");
        mnemonic.to_string()
    }

    pub fn get_secret_key(&self) -> Result<SecretKey, String> {
        let bytes = hex::decode(&self.private_key).map_err(|e| e.to_string())?;
        if bytes.len() != 32 {
            return Err("Invalid private key length".to_string());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        SecretKey::from_slice(&arr).map_err(|e| e.to_string())
    }

    // --- SIGNING ---
    pub fn sign(&self, message: &[u8; 32]) -> Result<Vec<u8>, String> {
        let secret = self.get_secret_key()?;
        let secp = Secp256k1::new();
        let msg = Message::from_digest_slice(message).map_err(|e| e.to_string())?;
        let sig = secp.sign_ecdsa(&msg, &secret);
        Ok(sig.serialize_compact().to_vec())
    }

    // --- CREATE TRANSACTION (Select UTXO + Sign) ---
    pub fn create_simple_tx(
        &self,
        utxos: &[(OutPoint, TxOut)],
        to_address: &str,
        amount: u64,
        fee: u64,
    ) -> Result<Transaction, String> {
        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        let mut selected = Vec::new();
        let mut total_in = 0u64;

        // 1. Select UTXOs
        for (outpoint, output) in utxos {
            selected.push((outpoint.clone(), output.clone()));
            total_in += output.value;
            if total_in >= amount + fee {
                break;
            }
        }

        if total_in < amount + fee {
            return Err(format!(
                "Insufficient funds. Need {} LYT, have {} LYT",
                amount + fee,
                total_in
            ));
        }

        let change = total_in - amount - fee;

        // 2. Build inputs
        let mut inputs = Vec::new();
        for (outpoint, _) in &selected {
            inputs.push(TxIn {
                prev_txid: outpoint.0,
                prev_index: outpoint.1,
                script_sig: vec![], // Empty for now, filled later
                sequence: 0xFFFFFFFF,
            });
        }

        // 3. Build outputs
        let mut outputs = Vec::new();
        
        let mut recipient = TxOut::create_p2pkh(to_address)?;
        recipient.value = amount;
        outputs.push(recipient);

        // Change (if remainder is greater than dust limit)
        if change > 100 {
            let mut change_out = TxOut::create_p2pkh(&self.address)?;
            change_out.value = change;
            outputs.push(change_out);
        }

        // 4. Assemble transaction
        let mut tx = Transaction {
            version: 1,
            inputs,
            outputs,
            locktime: 0,
        };

        // 5. Sign each input
        for i in 0..tx.inputs.len() {
            let sighash = tx.sighash(i); println!("WALLET sighash: {}", hex::encode(&sighash));
            
            let sig = self.sign(&sighash)?;

            // Format: [sig_len][sig][pubkey_len][pubkey]
            let mut script_sig = Vec::new();
            script_sig.push(sig.len() as u8);
            script_sig.extend_from_slice(&sig);

            let secret = self.get_secret_key()?;
            let secp = Secp256k1::new();
            let pubkey = PublicKey::from_secret_key(&secp, &secret);
            let pubkey_bytes = pubkey.serialize();
            
            
            
            
            script_sig.push(pubkey_bytes.len() as u8);
            script_sig.extend_from_slice(&pubkey_bytes);

            tx.inputs[i].script_sig = script_sig;
        }

        Ok(tx)
    }
    pub fn miner_id_hex(&self) -> Result<String, String> {
        let secret = self.get_secret_key()?;
        let secp = Secp256k1::new();
        let pubkey = PublicKey::from_secret_key(&secp, &secret);
        let compressed = pubkey.serialize();
        let sha = Sha256::digest(&compressed);
        let rip = Ripemd160::digest(&sha);
        Ok(hex::encode(&rip))
    }

    pub fn get_public_key_bytes(&self) -> Result<Vec<u8>, String> {
        let secret = self.get_secret_key()?;
        let secp = Secp256k1::new();
        let public = PublicKey::from_secret_key(&secp, &secret);
        Ok(public.serialize().to_vec())
    }

    pub fn create_bond_tx(
        &self,
        utxos: &[(OutPoint, TxOut)],
        target_miner_id: &[u8; 20],
        amount: u64,
        fee: u64,
    ) -> Result<Transaction, String> {
        if amount < 10_000_000 {
            return Err("Bond amount must be >= 10,000,000 LYT (1 ACM)".to_string());
        }

        let mut selected = Vec::new();
        let mut total_in = 0u64;

        for (outpoint, output) in utxos {
            selected.push((outpoint.clone(), output.clone()));
            total_in += output.value;
            if total_in >= amount + fee {
                break;
            }
        }

        if total_in < amount + fee {
            return Err(format!("Insufficient funds. Need {} LYT, have {} LYT", amount + fee, total_in));
        }

        let change = total_in - amount - fee;

        let mut inputs = Vec::new();
        for (outpoint, _) in &selected {
            inputs.push(TxIn {
                prev_txid: outpoint.0,
                prev_index: outpoint.1,
                script_sig: vec![],
                sequence: 0xFFFFFFFF,
            });
        }

        let mut outputs = Vec::new();
        let mut bond_out = TxOut::create_bond(target_miner_id)?;
        bond_out.value = amount;
        outputs.push(bond_out);

        if change > 100 {
            let mut change_out = TxOut::create_p2pkh(&self.address)?;
            change_out.value = change;
            outputs.push(change_out);
        }

        let mut tx = Transaction {
            version: 1,
            inputs,
            outputs,
            locktime: 0,
        };

        for i in 0..tx.inputs.len() {
            let sighash = tx.sighash(i);
            let sig = self.sign(&sighash)?;
            let pubkey_bytes = self.get_public_key_bytes()?;

            let mut script_sig = Vec::new();
            script_sig.push(sig.len() as u8);
            script_sig.extend_from_slice(&sig);
            script_sig.push(pubkey_bytes.len() as u8);
            script_sig.extend_from_slice(&pubkey_bytes);
            
            tx.inputs[i].script_sig = script_sig;
        }

        Ok(tx)
    }
}

fn derive_address(key_bytes: &[u8; 32]) -> Result<String, String> {
    let secret = SecretKey::from_slice(key_bytes)
        .map_err(|e| format!("Invalid secret key: {}", e))?;

    let secp = Secp256k1::new();
    let public = PublicKey::from_secret_key(&secp, &secret);
    let compressed = public.serialize();

    let sha = Sha256::digest(&compressed);
    let rip = Ripemd160::digest(&sha);
    let mut miner_id = [0u8; 20];
    miner_id.copy_from_slice(&rip);

    let mut with_version = vec![0x00];
    with_version.extend_from_slice(&miner_id);

    let checksum_full = Sha256::digest(&Sha256::digest(&with_version));
    let checksum = &checksum_full[0..4];
    with_version.extend_from_slice(checksum);

    Ok(bs58::encode(with_version).into_string())
}







