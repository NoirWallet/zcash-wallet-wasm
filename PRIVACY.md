# Security and privacy boundaries

This document describes the WASM module's observable behavior, not the
browser extension's complete privacy policy. The host selects storage and
most network endpoints, so its configuration and deployment must be reviewed
separately. The code does not make a third-party service anonymous merely by
using shielded Zcash transactions or HTTPS.

## Sensitive data crossing the JavaScript boundary

| Data | Entry or exit | Responsibility and exposure |
| --- | --- | --- |
| Seed phrase and optional passphrase | `WebWallet.create_account` receives JavaScript strings | The host already has these values. Account derivation runs locally; the host must protect input, copies, UI state, and backups. JavaScript strings cannot be reliably zeroized by this API. |
| Spending key and seed bytes | `UnifiedSpendingKey` and signing APIs receive byte arrays or key objects | Treat both host and WASM memory as sensitive. Do not log, transmit, or persist keys without a separate user-authorized design. |
| Viewing keys, addresses, transaction metadata, and wallet state | Wallet queries and `db_to_bytes` / `db_to_bytes_chunked` return data to the host | Serialized wallet bytes are sensitive and are **not an encrypted backup format** supplied by this module. The host chooses encryption, persistence, access control, and deletion behavior. |
| PCZT and voting state | Signing, submission, `WebVotingSession.exportState`, and restore methods exchange serialized values | These can reveal account activity or contain transaction authorization material. The host controls storage and any transfer to another process or device. |
| Errors, warnings, and panics | Rejected calls and the browser console | Release builds install a panic hook and emit `WARN`-level tracing. CDN failure warnings can contain the configured request URL or JavaScript exception text; service-provided error text can reach rejected calls. Review and redact errors before collecting or sharing them. |

Relevant code: `src/bindgen/wallet.rs`, `src/bindgen/keys.rs`,
`src/bindgen/voting.rs`, `src/init.rs`.

`db_to_bytes_chunked` currently ignores exceptions thrown by its JavaScript
callback and does not await a Promise returned by that callback. Its successful
return does not prove that every chunk was durably stored. Do not treat it as
confirmation of a completed backup; verify the stored bytes before relying on
the backup, or use `db_to_bytes` with a separately checked storage operation.

## Network data flow

The trigger column describes this module's configuration or the named method,
not whether the extension actually calls that method. Remote services can
usually observe the client IP address, request time, and request path. Their
retention policies are outside this repository.

| Data and recipient | Trigger and default | Existing protection | Remaining visibility or risk |
| --- | --- | --- | --- |
| Block requests, tree state, transaction lookup and transparent-address queries to the host-supplied lightwalletd endpoint | Wallet sync and related queries; endpoint is required when creating `WebWallet` | Wallet-side scanning and protocol checks; transport is browser gRPC-Web | The service can correlate IP, timing, requested heights, and transparent-address queries. The host must choose and validate the endpoint and its transport. |
| Network, requested height ranges, and block chunk paths to the host-supplied compact-block CDN | Optional `CompactBlockCdnOptions`; otherwise no CDN requests | Manifest, index, hash, range, and chain-continuity checks with lightwalletd fallback | The CDN can see IP and request timing. `http://` is currently accepted. The browser fetch uses default credential and redirect behavior; it buffers a complete response before enforcing `maxChunkBytes`. Server-side chunk limits do not bound a malicious or faulty response. |
| Raw transaction and submission timing to configured zero-indexer endpoints or lightwalletd | Only when broadcasting; zero-indexer endpoints are optional runtime configuration for transactions containing an Orchard bundle | Per-endpoint timeout and response handling | Zero-indexer URL options are only checked for nonempty strings here; this module does not enforce HTTPS for them. Depending on the result, the same transaction may also be sent to lightwalletd. Recipients can correlate broadcast timing with the client IP. An uncertain network response is not proof that a transaction was rejected; the host must present ambiguous results correctly. |
| Pinned voting configuration, active-round requests, PIR queries, tree/proof requests, ballots, and helper-share traffic to voting services | Voting methods such as `discoverRounds`, eligibility checks, and submission; no voting request merely from constructing a wallet | Built-in static configuration pin, authenticated round checks, HTTPS endpoint validation, credential-free and redirect-rejecting bounded Fetch transport | Services can observe IP, timing, round activity, and the requests they receive. Protocol-level privacy does not hide all network metadata or imply a retention policy. |

Relevant code: `src/wallet.rs`, `src/compact_block_source.rs`,
`src/voting/config.rs`, `src/voting/network.rs`, `src/voting/transport.rs`,
`src/voting/session.rs`.

## Host integration rules

- Review the transaction details shown to the user against the exact data
  submitted for signing; the WASM module does not provide the host's consent UI.
- Encrypt and access-control persisted wallet bytes and voting checkpoints.
  A browser profile, extension storage, or OS account is not by itself a
  cryptographic protection guarantee.
- Use trusted HTTPS endpoints and review redirects, credentials, proxy use,
  logging, and service retention at the host and deployment layers. This
  repository does not establish Tor or proxy coverage for every request.
- Do not put credentials in service URLs. CDN warnings can include full URLs;
  treat network errors and console output as sensitive when reporting failures.
- Do not log seed phrases, keys, wallet state, or account-linked addresses.
  Addresses are not secrets, but logs can connect wallet activity to an identity.
- Keep the complete JS/WASM/worker package from one verified build. A source
  audit does not verify a deployed binary unless its identity is checked.

This is an implementation boundary summary, not a completed security audit
or a claim that the extension has been independently reviewed.
