use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Hash32 = [u8; 32];
pub type OutPoint = (Hash32, u32);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxOut {
    pub value: u64,
    pub script_pubkey: Vec<u8>,
}

impl TxOut {
    /// Creates a P2PKH output for a Base58 address.
    pub fn create_bond(miner_id: &[u8; 20]) -> Result<Self, String> {
        let mut script = Vec::new();
        script.push(0xBA); // OP_BOND
        script.extend_from_slice(miner_id);
        Ok(Self { value: 0, script_pubkey: script })
    }

    pub fn extract_miner_id(&self) -> Option<[u8; 20]> {
        if self.script_pubkey.len() == 21 && self.script_pubkey[0] == 0xBA {
            let mut id = [0u8; 20];
            id.copy_from_slice(&self.script_pubkey[1..21]);
            Some(id)
        } else {
            None
        }
    }


    pub fn create_p2pkh(address: &str) -> Result<Self, String> {
        let decoded = bs58::decode(address).into_vec()
            .map_err(|e| format!("Invalid address: {}", e))?;
        if decoded.len() != 25 || decoded[0] != 0x00 {
            return Err("Invalid address format".to_string());
        }
        Ok(Self {
            value: 0,
            script_pubkey: decoded,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxIn {
    pub prev_txid: Hash32,
    pub prev_index: u32,
    pub script_sig: Vec<u8>,
    pub sequence: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    pub version: u32,
    pub inputs: Vec<TxIn>,
    pub outputs: Vec<TxOut>,
    pub locktime: u32,
}

impl Transaction {
    /// Double SHA-256 for txid (same as node).
    pub fn txid(&self) -> Hash32 {
        let mut hasher = Sha256::new();
        hasher.update(&self.version.to_le_bytes());
        hasher.update(&(self.inputs.len() as u64).to_le_bytes());
        for inp in &self.inputs {
            hasher.update(&inp.prev_txid);
            hasher.update(&inp.prev_index.to_le_bytes());
            hasher.update(&(inp.script_sig.len() as u64).to_le_bytes());
            hasher.update(&inp.script_sig);
            hasher.update(&inp.sequence.to_le_bytes());
        }
        hasher.update(&(self.outputs.len() as u64).to_le_bytes());
        for out in &self.outputs {
            hasher.update(&out.value.to_le_bytes());
            hasher.update(&(out.script_pubkey.len() as u64).to_le_bytes());
            hasher.update(&out.script_pubkey);
        }
        hasher.update(&self.locktime.to_le_bytes());
        
        let first_hash = hasher.finalize();
        let mut hasher2 = Sha256::new();
        hasher2.update(&first_hash);
        let digest = hasher2.finalize();
        
        let mut result = [0u8; 32];
        result.copy_from_slice(&digest);
        result
    }

    /// Computes sighash for signing a specific input.
    pub fn sighash(&self, input_index: usize) -> Hash32 {
        if input_index >= self.inputs.len() {
            return [0u8; 32];
        }

        let mut data = Vec::new();

        data.extend_from_slice(&self.version.to_le_bytes());
        data.extend_from_slice(&(self.inputs.len() as u32).to_le_bytes());

        for input in &self.inputs {
            data.extend_from_slice(&input.prev_txid);
            data.extend_from_slice(&input.prev_index.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes()); // Вместо script_sig
            data.extend_from_slice(&input.sequence.to_le_bytes());
        }

        data.extend_from_slice(&(self.outputs.len() as u32).to_le_bytes());
        for output in &self.outputs {
            data.extend_from_slice(&output.value.to_le_bytes());
            data.extend_from_slice(&(output.script_pubkey.len() as u32).to_le_bytes());
            data.extend_from_slice(&output.script_pubkey);
        }

        data.extend_from_slice(&self.locktime.to_le_bytes());
        data.extend_from_slice(&0x01u32.to_le_bytes());

        let first_hash = Sha256::digest(&data);
        let digest = Sha256::digest(&first_hash);
        let mut result = [0u8; 32];
        result.copy_from_slice(&digest);
        result
    }
}



