# Third-Party Notices

`LICENSE.md` covers NoirWallet-owned code and modifications, not pre-existing
third-party work. Those original rights and notices remain in effect even when
third-party code is integrated into a file modified by NoirWallet. This is a
source-provenance inventory, not a complete license list for a compiled WASM
package.

| Material | Upstream and license | Local evidence |
| --- | --- | --- |
| ChainSafe WebZjs-derived wallet code within `src/` | [ChainSafe/WebZjs](https://github.com/ChainSafe/WebZjs), MIT or Apache-2.0 at the recipient's option | The initial `src/lib.rs`, `src/wallet.rs`, and wallet bindings share code with WebZjs; `src/network.rs` retains `Copyright 2024 ChainSafe Systems`. The complete file/portion mapping is pending. |
| `librustzcash/`, including Sprout test vectors | [zcash/librustzcash](https://github.com/zcash/librustzcash), MIT or Apache-2.0 | [`librustzcash/COPYING.md`](librustzcash/COPYING.md) and its license files; source revision in [`librustzcash/UPSTREAM.md`](librustzcash/UPSTREAM.md). |
| `zcash_voting/` | [valargroup/zcash_voting](https://github.com/valargroup/zcash_voting), MIT or Apache-2.0 | [`zcash_voting/LICENSE-MIT`](zcash_voting/LICENSE-MIT), [`zcash_voting/LICENSE-APACHE`](zcash_voting/LICENSE-APACHE), and [`zcash_voting/UPSTREAM.md`](zcash_voting/UPSTREAM.md). |
| `zcash_voting/browser-deps/rusqlite/` | rusqlite, MIT | [`zcash_voting/browser-deps/rusqlite/LICENSE`](zcash_voting/browser-deps/rusqlite/LICENSE). |
| `zcash_voting/browser-deps/pir-client/` | [valargroup/vote-nullifier-pir](https://github.com/valargroup/vote-nullifier-pir), MIT or Apache-2.0 | The vendored crate manifest declares its upstream and license. |
| `src/wallet/zip244_orchard.hex` | Annotated in its test as ZIP-244 vector 0 | Exact upstream byte source and required attribution still need confirmation. |

For the ChainSafe-derived portions, this distribution relies on the
Apache-2.0 option offered by WebZjs. Its full text is included in
[`librustzcash/LICENSE-APACHE`](librustzcash/LICENSE-APACHE); the upstream MIT
option remains available under its original terms. The ChainSafe copyright
notice is retained here and in `src/network.rs`:

> Copyright 2024 ChainSafe Systems

Rust registry dependencies and JavaScript development dependencies are pinned
by `Cargo.lock` and `package-lock.json`; their individual metadata and license
files remain authoritative. A redistributor of the built WASM/JavaScript
package must separately collect the notices applicable to the actual binary.
