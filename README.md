# Zcash wallet WASM

This repository contains the Rust/WebAssembly wallet core used by the Noir
Wallet browser extension. It exposes wallet and voting APIs to a JavaScript
host. It is not a standalone extension, a key store, or a complete privacy
policy for the host application.

The wallet handles Zcash account import, scanning, balances, addresses, PCZT
creation and signing, and transaction submission. The `voting` feature adds
`WebVotingSession`. The release package enables `wasm-parallel,voting` and
exports `WebWallet`, `WebVotingSession`, and `initThreadPool`. The generated
`pkg/zcash_wallet_wasm.d.ts` describes the exact JavaScript API of a built
package.

## Build and verification

Use [BUILDING.md](BUILDING.md) to build the release package from a specific
source commit in the pinned Linux/amd64 Docker environment. The release
command inside the container is `make build-voting-wasm`; other Makefile
targets may enable different features or test fixtures. Do not mix files from
different `pkg/` builds. A source commit alone does not identify the binary
deployed by an extension; verify the entire package against its `SHA256SUMS`.

The parallel WASM package uses shared memory. Its browser host must provide
cross-origin isolation and `SharedArrayBuffer`, and correctly serve the WASM,
JavaScript, and worker files. The extension's permissions, Content Security
Policy, storage, and update mechanism are outside this repository.

## Security boundaries

The JavaScript host supplies wallet secrets, serialized wallet state, and
network endpoints. It is responsible for user authorization before signing,
protecting data at rest, choosing trusted HTTPS services, and reviewing what
is displayed to the user. See [PRIVACY.md](PRIVACY.md) for the data flows and
network trust limits visible in this code. Passing data into WASM does not
hide it from the host or automatically erase its JavaScript copies.

For private vulnerability reports and the published audit report, see
[SECURITY.md](SECURITY.md). No audit claim applies to a release unless its
exact source commit, enabled features, dependencies, and scope are covered by
the report.

## License

NoirWallet-owned code is available under the [Business Source License 1.1](LICENSE.md)
with no Additional Use Grant. Non-production use is allowed before the
2029-01-01 Change Date; production use requires a separate license until the
work changes to Apache-2.0 under the license terms. BUSL-1.1 is source-available,
not an Open Source license before that change. Separately licensed third-party
code retains its own notices and terms.
See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for the identified
upstream code and test materials; the source-provenance review is still open.
