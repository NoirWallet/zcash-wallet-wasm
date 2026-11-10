build: lint build-wasm

test-native:
	cargo test --package zcash-wallet-wasm --target x86_64-unknown-linux-gnu

build-wasm:
	wasm-pack build --target web --release --out-dir pkg --features wasm-parallel -Z build-std=panic_abort,std

build-voting-wasm:
	wasm-pack build --target web --release --out-dir pkg --features wasm-parallel,voting -Z build-std=panic_abort,std --locked
	node tests/check_wasm_imports.mjs pkg/zcash_wallet_wasm_bg.wasm

build-voting-sequential:
	wasm-pack build --target web --release --out-dir pkg-sequential --no-default-features --features voting -Z build-std=panic_abort,std
	node tests/check_wasm_imports.mjs pkg-sequential/zcash_wallet_wasm_bg.wasm

build-voting-proof:
	wasm-pack build --target web --release --out-dir pkg-proof --features wasm-parallel,voting-test-fixtures -Z build-std=panic_abort,std
	wasm-pack build --target web --release --out-dir pkg-proof-sequential --no-default-features --features voting-test-fixtures -Z build-std=panic_abort,std

# FROST Secure Vault. Optional feature: without it the package is byte-for-byte the shipping wallet.
# The frontend builds with this target; the module is emitted to `pkg-frost` so `pkg` keeps the
# ordinary build.
build-frost-wasm:
	wasm-pack build --target web --release --out-dir pkg-frost --features wasm-parallel,frost-vault -Z build-std=panic_abort,std
	node tests/check_wasm_imports.mjs pkg-frost/zcash_wallet_wasm_bg.wasm

# Everything the wallet can currently be built with: base + voting + frost.
build-all-wasm:
	wasm-pack build --target web --release --out-dir pkg-all --features wasm-parallel,voting,frost-vault -Z build-std=panic_abort,std
	node tests/check_wasm_imports.mjs pkg-all/zcash_wallet_wasm_bg.wasm
	@for n in Dkg Coordinator Signer Keyset ShareBackup Canary Deposit Verification MessageSet SessionMarker EnvelopeCodec PcztTools; do \
		grep -q "class $$n" pkg-all/zcash_wallet_wasm.d.ts || { echo "missing vault export: $$n"; exit 1; }; \
	done
	@echo "vault exports present in pkg-all"

check-frost:
	cargo check --target wasm32-unknown-unknown -Z build-std=panic_abort,std --features frost-vault

check-all:
	cargo check --target wasm32-unknown-unknown -Z build-std=panic_abort,std --features wasm-parallel,voting,frost-vault

# The vault crates plus a real wasm run of the protocol code. This is the gate that catches
# problems the native tests cannot see (32-bit usize, linker flags, missing imports).
# One shared target dir: these crates share almost every dependency, so without it each of the six
# suites rebuilds Halo2 and the browser test suite takes the best part of half an hour.
test-frost:
	CARGO_TARGET_DIR=target/frost-tests cargo test --target x86_64-unknown-linux-gnu --manifest-path crates/frost-vault-core/Cargo.toml
	CARGO_TARGET_DIR=target/frost-tests cargo test --target x86_64-unknown-linux-gnu --manifest-path crates/pczt-signer-core/Cargo.toml
	CARGO_TARGET_DIR=target/frost-tests cargo test --target x86_64-unknown-linux-gnu --manifest-path crates/vault-keyset/Cargo.toml
	CARGO_TARGET_DIR=target/frost-tests cargo test --target x86_64-unknown-linux-gnu --manifest-path crates/vault-deposit/Cargo.toml
	CARGO_TARGET_DIR=target/frost-tests cargo test --target x86_64-unknown-linux-gnu --manifest-path crates/vault-pczt/Cargo.toml
	CARGO_TARGET_DIR=target/frost-tests cargo test --target x86_64-unknown-linux-gnu --manifest-path crates/vault-flow-tests/Cargo.toml
	CARGO_TARGET_DIR=target/frost-tests cargo build --release --target wasm32-unknown-unknown --manifest-path crates/vault-wasm-smoke/Cargo.toml
	node crates/vault-wasm-smoke/run.mjs

# The vault JS API on its own, as a node package, plus the JS checks that drive it. Fast (~15 s),
# unlike the wallet bundle, so this is the loop to use while working on the vault layer.
build-vault-wasm:
	cd crates/vault-wasm && wasm-pack build --target nodejs --release --out-dir pkg-node

test-vault-wasm: build-vault-wasm
	node crates/vault-wasm/test/basic.mjs

test-voting-browser:
	npm test
	npm run typecheck:voting

lint:
	@cargo fmt --all --package zcash-wallet-wasm
	@cargo clippy --fix --allow-dirty --allow-staged --package zcash-wallet-wasm

check:
	cargo check --target wasm32-unknown-unknown -Z build-std=panic_abort,std

# Every Cargo target dir in this tree. `cargo clean` only knows about the root package: the crates
# under `crates/` are standalone packages, not workspace members, so each keeps its own `target/`
# and has to be removed by hand. The shared test dir lives under the root `target/`.
clean-cargo:
	cargo clean
	rm -rf crates/*/target

# Build outputs as well as target dirs: the wasm packages and the release staging directory.
clean: clean-cargo
	rm -rf pkg pkg-* wasm-dist

.PHONY: clean clean-cargo build lint check serve test-native build-wasm build-voting-wasm build-voting-sequential build-voting-proof build-frost-wasm build-all-wasm check-frost check-all test-frost build-vault-wasm test-vault-wasm test-voting-browser
