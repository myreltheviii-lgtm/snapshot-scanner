# snapshot-scanner

A standalone Rust binary that seeds a Solana MEV engine's pool graph at startup
by scanning full Agave validator snapshots directly — across 8 DEX protocols,
in parallel, with sub-second latency on modern NVMe hardware.

---

## Status

**Architecture complete. Not yet compiled or tested against live snapshots.**

Building requires a forked Agave workspace with the visibility patches documented
in `FORK_PATCHES.md`. Full validator hardware (NVMe, 128GB+ RAM) is needed to
test against real snapshots (50–80GB). This will be compiled and validated once
dedicated validator hardware is available.

---

## What It Does

A Solana MEV engine that relies on Geyser for pool discovery has a cold-start
problem: on startup it knows nothing about existing pools and must wait for live
account updates to stream in before it can route arbitrage. Depending on pool
count and Geyser throughput, this blind window can last seconds to minutes.

This scanner eliminates that window. It runs once before the MEV engine starts,
reads the full validator snapshot directly from disk, extracts every DEX pool
account across all supported protocols, and writes the result to a binary file.
The MEV engine reads that file at startup and pre-populates its pool graph
instantly — no Geyser warm-up required.

---

## Architecture

### Two-Phase Account Scan

Scanning a full Solana snapshot naively (reading every account's full data) is
extremely slow — a mainnet snapshot contains hundreds of millions of accounts.

This scanner uses a two-phase strategy:

**Phase 1 — Header-only scan (`scan_accounts_without_data`)**
Reads only the 136-byte account header for every account. Checks the `owner`
field against a HashSet of DEX program IDs. For non-DEX accounts (>99.99% of
all accounts) this is all that happens — no heap allocation, no data read.

**Phase 2 — Full data read (`get_stored_account_callback`)**
Called only for DEX-owned accounts (~0.01% hit rate). Reads the full account
data. Pushes a `PoolRecord` into a thread-local buffer.

### Parallel Scan

AppendVec files (Solana's account storage format) are scanned in parallel via
`rayon::par_bridge`. Each rayon worker accumulates DEX hits into a thread-local
`Vec<PoolRecord>` — no lock contention during the scan itself. The shared
`Mutex<Vec<PoolRecord>>` is only locked once per AppendVec that had any DEX
hits, for the duration of a single `extend()` call.

### Atomic Output

Output is written to `OUTPUT_PATH.tmp` first, then renamed into `OUTPUT_PATH`.
On Linux, `rename(2)` is atomic when source and destination are on the same
filesystem. The MEV engine therefore never reads a partial output file —
it sees either the previous complete file or the new complete file. Never torn.

### Agave Fork Patches

The scanner accesses Agave internals that are `pub(crate)` in the upstream
repository. Four minimal visibility changes are required — documented in full
in `FORK_PATCHES.md`. No logic changes. No validator startup path changes.
No validator behavior changes. Visibility only.

---

## Supported DEX Protocols

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

Byreal support pending confirmed program ID.

---

## Files

| File | Description |
|---|---|
| `src/main.rs` | Entry point — snapshot unpacking, parallel scan, atomic output |
| `src/dex_owners.rs` | DEX program ID registry with startup assertion |
| `src/output.rs` | Serialization types and output path constant |
| `pool_scanner_consumer.rs` | Drop-in consumer for the MEV engine crate |
| `FORK_PATCHES.md` | Exact visibility changes needed in the Agave fork |
| `Cargo.toml` | Crate manifest with Agave path dependencies |

---

## Build Requirements

- Agave validator fork with patches from `FORK_PATCHES.md` applied
- Rust 1.75+
- NVMe storage (snapshot I/O is the bottleneck)
- 128GB+ RAM recommended for full mainnet snapshots

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

In the MEV engine:
```rust
let seed = pool_scanner_consumer::load_from_snapshot()?;
// seed.accounts contains (pubkey, owner, lamports, raw_data) tuples
// seed.snapshot_slot tells you how stale the seed is
```

---

## Relationship to MEV Engine

This scanner is one component of a larger Solana MEV arbitrage system that includes:
- Jito ShredStream integration for low-latency transaction detection
- DEX arbitrage graph with multi-hop path finding
- Flash loan execution via Jito bundles
- Geyser plugin for live pool state updates

The scanner handles the cold-start problem specifically. Geyser handles live updates.
Together they ensure the pool graph is always populated — from first slot onward.
