use solana_pubkey::Pubkey;
use std::collections::HashSet;
use std::str::FromStr;

/// All DEX program IDs your MEV engine covers.
/// Every pool account's owner field matches exactly one of these.
///
/// EXPECTED_COUNT must equal the number of entries in `ids`.
/// The assertion at the end catches silent parse failures and duplicate IDs
/// at binary startup — far better than silently missing entire DEX pools.
///
/// When Byreal ID is confirmed: add it to `ids` and bump EXPECTED_COUNT to 11.
pub fn dex_owners() -> HashSet<Pubkey> {
    const EXPECTED_COUNT: usize = 10;

    let ids: &[&str] = &[
        // Raydium V4 (AMM)
        "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
        // Raydium CPMM
        "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C",
        // Raydium CLMM
        "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
        // Orca Whirlpool
        "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
        // Meteora DLMM
        "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo",
        // Meteora DAMM V1
        "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB",
        // Meteora DAMM V2
        "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG",
        // PumpSwap
        "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",
        // PancakeSwap
        "6MLxLqofvEgqqoYnVzcW4B2NLMHKbvHqAiV9bHuDqg5t",
        // Humidifi
        "HUMIDxKRmBpWqRZMRusFmHGMCJrGC3xbXVhezQfMgK9J",
        // Byreal — TODO: replace placeholder with confirmed program ID, bump EXPECTED_COUNT = 11
        // "BYREAL_PROGRAM_ID_HERE",
    ];

    let set: HashSet<Pubkey> = ids
        .iter()
        .map(|s| {
            // Hard panic — a bad pubkey here means an entire DEX's pools are silently missed.
            // Fix the string above, never silence this.
            Pubkey::from_str(s)
                .unwrap_or_else(|e| panic!("invalid DEX owner pubkey {s:?}: {e}"))
        })
        .collect();

    // Catches duplicates (HashSet collapses them) and any IDs accidentally removed.
    assert_eq!(
        set.len(),
        EXPECTED_COUNT,
        "expected {EXPECTED_COUNT} unique DEX owner pubkeys, got {}. \
         Check for duplicates or bad base58 strings in dex_owners()",
        set.len()
    );

    set
}
