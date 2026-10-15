# Third-Party Notices

`LICENSE.md` covers NoirWallet-owned code and modifications, not pre-existing
third-party work. Those original rights and notices remain in effect even when
third-party code is integrated into a file modified by NoirWallet. This is a
source-provenance inventory, not a complete license list for a compiled WASM
package.

| Material | Upstream and license | Local evidence |
| --- | --- | --- |
| ChainSafe WebZjs-derived wallet code within `src/` | [ChainSafe/WebZjs](https://github.com/ChainSafe/WebZjs), MIT or Apache-2.0 at the recipient's option | File-level source mapping below. This does not describe NoirWallet as a Git fork. |
| `librustzcash/`, including Sprout test vectors | [zcash/librustzcash](https://github.com/zcash/librustzcash), MIT or Apache-2.0 | [`librustzcash/COPYING.md`](librustzcash/COPYING.md) and its license files; source revision in [`librustzcash/UPSTREAM.md`](librustzcash/UPSTREAM.md). |
| `zcash_voting/` | [valargroup/zcash_voting](https://github.com/valargroup/zcash_voting), MIT or Apache-2.0 | [`zcash_voting/LICENSE-MIT`](zcash_voting/LICENSE-MIT), [`zcash_voting/LICENSE-APACHE`](zcash_voting/LICENSE-APACHE), and [`zcash_voting/UPSTREAM.md`](zcash_voting/UPSTREAM.md). |
| `zcash_voting/browser-deps/rusqlite/` | rusqlite, MIT | [`zcash_voting/browser-deps/rusqlite/LICENSE`](zcash_voting/browser-deps/rusqlite/LICENSE). |
| `zcash_voting/browser-deps/pir-client/` | [valargroup/vote-nullifier-pir](https://github.com/valargroup/vote-nullifier-pir), MIT or Apache-2.0 | The vendored crate manifest declares its upstream and license. |
| `src/wallet/zip244_orchard.hex` | [zcash/zcash-test-vectors](https://github.com/zcash/zcash-test-vectors), MIT or Apache-2.0 | Exactly matches the transaction bytes at index 2 (vector 0; the first two entries are metadata) of [`zip_0244.json` at `78321be`](https://github.com/zcash/zcash-test-vectors/blob/78321beacb0e0477e33cd002b56585a107c2708c/test-vectors/zcash/zip_0244.json). Decoded-byte SHA-256: `b91277bb10ec58b576ebcab779ecd65cec12112d827b9209b1c2d4393e9ee9a0`. |

The following files in the initial NoirWallet commit (`543a8c5`) contain code
matching WebZjs source at commit
[`657c960`](https://github.com/ChainSafe/WebZjs/commit/657c960c38edbe2b6da8c5c61c54ecbee6377182),
which predates that commit. Matching portions remain in the current files;
`src/lib.rs` has a smaller match of declarations. This comparison establishes
source overlap, not the exact WebZjs revision originally consulted.

| Local file | Matching WebZjs source under `crates/` |
| --- | --- |
| `src/bindgen/keys.rs` | `webzjs-keys/src/keys.rs` |
| `src/bindgen/pczt.rs` | `webzjs-common/src/pczt.rs` |
| `src/bindgen/requests.rs` | `webzjs-requests/src/requests.rs` |
| `src/bindgen/sign.rs` | `webzjs-keys/src/pczt_sign.rs` |
| `src/bindgen/wallet.rs` | `webzjs-wallet/src/bindgen/wallet.rs` and `proposal.rs` |
| `src/error.rs` | `webzjs-wallet/src/error.rs` |
| `src/init.rs` | `webzjs-wallet/src/init.rs` |
| `src/lib.rs` | `webzjs-wallet/src/lib.rs` |
| `src/network.rs` | `webzjs-common/src/network.rs` |
| `src/wallet.rs` | `webzjs-wallet/src/wallet.rs` |

For the ChainSafe-derived portions, this distribution relies on the
Apache-2.0 option offered by WebZjs. Its full text is included in
[`librustzcash/LICENSE-APACHE`](librustzcash/LICENSE-APACHE); the upstream MIT
option remains available under its original terms. The ChainSafe copyright
and NoirWallet modification notices are retained in each mapped source file:

> Copyright 2024 ChainSafe Systems

For the ZIP-244 vector, this distribution likewise relies on the upstream
Apache-2.0 option. Its upstream MIT license identifies the copyright holder as:

> Copyright (c) 2018-2021 The Electric Coin Company

Rust registry dependencies and JavaScript development dependencies are pinned
by `Cargo.lock` and `package-lock.json`; their individual metadata and license
files remain authoritative. A redistributor of the built WASM/JavaScript
package must separately collect the notices applicable to the actual binary.
