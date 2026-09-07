# Valar Voting Upstream

- Repository: https://github.com/valargroup/zcash_voting
- Commit: `57e25c7dae7d75fe746e5f40e450f6cd65c5c7c5`
- Crate: `zcash_voting` `3.1.0-rc.15`
- Imported: 2026-09-07
- Wallet integration backend: `lrz`, with `browser`; never `zakura`.

This directory contains the complete workspace from that commit, copied with
`rsync -a --exclude=.git` after checking out and verifying the exact revision.
It is ordinary source, not a submodule. Upstream licenses, tests, examples,
documentation, and release tooling are retained.
One extra EOF blank line in `docs/immediate-share-selection-plan.md` is removed
so the vendor import passes `git diff --check`.

## Local Compatibility Patches

The root and this workspace patch public LRZ crates to `../librustzcash` so
PCZT, wallet backend, keys, primitives, protocol, and transparent types come
from the same local source. WASM imports those crates directly; native builds
retain the upstream `zakura-wallet-lib` facade with its `lrz` feature.

The browser patch categories are:

- Dependency gating: native Hyper, Rustls, Tonic channels, wallet SQLite,
  and multi-thread Tokio are target-gated. The browser graph uses the pinned
  dependency adapters documented in [browser-deps/README.md](browser-deps/README.md).
- Input-based wallet boundary: native wallet DB convenience entry points are
  gated. Protocol types, note bundling, signing, and witness validation remain
  compiled; the browser input adapter is added in the subsequent wallet task.
- SQLite serialization: `serialize` is enabled for both SQLite variants.
  Browser connection ownership, byte loading, and serialization APIs belong
  to the subsequent storage task; schema version 17 is unchanged here.
- Fetch transport: native implementations are gated while owned transport
  traits and response parsers remain compiled. Fetch is added in the transport task.
- Async tree sync: the native Hyper convenience method is gated; the pure
  tree client, validation, and witness core remain. Async fetching is added later.
- Proof executor: native large-stack threads are retained on native targets;
  WASM compiles the same proof construction directly and sequentially. The
  dedicated browser executor and worker orchestration are added later.
- Helper local futures: native Tokio preflight, submission, and polling entry
  points are gated. Policy, wire parsing, durable storage, recovery, planning,
  and `session::resume_plan` remain compiled. Browser timers, local futures,
  and checkpoint hooks are added in the helper task.

Target gates intentionally preserve native APIs when `browser` is enabled on
a native test host. This permits contract tests alongside upstream regressions.
The `browser` feature rejects the Zakura backend at compile time.

## Updating

The session facade additionally exposes `VotingDb::round_session_json`, a
wallet-scoped read of caller-owned round context. Missing rounds return an
error, while a present round without context returns `None`. Schema and helper
lifecycle semantics are unchanged.
`VotingDb::round_params_with_network` similarly exposes a wallet-scoped read
of the complete stored protocol parameters and network for recovery identity
comparison, with safe errors for missing or unreadable records.

Browser preparation adds schema v19's wallet/round-scoped voting hotkey column,
with create-once/load APIs and no changes to the caller's strict authenticated
session envelope. Async PIR cache preparation releases SQLite ownership before
fetching or validating proofs and reuses the existing cache verification path.
The snapshot transport binds the actual PIR handshake to the authenticated
testnet network, Ironwood dataset, layout, IMT root and height. The PIR adapter
uses browser clocks and validates the public encoded response length before
upstream decoding; query encoding and proof verification remain unchanged.
Owned proof inputs may run sequentially or on the host's initialized Rayon pool;
native large-stack proof behavior remains target-gated and unchanged.

Check out the proposed upstream commit separately, review protocol/schema and
dependency changes, and merge the documented compatibility patches explicitly.
Update this revision and both Cargo lockfiles only after native LRZ tests,
root voting WASM compilation, and dependency-source checks pass. Do not copy
upstream `.git` metadata or replace the local LRZ graph with registry duplicates.
