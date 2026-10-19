import { pbkdf2Sync } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

// Use synthetic test mnemonics only. This wrapper sends the derived seed to the
// selected binary; only that binary can guarantee it remains offline and unlogged.
// Usage: node scripts/diagnostics/run_transparent_replay.mjs <trusted-binary> <public-fixture.json>
async function readMnemonic() {
  if (!process.stdin.isTTY) {
    const rl = createInterface({ input: process.stdin, terminal: false });
    const phrase = await new Promise(resolve => rl.once('line', resolve));
    rl.close();
    return phrase;
  }
  process.stderr.write('Synthetic test mnemonic (hidden input): ');
  process.stdin.setRawMode(true);
  process.stdin.setEncoding('utf8');
  process.stdin.resume();
  return new Promise(resolve => {
    let phrase = '';
    const onData = data => {
      for (const character of data) {
        if (character === '\u0003') {
          process.stdin.setRawMode(false);
          process.exit(130);
        } else if (character === '\r' || character === '\n') {
          process.stdin.off('data', onData);
          process.stdin.setRawMode(false);
          process.stdin.pause();
          process.stderr.write('\n');
          resolve(phrase);
          return;
        } else if (character === '\u007f' || character === '\b') {
          phrase = phrase.slice(0, -1);
        } else phrase += character;
      }
    };
    process.stdin.on('data', onData);
  });
}
if (process.argv.length !== 4) throw Error('Expected binary path and public fixture path');
process.stderr.write('Synthetic test mnemonic only; the selected binary receives its derived seed.\n');
let phrase = await readMnemonic();
try {
  const seed = pbkdf2Sync(phrase.trim().normalize('NFKD'), 'mnemonic', 2048, 64, 'sha512');
  const child = spawn(process.argv[2], [process.argv[3]], { stdio: ['pipe', 'inherit', 'inherit'] });
  const finished = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', code => resolve(code ?? 1));
  });
  child.stdin.end(seed.toString('hex') + '\n');
  seed.fill(0);
  process.exitCode = await finished;
} finally {
  phrase = '';
}
