mod dex_owners;
mod output;

use anyhow::{Context, Result};
use output::{OUTPUT_PATH, PoolRecord, ScanOutput};
use rayon::prelude::*;
use agave_snapshots::paths::get_full_snapshot_archives;
use solana_accounts_db::{
    accounts_db::AccountsDbConfig,
    accounts_file::StorageAccess,
};
use solana_runtime::snapshot_utils;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};

fn main() -> Result<()> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let snapshot_dir = PathBuf::from(
        args.get(1)
            .map(|s| s.as_str())
            .unwrap_or("/mnt/ledger/snapshot"),
    );
    let account_paths: Vec<PathBuf> = args
        .get(2)
        .map(|s| s.split(',').map(PathBuf::from).collect())
        .unwrap_or_else(|| vec![PathBuf::from("/mnt/ledger/accounts")]);

    eprintln!("[scanner] snapshot dir: {}", snapshot_dir.display());
    eprintln!("[scanner] account paths: {:?}", account_paths);

    let t0 = Instant::now();

    // -------------------------------------------------------------------------
    // Step 1: Find the highest full snapshot archive
    //
    // get_full_snapshot_archives() returns all full snapshot archives found in
    // snapshot_dir in unspecified order. We sort ascending by slot and take the
    // last element (highest slot = most recent). This is equivalent to the
    // removed get_highest_full_snapshot_archive_info() convenience wrapper,
    // written out explicitly to avoid the ambiguous import path.
    // -------------------------------------------------------------------------
    let mut archives = get_full_snapshot_archives(&snapshot_dir);
    archives.sort_unstable();
    let archive_info = archives
        .into_iter()
        .next_back()
        .context("no full snapshot archive found in snapshot dir")?;

    let snapshot_slot = archive_info.slot();
    eprintln!(
        "[scanner] found snapshot at slot {} ({:.1}s)",
        snapshot_slot,
        t0.elapsed().as_secs_f64()
    );

    // -------------------------------------------------------------------------
    // Step 2: Unarchive snapshot via the public API
    //
    // verify_and_unarchive_snapshots() is the correct public entry point.
    // It creates and manages its own TempDir internally — we must NOT try to
    // pass or pre-create an external path. The returned UnarchivedSnapshotsGuard
    // holds those TempDirs and MUST remain in scope for the entire scan.
    //
    // Dropping _guard before the scan completes would trigger remove_dir_all on
    // the unpack directory. With StorageAccess::File, the AppendVec file handles
    // remain valid on Linux after unlink (open fds survive), but we keep the
    // guard alive anyway for explicit, portable correctness.
    //
    // AccountsDbConfig: only storage_access matters here. All other fields
    // default to the same values a standard validator startup would use.
    // StorageAccess::File means AppendVec data is accessed via read() syscalls
    // rather than mmap. For a one-shot scan this is correct — we do not need
    // the entire snapshot mapped into virtual address space, and mmap on a
    // 50-80 GB snapshot adds TLB and page-fault pressure for no benefit.
    // -------------------------------------------------------------------------
    eprintln!("[scanner] unpacking archive...");

    let accounts_db_config = AccountsDbConfig {
        storage_access: StorageAccess::File,
        ..AccountsDbConfig::default()
    };

    let (unarchived, _guard) = snapshot_utils::verify_and_unarchive_snapshots(
        &snapshot_dir,  // bank_snapshots_dir: parent for the internal TempDir
        &archive_info,
        None,           // no incremental snapshot
        &account_paths,
        &accounts_db_config,
    )
    .context("failed to unarchive snapshot")?;

    eprintln!(
        "[scanner] unpacked ({:.1}s)",
        t0.elapsed().as_secs_f64()
    );

    // -------------------------------------------------------------------------
    // Step 3: Get AccountStorageMap
    //
    // full_storage is DashMap<Slot, Arc<AccountStorageEntry>>. A full snapshot
    // contains accounts from many historical slots — the same pubkey may appear
    // in multiple slots with different data (old vs new version of a pool account).
    //
    // We collect ALL versions intentionally. The MEV engine's pool registration
    // is last-write-wins (Geyser updates overwrite on every processed slot), so
    // a slightly stale pool account at startup is corrected within one slot of
    // the engine going live. The stale version is never acted on — it is only
    // used to pre-populate the pool graph before live data arrives.
    // -------------------------------------------------------------------------
    let storage_map = &unarchived.full_storage;

    eprintln!(
        "[scanner] {} storage entries to scan ({:.1}s)",
        storage_map.len(),
        t0.elapsed().as_secs_f64()
    );

    // -------------------------------------------------------------------------
    // Step 4: Build owner filter — hardcoded DEX program IDs.
    // dex_owners() hard-panics at startup on any malformed ID string.
    // -------------------------------------------------------------------------
    let dex_owners = dex_owners::dex_owners();
    eprintln!("[scanner] filtering for {} DEX owners", dex_owners.len());

    // -------------------------------------------------------------------------
    // Step 5: Parallel scan across all AppendVec files
    //
    // Two-phase per AppendVec:
    //   Phase 1 — scan_accounts_without_data(): reads 136-byte headers only.
    //             Checks owner against dex_owners set. O(1) HashSet lookup.
    //             For non-DEX accounts (the vast majority) this is all we do.
    //   Phase 2 — get_stored_account_callback(): reads full account data.
    //             Called only for DEX-owned accounts. Hit rate is very sparse
    //             (~0.01% of all accounts are DEX pool accounts).
    //
    // Thread model: each rayon worker accumulates hits into a thread-local
    // `local` vec (no lock contention during scan). The shared Mutex is only
    // acquired once per AppendVec that had any DEX hits — not once per account.
    //
    // FORK PATCH REQUIRED: scan_accounts_without_data() and
    // get_stored_account_callback() must be forwarded as pub through AccountsFile.
    // AccountStorageEntry.accounts is already pub. See FORK_PATCHES.md.
    // -------------------------------------------------------------------------
    let records: Mutex<Vec<PoolRecord>> = Mutex::new(Vec::with_capacity(200_000));

    storage_map
        .iter()
        .par_bridge()
        .for_each(|entry| {
            let storage = entry.value();

            // Capacity 8: DEX pool accounts are sparse. Most AppendVecs have
            // zero DEX hits. Starting at 8 avoids realloc for the common case
            // of 1-4 hits without wasting memory on the zero-hit majority.
            let mut local: Vec<PoolRecord> = Vec::with_capacity(8);

            // Phase 1: header-only scan — no data allocation on non-DEX accounts.
            // scan_accounts_without_data uses a stack-allocated 16KB BufferedReader;
            // no heap allocation occurs for the non-DEX majority of accounts.
            let result = storage
                .accounts
                .scan_accounts_without_data(|offset, account| {
                    if dex_owners.contains(account.owner()) {
                        // Phase 2: full data read — only for DEX-owned accounts.
                        // The offset returned by scan_accounts_without_data is
                        // stable (snapshot AppendVecs are read-only), so this
                        // lookup is always valid for the life of the scan.
                        storage.accounts.get_stored_account_callback(
                            offset,
                            |full| {
                                local.push(PoolRecord {
                                    pubkey: *full.pubkey(),
                                    owner: *full.owner(),
                                    lamports: full.lamports(),
                                    // to_vec() is the unavoidable allocation here:
                                    // the callback borrow ends when this closure
                                    // returns, so we must own the data.
                                    data: full.data().to_vec(),
                                });
                            },
                        );
                    }
                });

            if let Err(e) = result {
                // Non-fatal: a single corrupt AppendVec should not abort the
                // entire scan. Log, skip, and continue with remaining files.
                eprintln!(
                    "[scanner] WARN: scan failed for slot {}: {}",
                    entry.key(),
                    e
                );
                return;
            }

            if !local.is_empty() {
                // Lock held only for extend(local) — duration proportional to
                // DEX hit count in this AppendVec, which is tiny (0-10 accounts).
                // No other locks acquired while holding this. Deadlock impossible.
                // Mutex poisoning impossible — we never panic while holding it.
                records.lock().unwrap().extend(local);
            }
        });

    let records = records.into_inner().unwrap();

    if records.is_empty() {
        // Not a panic — engine can still start and fill via Geyser.
        // But this is always worth investigating: either the DEX owner list is
        // wrong, or the snapshot path is pointing at the wrong directory.
        eprintln!(
            "[scanner] WARN: zero DEX pool accounts found. \
             Verify DEX owner IDs in dex_owners.rs and snapshot path."
        );
    }

    eprintln!(
        "[scanner] found {} DEX pool accounts ({:.1}s)",
        records.len(),
        t0.elapsed().as_secs_f64()
    );

    // -------------------------------------------------------------------------
    // Step 6: Serialize and write output atomically
    //
    // Write to OUTPUT_PATH.tmp first, then rename into OUTPUT_PATH.
    // On Linux, rename(2) is atomic when src and dst are on the same filesystem.
    // Since both paths share the /mnt/mev/ prefix they are on the same fs.
    // The MEV engine therefore never reads a partial output file — it either
    // sees the previous complete file or the new complete file, never a torn one.
    // -------------------------------------------------------------------------
    let output = ScanOutput {
        snapshot_slot,
        records,
    };

    if let Some(parent) = Path::new(OUTPUT_PATH).parent() {
        fs::create_dir_all(parent)
            .context("failed to create output dir")?;
    }

    let tmp_path = format!("{OUTPUT_PATH}.tmp");
    let bytes = bincode::serialize(&output)
        .context("failed to serialize output")?;

    fs::write(&tmp_path, &bytes)
        .context("failed to write temp output file")?;

    fs::rename(&tmp_path, OUTPUT_PATH)
        .context("failed to rename temp output into place")?;

    eprintln!(
        "[scanner] wrote {} bytes to {} ({:.1}s total)",
        bytes.len(),
        OUTPUT_PATH,
        t0.elapsed().as_secs_f64()
    );

    // _guard drops here → unpack TempDir removed cleanly.
    eprintln!("[scanner] done.");
    Ok(())
}
