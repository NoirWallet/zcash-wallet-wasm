import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { basename, resolve } from "node:path";

const path = process.argv[2];
if (!path) throw new Error("usage: node tests/check_wasm_imports.mjs <wasm>");
const module = await WebAssembly.compile(await readFile(path));
const descriptors = WebAssembly.Module.imports(module);
const imports = descriptors.map(({ module, name }) => `${module}.${name}`);
const forbidden = imports.filter((name) => /mio|socket|wasi_snapshot_preview1|tokio::net/i.test(name));
if (forbidden.length) throw new Error(`forbidden native imports: ${forbidden.join(", ")}`);
console.log(`checked ${imports.length} imports`);
const memories = descriptors.filter(({kind}) => kind === "memory");
if (memories.length !== 1 || memories[0].module !== `./${basename(path, ".wasm")}.js` || memories[0].name !== "memory") {
  throw new Error("expected the single generated-binding memory import used by wasm-bindgen workers");
}

// The atomics build must use shared memory even before a Rayon pool is started:
// wasm-bindgen's local async executor also uses Atomics.waitAsync.
const bindingsPath = path.replace(/_bg\.wasm$/, ".js");
if (bindingsPath === path) throw new Error("expected a wasm-bindgen *_bg.wasm artifact");
// Web-target worker helpers register a message listener when imported in Node.
globalThis.self ??= new EventTarget();
const bindings = await import(pathToFileURL(resolve(bindingsPath)));
const exports = await bindings.default({ module_or_path: module });
if (!(exports.memory.buffer instanceof SharedArrayBuffer)) {
  throw new Error("atomics build must expose shared WebAssembly memory");
}
console.log("instantiated module with shared WebAssembly memory");
