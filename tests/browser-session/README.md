# Session Browser Regressions

These tests exercise the real root WASM module in Chrome, using synthetic wallet
and voting snapshots. They perform no protocol submission or remote discovery.
The local runner serves the atomics-enabled module with COOP/COEP headers.
Hosts must serve `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp` (and compatible worker/asset
responses) so shared WebAssembly memory and the existing Rayon pool can run.

```sh
cargo test --target x86_64-unknown-linux-gnu --features voting write_browser_session_fixtures -- --ignored
wasm-pack build --target web --dev --out-dir /tmp/zcash-voting-bindings --features voting -Z build-std=panic_abort,std
node tests/browser-session/run.mjs
```

The native fixture generator writes only public, synthetic test seed material
to `/tmp/task-8-browser-fixtures`. Checkpoint tests cover synchronous throws,
rejections, plain returns, resolved promises, thenables, pending callbacks,
BigInt revisions, copied buffers, deletion persistence and stable JS errors.
Install the Chromium version required by Playwright or set `CHROME` to a
compatible system Chromium executable. Set `SESSION_WASM_DIR` to a
default-feature build directory to run the same shared-memory and Rayon
initialization checks plus an ordinary async wallet read without voting.
