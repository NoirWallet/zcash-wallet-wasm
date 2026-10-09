# Browser Transport Regressions

This small WASM package imports the production platform and Fetch transport
sources directly. It exercises JavaScript boundary failures without linking the
wallet's cryptographic modules. JavaScript binding versions match the root lock.

The fixture replaces Fetch with controlled host functions and uses small real
ArrayBuffers with overridden `byteLength` properties. It makes no outbound
requests and does not allocate multi-gigabyte buffers. The local server only
serves test artifacts; real HTTP/CORS behavior remains a separate integration
gate.

From the repository root, with the pinned toolchain, wasm-bindgen CLI 0.2.122,
Node, Chrome and matching ChromeDriver installed:

```sh
RUSTFLAGS='' cargo build --manifest-path tests/browser-transport/Cargo.toml --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir tests/browser-transport/pkg tests/browser-transport/target/wasm32-unknown-unknown/debug/browser_transport_regressions.wasm
node tests/browser-transport/run.mjs
```

Set `CARGO_TARGET_DIR` when a different build directory is needed, and use that
path in the wasm-bindgen command. `CHROMEDRIVER` can select a driver executable.
`WASM_BINDGEN_TEST_WEBDRIVER_JSON` can point to Chrome capability JSON, including
an explicit browser binary. The runner supplies headless mode.

The runner exits nonzero for any failed assertion. Generated lockfile, package
and build outputs are ignored. Only the fixture package exports its `request`
hook; the production JavaScript API is unchanged.
