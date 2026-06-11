build: lint wasm

lint:
	@cargo fmt --all --package zcash-wallet-wasm
	@cargo clippy --fix --allow-dirty --allow-staged --package zcash-wallet-wasm

wasm:
	wasm-pack build --target web --release --out-dir pkg --features wasm-parallel -Z build-std=panic_abort,std
	./add-worker-module.sh
	rm -f pkg/.gitignore

check:
	cargo check --target wasm32-unknown-unknown -Z build-std=panic_abort,std

.PHONY: build check serve