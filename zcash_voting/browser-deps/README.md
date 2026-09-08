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

The maintained release-to-local patch categories are:

- The normalized manifest moves Tokio to a non-WASM dependency table and adds
  WASM-only `js-sys`. `PirClientBlocking` and its implementation remain native;
  the async client and proof APIs compile on both targets.
- WASM timing uses a `Date::now()` adapter with negative elapsed time clamped
  to zero; native timing retains `std::time::Instant`.
- Root/layout/geometry validation precedes large asset downloads.
  `futures::join!` finishes both tier fetches and independently validates every
  successful asset, including Tier 0 length/root and Tier 1 scenario geometry.
  A permanent validation error dominates a sibling retryable transport error.
- `RetryableTransportError` is a typed host classification, retained through
  `anyhow::Context` error chains. Limit, identity, decode and proof failures
  must not carry this marker. Non-success query responses use the marker.
- Before assertion-based SimplePIR decoding, response length is checked against
  the codec's ciphertext geometry and instance count using checked arithmetic.
- Query failure logging omits raw transport errors and response bodies.
  Query encoding and cryptographic proof verification remain upstream.

The voting crate uses this same adapter on both targets. The reviewed complete
release-to-local diff SHA-256 is
`ddab09cabd842d5a0828066f3396d4ffc6983300653b1bd769b072e7884706f0`.
Final maintained file SHA-256 values (release source/archives remain above):

| File | SHA-256 |
| --- | --- |
| `Cargo.toml` | `2a92f58cfd349c20c20391454b85e60f28bef15ee287baeef24b8821dc255a7d` |
| `src/lib.rs` | `7b78f0cef77c098a251768e86096cb5382b170884579884656364d124d8c7f03` |
| `src/transport.rs` | `45b9920ad251d0e4dbc51fe122aafe0b54a1ebe77ebeb52f6a102acd004a80a3` |

## Import Verification

Both copies retain the released source and license files. Registry extraction
markers `.cargo-ok` and `.cargo_vcs_info.json` are omitted. The release archives'
SHA-256 checksums above were verified before import; Cargo's original release
manifests remain available as `Cargo.toml.orig` for comparison.
