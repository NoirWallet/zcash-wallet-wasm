# Rebuilding the wallet WASM

The wallet uses the `wasm-parallel,voting` release package built by
`make build-voting-wasm`. `Dockerfile.wasm` builds the same Rust feature set and
exports the generated `pkg/` files. It does not build the browser extension.

## Requirements

- Docker with Buildx and enough disk and memory for an optimized Rust/WASM build.
- A clean checkout of the source commit to reproduce. Use its full commit SHA;
  a tag is not required. Do not use a moving branch name as the build identity.
- Network access on the first build for the pinned tool downloads and locked
  Cargo dependencies. The tool archives are checked against SHA-256 values in
  the Dockerfile.

The container uses Linux amd64, pinned Rust and Node image digests,
`nightly-2025-11-15`, `wasm-pack 0.15.0`, `wasm-bindgen 0.2.122`, and
`wasm-opt 117`. Clang 19 comes from the 2025-12-08 Debian snapshot. Cargo
must accept the checked-in `Cargo.lock` without changes.

## Build

Set `EXPECTED_SHA` to the published full source commit SHA before running this
block in a clean checkout. The checks stop if the checkout differs from that
commit or contains tracked or untracked changes.

```bash
set -euo pipefail
: "${EXPECTED_SHA:?Set EXPECTED_SHA to the published full commit SHA}"
test "$(git rev-parse HEAD)" = "$EXPECTED_SHA"
test -z "$(git status --porcelain)"
test ! -e wasm-dist/pkg
mkdir -p wasm-dist
docker buildx build --platform linux/amd64 -f Dockerfile.wasm \
  --target wasm-package --output type=local,dest=./wasm-dist \
  --no-cache --progress plain . 2>&1 | tee wasm-dist/build.log
find wasm-dist/pkg -type f -print0 | sort -z | xargs -0 sha256sum \
  | tee wasm-dist/SHA256SUMS
```

The output is in `wasm-dist/pkg/`, which is ignored by Git; the full build log
is `wasm-dist/build.log` and the file hashes are `wasm-dist/SHA256SUMS`. Save the
complete source SHA, Dockerfile revision, Docker image digests, and all output
hashes together when reporting a build. To check byte reproducibility, run the
same block from a second independent clean checkout of the same SHA, then
compare the two `SHA256SUMS` files. Pinned inputs and one successful build alone
do not establish byte reproducibility.

The `pkg/` output uses shared WebAssembly memory. The browser host must support
`SharedArrayBuffer` and cross-origin isolation, including compatible COOP/COEP
headers for the page, workers, and loaded resources. The separate
`build-voting-proof` target includes test fixtures and is not a release build.
The container checks the shared-memory import contract, required wallet/voting
exports, and the absence of voting test-only exports before copying any output.

The release package can be exercised in Chrome with the repository's isolated
test server and synthetic wallet fixtures:

```sh
npm ci
npx playwright install chromium
cargo test --locked --package zcash-wallet-wasm --features voting \
  --target x86_64-unknown-linux-gnu write_browser_session_fixtures -- --ignored
SESSION_WASM_DIR="$PWD/wasm-dist/pkg" \
SESSION_TEST_PAGE=release-smoke.html \
node tests/browser-session/run.mjs
```

Install the Chromium version required by the checked-in Playwright dependency
before running the browser smoke test. Set `CHROME` to a compatible Chromium
executable when using a system browser.
