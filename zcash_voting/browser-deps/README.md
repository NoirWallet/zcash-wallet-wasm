# Browser Dependency Adapters

These exact crates.io release sources are copied locally because their native
dependency wiring prevents the voting browser build. They are direct path
dependencies of the voting crate, not global registry overrides.

## rusqlite 0.40.2

- Repository: https://github.com/rusqlite/rusqlite
- Release archive: https://static.crates.io/crates/rusqlite/rusqlite-0.40.2.crate
- SHA-256: `23f2a97da3e3873c73cb2a2e71b35c40ff95e0b1eefa8d72d8499a6928c3b5b3`

Only the normalized `Cargo.toml` is patched: remove both target-specific
`libsqlite3-sys` dependencies and every `libsqlite3-sys?/...` feature edge.
The Rust source, version, and original manifest are unchanged. This copy is
for `wasm32-unknown-unknown` with `ffi-sqlite-wasm-rs` only; it is not a native
rusqlite replacement. The voting dependency also enables `cache`, `serialize`,
and `fallible_uint`, preserving upstream voting's checked `u64` SQL conversions.
It resolves exactly `sqlite-wasm-rs` 0.5.5.

Cargo enforces `links = "sqlite3"` uniqueness across target dependency tables,
even for inactive optional dependencies. Removing unused native wiring here
allows native voting and wallet crates to retain registry rusqlite 0.37.0 and
libsqlite3-sys 0.35.0 while WASM uses this independent implementation.
The dependency is named `rusqlite-browser` and conditionally aliased in the
voting crate, because Cargo forbids different source paths under the same
dependency name across targets.

## pir-client 0.7.2

- Repository: https://github.com/valargroup/vote-nullifier-pir
- Release archive: https://static.crates.io/crates/pir-client/pir-client-0.7.2.crate
- SHA-256: `ba1965648d9ea094809794710b0a654c9376d034b38405fba610905d91150bbd`

The normalized manifest moves Tokio to a non-WASM dependency table.
`src/lib.rs` gates `PirClientBlocking` and its implementation to native targets,
and uses the existing `futures::try_join!` for the two independent tier fetches.
The asynchronous `PirClient`, local proof APIs, parsing, protocol validation,
and transport traits remain compiled on both targets. Native blocking behavior
is retained. The voting crate uses this same path dependency on both targets.

## Import Verification

Both copies retain the released source and license files. Registry extraction
markers `.cargo-ok` and `.cargo_vcs_info.json` are omitted. The release archives'
SHA-256 checksums above were verified before import; Cargo's original release
manifests remain available as `Cargo.toml.orig` for comparison.
