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

test-voting-browser:
	npm test
	npm run typecheck:voting

lint:
	@cargo fmt --all --package zcash-wallet-wasm
	@cargo clippy --fix --allow-dirty --allow-staged --package zcash-wallet-wasm

check:
	cargo check --target wasm32-unknown-unknown -Z build-std=panic_abort,std

.PHONY: build lint check serve test-native build-wasm build-voting-wasm build-voting-sequential build-voting-proof test-voting-browser
