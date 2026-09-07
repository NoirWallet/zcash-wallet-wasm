import { readFile } from "node:fs/promises";

const path = process.argv[2];
if (!path) throw new Error("usage: node tests/check_wasm_imports.mjs <wasm>");
const module = await WebAssembly.compile(await readFile(path));
const imports = WebAssembly.Module.imports(module).map(({ module, name }) => `${module}.${name}`);
const forbidden = imports.filter((name) => /mio|socket|wasi_snapshot_preview1|tokio::net/i.test(name));
if (forbidden.length) throw new Error(`forbidden native imports: ${forbidden.join(", ")}`);
console.log(`checked ${imports.length} imports`);
