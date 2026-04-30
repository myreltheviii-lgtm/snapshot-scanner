# Fork Visibility Patches
# These are the ONLY changes needed in the agave fork for the scanner to compile.
# All other code is in the standalone binary — no changes to validator startup path.

---

## 1. snapshot_utils.rs
### `unarchive_snapshot` — already pub, no change needed.
### `get_highest_full_snapshot_archive_info` — already pub, no change needed.
### `UnarchivedSnapshot.storage` field:

Find:
```rust
pub(crate) struct UnarchivedSnapshot {
    ...
    pub(crate) storage: AccountStorageMap,
    ...
}
```

Change:
```rust
pub struct UnarchivedSnapshot {
    ...
    pub storage: AccountStorageMap,
    ...
}
```

---

## 2. account_storage.rs
### `AccountStorage::all_storages` — only needed if you use AccountsDb path, NOT needed for the unarchived.storage approach.
### `AccountStorageEntry.accounts` field:

Find:
```rust
pub struct AccountStorageEntry {
    pub(crate) accounts: AccountsFile,
    ...
}
```

Change:
```rust
pub struct AccountStorageEntry {
    pub accounts: AccountsFile,
    ...
}
```

---

## 3. append_vec.rs / accounts_file.rs
### `scan_accounts_without_data` — verify it is `pub`. If it is `pub(crate)`:

Find:
```rust
pub(crate) fn scan_accounts_without_data<F>(&self, callback: F) -> Result<()>
```

Change:
```rust
pub fn scan_accounts_without_data<F>(&self, callback: F) -> Result<()>
```

### `get_stored_account_callback` — same check:

Find:
```rust
pub(crate) fn get_stored_account_callback<F>(&self, offset: usize, callback: F)
```

Change:
```rust
pub fn get_stored_account_callback<F>(&self, offset: usize, callback: F)
```

---

## Summary — Maximum 4 visibility changes, minimum 2.

The UnarchivedSnapshot.storage field is the most critical.
AccountStorageEntry.accounts is the second most critical.
The scan methods are likely already pub — check before touching.

No logic changes. No startup path changes. No validator behavior changes.
