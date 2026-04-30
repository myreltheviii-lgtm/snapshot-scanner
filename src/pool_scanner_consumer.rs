// pool_scanner_consumer.rs — drop this file into your MEV engine crate.
//
// Call load_from_snapshot() once at engine startup, AFTER snapshot-scanner
// binary has run and written its output to OUTPUT_PATH.
//
// The PoolRecord / ScanOutput types below must match the definitions in
// snapshot_scanner/src/output.rs exactly — same field order, same types.
// bincode is not self-describing; layout drift causes silent deserialization
// garbage or an outright Err. If you change output.rs, change this file too.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;
use std::fs;

/// Canonical path written by snapshot-scanner, read here.
/// Must match output::OUTPUT_PATH in the scanner binary.
const OUTPUT_PATH: &str = "/mnt/mev/pool_snapshot.bin";

/// Mirror of snapshot_scanner::output::PoolRecord.
/// Field order must be identical — bincode encodes by position.
#[derive(Serialize, Deserialize)]
struct PoolRecord {
    pubkey: Pubkey,
    owner: Pubkey,
    lamports: u64,
    data: Vec<u8>,
}

/// Mirror of snapshot_scanner::output::ScanOutput.
#[derive(Serialize, Deserialize)]
struct ScanOutput {
    snapshot_slot: u64,
    records: Vec<PoolRecord>,
}

/// Public return type handed to the MEV engine's pool registration path.
pub struct SnapshotPoolSeed {
    /// Slot of the snapshot this data came from.
    /// Log this at startup so you know how stale the seed is.
    pub snapshot_slot: u64,
    /// All DEX-owned pool accounts found in the snapshot.
    /// Tuple: (pubkey, owner, lamports, raw_account_data)
    pub accounts: Vec<(Pubkey, Pubkey, u64, Vec<u8>)>,
}

/// Read and deserialize the scanner output.
///
/// Returns Err if the file doesn't exist (scanner hasn't run yet),
/// is truncated (scanner crashed mid-write — impossible with atomic rename),
/// or the format has drifted (output.rs changed without updating this file).
pub fn load_from_snapshot() -> Result<SnapshotPoolSeed> {
    let bytes = fs::read(OUTPUT_PATH).with_context(|| {
        format!(
            "snapshot scanner output not found at {OUTPUT_PATH} — \
             run snapshot-scanner binary before starting the MEV engine"
        )
    })?;

    let output: ScanOutput = bincode::deserialize(&bytes).with_context(|| {
        format!(
            "failed to deserialize scanner output at {OUTPUT_PATH} — \
             file may be corrupt or PoolRecord/ScanOutput layout has drifted \
             between scanner and consumer"
        )
    })?;

    if output.records.is_empty() {
        // Not fatal — engine starts with empty pool state and fills via Geyser.
        // But this warrants investigation: scanner ran but found nothing.
        eprintln!(
            "[pool_scanner_consumer] WARN: snapshot at slot {} contained \
             zero DEX pool accounts. Check scanner DEX owner list.",
            output.snapshot_slot
        );
    }

    let accounts = output
        .records
        .into_iter()
        .map(|r| (r.pubkey, r.owner, r.lamports, r.data))
        .collect();

    Ok(SnapshotPoolSeed {
        snapshot_slot: output.snapshot_slot,
        accounts,
    })
}
