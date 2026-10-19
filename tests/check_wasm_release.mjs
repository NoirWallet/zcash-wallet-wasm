import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

const packageDir = process.argv[2];
if (!packageDir) {
  throw new Error('usage: node tests/check_wasm_release.mjs <package-directory>');
}

// The generated web bindings register worker message handlers during import.
globalThis.self ??= new EventTarget();
const bindings = await import(pathToFileURL(resolve(packageDir, 'zcash_wallet_wasm.js')));

for (const name of ['WebWallet', 'WebVotingSession', 'initThreadPool']) {
  if (!(name in bindings)) throw new Error(`release package is missing ${name}`);
}

const testExports = Object.keys(bindings).filter((name) => name.startsWith('votingTest'));
if (testExports.length) {
  throw new Error(`test-only exports found in release package: ${testExports.join(', ')}`);
}

console.log('release exports checked; no voting test fixtures found');
