# snapshot-scanner

> ⚠️ **UNDER ACTIVE DEVELOPMENT** — This project is not production-ready.
> Architecture is complete. Compilation and end-to-end testing against live
> validator snapshots is pending dedicated hardware. Do not use in production.

---

A production-grade Rust binary demonstrating deep Solana infrastructure
knowledge — parallel account scanning across a full validator account database,
surgical Agave fork patches, and atomic file I/O.

Built as part of a broader Solana development portfolio alongside
[AUDDShield](https://github.com/myreltheviii-lgtm/AuddShieldV1) — a trustless
mutual insurance protocol on Solana.

---

## What It Does

Reads a full Agave validator snapshot directly from disk, scans every account
in parallel, filters for accounts owned by specific on-chain programs, and
writes the results atomically to a binary output file.

The scanner operates at the validator storage layer — below RPC, below Geyser,
directly against the raw `AppendVec` files that make up Solana's account
database. This gives it access to the complete account state at a given slot
without any network dependency.

---

## Architecture

### Two-Phase Account Scan

A full Solana mainnet snapshot contains hundreds of millions of accounts.
Scanning all of them naively is prohibitively slow.

This scanner uses a two-phase strategy:

**Phase 1 — Header-only scan (`scan_accounts_without_data`)**
Reads only the 136-byte account header for every account. Checks the `owner`
field against a HashSet of target program IDs. For non-target accounts
(>99.99% of all accounts) this is all that happens — no heap allocation,
no data read. Uses a stack-allocated 16KB BufferedReader internally.

**Phase 2 — Full data read (`get_stored_account_callback`)**
Called only for target-owned accounts (~0.01% hit rate). Reads the full
account data. Pushes a `PoolRecord` into a thread-local buffer.

This two-phase design means the scanner reads approximately 0.01% of the total
snapshot data volume to extract 100% of the target accounts.

### Parallel Execution

AppendVec files (Solana's account storage format) are scanned in parallel via
`rayon::par_bridge`. Each rayon worker accumulates hits into a thread-local
`Vec<PoolRecord>` — no lock contention during the scan itself. The shared
`Mutex<Vec<PoolRecord>>` is only locked once per AppendVec that had any hits,
for the duration of a single `extend()` call.

Lock held = duration of extend(local) only. No other locks acquired while
holding this. Deadlock impossible. Mutex poisoning impossible — we never
panic while holding it.

### Atomic Output

Output is written to `OUTPUT_PATH.tmp` first, then renamed into `OUTPUT_PATH`.
On Linux, `rename(2)` is atomic when source and destination are on the same
filesystem. A reader therefore never sees a partial output file — it either
sees the previous complete file or the new complete file. Never torn.

### Agave Fork Patches

The scanner accesses Agave internals that are `pub(crate)` in the upstream
repository. Four minimal visibility changes are required — documented in full
in `FORK_PATCHES.md`. No logic changes. No validator startup path changes.
No validator behavior changes. Visibility only.

Maximum 4 changes. Minimum 2. Surgical.

---

## Supported Programs

The scanner filters accounts by owner program ID. The current target set covers
8 on-chain programs across major Solana DEX protocols:

| Protocol | Program ID |
|---|---|
| Raydium V4 (AMM) | `675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8` |
| Raydium CPMM | `CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C` |
| Raydium CLMM | `CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK` |
| Orca Whirlpool | `whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc` |
| Meteora DLMM | `LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo` |
| Meteora DAMM V1 | `Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB` |
| Meteora DAMM V2 | `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG` |
| PumpSwap | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` |

The target program list is hardcoded in `src/dex_owners.rs` with a startup
assertion that panics if the count does not match `EXPECTED_COUNT`. This
ensures silent removals or duplicate IDs are caught at binary startup rather
than producing wrong output silently.

---

## Files

| File | Description |
|---|---|
| `src/main.rs` | Entry point — snapshot unpacking, parallel scan, atomic output |
| `src/dex_owners.rs` | Target program ID registry with startup assertion |
| `src/output.rs` | Serialization types and output path constant |
| `pool_scanner_consumer.rs` | Consumer module for reading the output file |
| `FORK_PATCHES.md` | Exact visibility changes needed in the Agave fork |
| `Cargo.toml` | Crate manifest with Agave path dependencies |

---

## Build Requirements

> ⚠️ Building requires a forked Agave workspace. See `FORK_PATCHES.md`.

- Agave validator fork with patches from `FORK_PATCHES.md` applied
- Rust 1.75+
- NVMe storage (snapshot I/O is the primary bottleneck)
- 128GB+ RAM recommended for full mainnet snapshots (50–80GB)

```bash
# From the Agave fork workspace root
cargo build --release -p snapshot-scanner
```

---

## Usage

```bash
./target/release/snapshot-scanner \
    /mnt/ledger/snapshot \
    /mnt/ledger/accounts
```

Output written to `/mnt/mev/pool_snapshot.bin`.

Read the output:
```rust
let seed = pool_scanner_consumer::load_from_snapshot()?;
// seed.accounts: Vec<(pubkey, owner, lamports, raw_account_data)>
// seed.snapshot_slot: u64 — how stale the data is
```

---

## Development Status

| Component | Status |
|---|---|
| Architecture design | ✅ Complete |
| Agave fork patches documented | ✅ Complete |
| Core scan logic | ✅ Written |
| Serialization / output | ✅ Written |
| Consumer module | ✅ Written |
| Compilation against Agave fork | 🔧 Pending hardware |
| End-to-end test on real snapshot | 🔧 Pending hardware |
| Production validation | 🔧 Pending |

Full validation requires dedicated validator hardware with a live mainnet
snapshot. This is the only remaining step before the binary is production-ready.
