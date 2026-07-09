# librustzcash Sync Notes

This directory records local changes that need to be reviewed when syncing the
vendored `librustzcash` tree with upstream Zcash.

## Local Changes Recorded Before Ironwood Sync

- Source branch before sync: `main`
- Work branch: `sync-librustzcash-ironwood-testnet`
- Local project commit containing vendored library changes:
  `768242dab7cc4dd74a5e15cf67db2ea31e361102`
- Patch file: `local-librustzcash-changes-768242d.patch`

The local library patch touches:

- `librustzcash/zcash_primitives/src/transaction/builder.rs`
  - Adds `Builder::expiry_height`.
  - Adds `Builder::with_expiry_height`.
- `librustzcash/zcash_client_backend/src/data_api/wallet.rs`
  - Allows PCZT creation to override transaction expiry height.
  - Adds `create_pczt_from_proposal_with_expiry_delta`.

During the Ironwood sync, these changes should be reapplied only if upstream has
not already added an equivalent API.

## Ironwood Testnet Sync

- Upstream repository: `https://github.com/zcash/librustzcash`
- Upstream branch: `dw/merge-ironwood-branch`
- Upstream commit: `139601061b89d622dc4c9464f1e6638611ea0027`
- Sync target: NU6.3 / Ironwood testnet activation logic.

The official branch adds `NetworkUpgrade::Nu6_3`, `BranchId::Nu6_3`, v6
transaction constants, and testnet activation height `4_134_000`.

Local adaptations kept after the sync:

- `zcash_client_memory` is kept in the workspace because this project depends on
  it, while upstream has removed it from the Ironwood branch.
  - Upstream removal commit: `59c685cd3ae44628026ef52820d7c436276e1e24`.
  - Upstream reason: the in-memory wallet backend now lives in its own
    repository at `https://github.com/zcash/zcash_client_memory`.
- `create_pczt_from_proposal_with_expiry_delta` is kept as a local wrapper on top
  of upstream `Builder::with_expiry_height`.
- `sync::run_next_batch` and public `sync::refresh_utxos` are kept as local
  compatibility APIs for this project's incremental sync paths.
- `pczt` WASM helpers now use the crate's binary `serialize` / `parse` APIs
  instead of serde derives, because upstream `pczt::Pczt` no longer implements
  serde serialization.
- Orchard proving and verifying keys are initialized with
  `OrchardCircuitVersion::PostNu6_3` for Ironwood testnet.

Follow-up items before mainnet final:

- `zcash_client_memory` now stores an optional Orchard `note_version` in its note
  protobuf. Missing values from older snapshots are treated as V2.
- Mainnet activation height is not set in this upstream branch; resync from the
  official final release before enabling mainnet Ironwood behavior.
