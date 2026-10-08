// Independent golden: protoc encodes the server schema; Node hashes tag || protobuf.
// Run with PROTOC=/path/to/protoc node generate_delegation_golden.cjs.
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const fixture = JSON.parse(fs.readFileSync(path.join(__dirname, 'delegation_tx1_effects_v1.json')));
const b64 = (b) => Buffer.from(b).toString('base64');
const wire = {
  rk: fixture.rk,
  spend_auth_sig: fixture.spend_auth_sig,
  tx1_effects: fixture.tx1_effects,
  signed_note_nullifier: fixture.signed_note_nullifier,
  cmx_new: fixture.cmx_new,
  van_cmx: b64(Buffer.alloc(32, 4)),
  gov_nullifiers: Array.from({ length: 5 }, (_, i) => b64(Buffer.alloc(32, i + 5))),
  proof: b64([1, 2, 3]),
  vote_round_id: b64(Buffer.alloc(32, 1)),
};
const escape = (s) => [...Buffer.from(s, 'base64')].map((b) => '\\' + b.toString(8).padStart(3, '0')).join('');
const text = Object.entries(wire).flatMap(([key, value]) => (Array.isArray(value) ? value : [value]).map((v) => `${key}: "${escape(v)}"`)).join('\n');
const encoded = spawnSync(process.env.PROTOC || 'protoc', ['--encode=svote.v1.MsgDelegateVote', '-I', __dirname, path.join(__dirname, 'delegation_vote_sdk.proto')], { input: text });
if (encoded.status !== 0) throw new Error(encoded.stderr.toString());
const tagged = Buffer.concat([Buffer.from([2]), encoded.stdout]);
fs.writeFileSync(path.join(__dirname, 'delegation_submission_golden.json'), JSON.stringify({
  authority: 'vote-sdk 990fb1c3fbdbda9f4f459a5259551580c157c5dd: api/codec.go, api/handler.go, proto/svote/v1/tx.proto',
  note: 'Public synthetic codec fixture; not a valid delegation proof.',
  canonical_json: JSON.stringify(wire),
  transaction_hex: tagged.toString('hex'),
  transaction_hash: createHash('sha256').update(tagged).digest('hex').toUpperCase(),
}, null, 2) + '\n');
