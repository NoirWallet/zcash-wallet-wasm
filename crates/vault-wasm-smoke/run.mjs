#!/usr/bin/env node
// Run the protocol crates under wasm and check the results.
//
// Native tests run on 64-bit; wasm32 has a 32-bit `usize`, different panic behaviour and no threads.
// This script instantiates the real module in Node and calls into it, which is the only way to know
// the crates actually work where they will be deployed.
//
// Usage: node run.mjs   (after `cargo build --release --target wasm32-unknown-unknown`)

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(
  here,
  "target/wasm32-unknown-unknown/release/vault_wasm_smoke.wasm",
);

const bytes = await readFile(wasmPath);

// The repository root `.cargo/config.toml` applies the wallet's threaded wasm settings to every
// wasm32 build under it: `--import-memory --shared-memory --max-memory=4 GiB`. So a cdylib here is
// not self-contained; it expects the host to provide the shared memory, which is what the Extension
// does anyway. Supplying it is part of testing under the real flags.
const memory = new WebAssembly.Memory({
  initial: 256, // 16 MiB
  maximum: 65536, // 4 GiB, matching --max-memory
  shared: true,
});

const { instance } = await WebAssembly.instantiate(bytes, { env: { memory } });
const e = instance.exports;

let failures = 0;
function check(name, actual, expected) {
  const ok = actual === expected;
  if (!ok) failures += 1;
  console.log(
    `${ok ? "ok  " : "FAIL"} ${name}: ${actual}${ok ? "" : ` (expected ${expected})`}`,
  );
}

// Declared imports are part of the contract. Anything beyond the shared memory the linker flags
// demand would be a deployment surprise, so list them rather than ignore them.
const imports = WebAssembly.Module.imports(new WebAssembly.Module(bytes));
check(
  "imports are only the shared memory",
  imports.length === 1 &&
    imports[0].module === "env" &&
    imports[0].name === "memory" &&
    imports[0].kind === "memory",
  true,
);

// 32-bit usize is the whole reason this script exists.
check("usize bits", e.fv_usize_bits(), 32);

check("envelope roundtrip", e.fv_envelope_roundtrip(), 0);
check("truncated envelope refused", e.fv_truncated_envelope_refused(), 0);
check("envelope length", e.fv_envelope_len(), 29 + 16);

// Deposit arithmetic, including values that only differ on a 32-bit word boundary.
const COIN = 100_000_000n;
const oneZec = COIN;
check("1 ZEC outputs", e.fv_plan_outputs(oneZec), 1);
check("1 ZEC total", e.fv_plan_total(oneZec), oneZec);
check("1.23456789 ZEC outputs", e.fv_plan_outputs(COIN + 23_456_789n), 4);
check("1.23456789 ZEC total", e.fv_plan_total(COIN + 23_456_789n), COIN + 23_000_000n);
check("1.23456789 ZEC residual", e.fv_plan_residual(COIN + 23_456_789n), 456_789n);
check("25,000 ZEC outputs", e.fv_plan_outputs(25_000n * COIN), 3);
check("all outputs canonical", e.fv_plan_all_canonical(25_000n * COIN), 1);

check("canary blocks bulk", e.fv_canary_blocks_bulk(), 1);

if (failures > 0) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
console.log("\nwasm smoke: all checks passed");
