use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;

/// One pool account record written by the scanner, read by the MEV engine.
#[derive(Serialize, Deserialize, Debug)]
pub struct PoolRecord {
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub data: Vec<u8>,
}

/// The full output file written to OUTPUT_PATH.
/// MEV engine deserializes this on startup.
#[derive(Serialize, Deserialize, Debug)]
pub struct ScanOutput {
    pub snapshot_slot: u64,
    pub records: Vec<PoolRecord>,
}

/// Canonical output path. MEV engine reads from here.
pub const OUTPUT_PATH: &str = "/mnt/mev/pool_snapshot.bin";
