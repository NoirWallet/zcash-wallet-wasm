build: lint wasm

test-native:
	cargo test --package zcash-wallet-wasm --target x86_64-unknown-linux-gnu

build-wasm:
	wasm-pack build --target web --release --out-dir pkg --features wasm-parallel -Z build-std=panic_abort,std

build-voting-wasm:
	wasm-pack build --target web --release --out-dir pkg-voting --features wasm-parallel,voting -Z build-std=panic_abort,std
	node tests/check_wasm_imports.mjs pkg-voting/zcash_wallet_wasm_bg.wasm

build-voting-sequential:
	wasm-pack build --target web --release --out-dir pkg-voting-sequential --no-default-features --features voting -Z build-std=panic_abort,std
	node tests/check_wasm_imports.mjs pkg-voting-sequential/zcash_wallet_wasm_bg.wasm

build-voting-proof:
	wasm-pack build --target web --release --out-dir pkg-voting-proof --features wasm-parallel,voting-test-fixtures -Z build-std=panic_abort,std
	wasm-pack build --target web --release --out-dir pkg-voting-proof-sequential --no-default-features --features voting-test-fixtures -Z build-std=panic_abort,std

test-voting-browser:
	npm test
	npm run typecheck:voting

lint:
	@cargo fmt --all --package zcash-wallet-wasm
	@cargo clippy --fix --allow-dirty --allow-staged --package zcash-wallet-wasm

wasm:
	wasm-pack build --target web --release --out-dir pkg --features wasm-parallel -Z build-std=panic_abort,std
	./add-worker-module.sh
	rm -f pkg/.gitignore

check:
	cargo check --target wasm32-unknown-unknown -Z build-std=panic_abort,std

.PHONY: build check serve test-native build-wasm build-voting-wasm build-voting-sequential build-voting-proof test-voting-browser
