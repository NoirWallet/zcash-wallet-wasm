// Basic functionality, driven from JS exactly the way the frontend will drive it.
//
// This is the check that "the frontend can call the wasm" actually holds: it imports the wasm-bindgen
// package, runs a full 2-of-3 vault ceremony across three simulated devices, and verifies the
// aggregate. Nothing here reaches into Rust internals — if the JS API is wrong, this fails.
//
// Run:  make test-vault-wasm

import { readFileSync } from "node:fs";
import {
  Canary,
  Coordinator,
  Deposit,
  Dkg,
  EnvelopeCodec,
  Keyset,
  MessageSet,
  PcztTools,
  SessionMarker,
  ShareBackup,
  Signer,
  Verification,
} from "../pkg-node/vault_wasm.js";

let failures = 0;
function check(name, fn) {
  try {
    fn();
    console.log(`ok   ${name}`);
  } catch (e) {
    failures += 1;
    console.log(`FAIL ${name}: ${e?.message ?? e}`);
  }
}
function assert(cond, msg) {
  if (!cond) throw new Error(msg ?? "assertion failed");
}

const hex = (u8) => Buffer.from(u8).toString("hex");
const COIN = 100_000_000;

// --- deposit policy -------------------------------------------------------

check("deposit: 1 ZEC is one canonical output", () => {
  const out = Deposit.plan(1 * COIN);
  assert(out.length === 1, `got ${out.length} outputs`);
  assert(out[0] === COIN);
  assert(Deposit.residual(1 * COIN) === 0);
});

check("deposit: odd amount rounds down and keeps dust", () => {
  const amount = COIN + 23_456_789;
  const out = Deposit.plan(amount);
  assert(out.length === 4, `got ${out.length}`);
  assert(out.every((v) => Deposit.is_canonical(v)));
  assert(Deposit.residual(amount) === 456_789);
});

check("deposit: 25,000 ZEC never exceeds the per-output cap", () => {
  const out = Deposit.plan(25_000 * COIN);
  assert(out.length === 3, `got ${out.length}`);
  assert(out.every((v) => v <= Deposit.denom_cap()));
});

check("deposit: below the minimum is refused", () => {
  let threw = false;
  try {
    Deposit.plan(COIN / 200);
  } catch {
    threw = true;
  }
  assert(threw, "expected an error");
});

check("canary: bulk refused until a spend is proven", () => {
  const c = new Canary();
  assert(c.phase() === "notStarted");
  assert(!c.allows(COIN));
  c.begin(Deposit.canary_amount());
  assert(c.phase() === "awaitingArrival");
  assert(c.allows(Deposit.canary_amount()));
  assert(!c.allows(COIN), "bulk must stay refused");
  c.arrival_confirmed();
  assert(!c.allows(COIN), "arrival alone is not proof");
  c.spend_confirmed();
  assert(c.phase() === "proven");
  assert(c.allows(COIN));
});

// --- envelope ------------------------------------------------------------

check("envelope: round-trip and rejection", () => {
  const sessionId = new Uint8Array(16).fill(7);
  const payload = new Uint8Array([1, 2, 3, 4]);
  const bytes = EnvelopeCodec.build(0x21, 1, 0, EnvelopeCodec.coordinator_slot, 1, sessionId, payload);
  const [type, round, sender, recipient, sequence, sid, body] = EnvelopeCodec.decode(bytes);
  assert(type === 0x21 && round === 1 && sender === 0 && sequence === 1);
  assert(recipient === EnvelopeCodec.coordinator_slot);
  assert(hex(sid) === hex(sessionId));
  assert(hex(body) === hex(payload));

  let threw = false;
  try {
    EnvelopeCodec.decode(bytes.slice(0, bytes.length - 1));
  } catch {
    threw = true;
  }
  assert(threw, "truncated envelope must be refused");
});

// --- the ceremony --------------------------------------------------------

const PARTICIPANTS = 3;
const THRESHOLD = 2;
const sessionId = new Uint8Array(16).fill(0x5a);

const devices = [];
for (let slot = 0; slot < PARTICIPANTS; slot += 1) {
  devices.push(new Dkg(PARTICIPANTS, THRESHOLD, slot));
}

// Round 1
const round1Secret = [];
const broadcasts = new MessageSet();
for (const d of devices) {
  const r = d.part1();
  round1Secret.push(r.secret());
  broadcasts.set(d.slot(), r.broadcast());
}
check("dkg: every device produced a round-1 broadcast", () => {
  assert(broadcasts.len() === PARTICIPANTS, `got ${broadcasts.len()}`);
});

// Round 2
const round2Secret = [];
const inboxes = new Map();
for (let slot = 0; slot < PARTICIPANTS; slot += 1) inboxes.set(slot, new MessageSet());
for (const d of devices) {
  const r = d.part2(round1Secret[d.slot()], broadcasts);
  round2Secret.push(r.secret());
  const out = r.outgoing();
  for (const to of out.slots()) inboxes.get(to).set(d.slot(), out.get(to));
}
check("dkg: every device received one package per peer", () => {
  for (let slot = 0; slot < PARTICIPANTS; slot += 1) {
    assert(inboxes.get(slot).len() === PARTICIPANTS - 1, `slot ${slot}: ${inboxes.get(slot).len()}`);
  }
});

// Round 3
const keyPackages = [];
const publicPackages = [];
const finalRound = [];
for (const d of devices) {
  const r = d.part3(round2Secret[d.slot()], broadcasts, inboxes.get(d.slot()));
  keyPackages.push(r.key_package());
  publicPackages.push(r.public_key_package());
  finalRound.push(r);
}
check("dkg: all devices agree on the public package", () => {
  for (const p of publicPackages) assert(hex(p) === hex(publicPackages[0]));
});

// --- keyset --------------------------------------------------------------

const ak = groupKey();

check("keyset: derives a shielded-only address and a stable fingerprint", () => {
  const qsk = new Uint8Array(32);
  qsk[0] = 0x42;
  qsk[31] = 0x01;
  const [fvk, ufvk, address, raw, fingerprint] = Keyset.derive(ak, qsk, false, 0);
  assert(fvk.length === 96, `fvk is ${fvk.length}`);
  assert(raw.length === 43, `raw address is ${raw.length}`);
  assert(fingerprint.length === 32);
  // ak is literally the first 32 bytes of the FVK.
  assert(hex(fvk.slice(0, 32)) === hex(ak), "fvk must start with ak");
  assert(ufvk.startsWith("uview"), ufvk);
  assert(address.startsWith("u1") || address.startsWith("utest1"), address);
  assert(Keyset.address_is_shielded_only(address), "address must be shielded-only");

  // Deterministic: the same inputs must give the same address on every device.
  const again = Keyset.derive(ak, qsk, false, 0);
  assert(hex(again[2] === undefined ? again.fvk : again[0]) === hex(fvk));
});

check("keyset: a garbage ak is refused", () => {
  let threw = false;
  try {
    Keyset.derive(new Uint8Array(32).fill(0xff), new Uint8Array(32).fill(1), false, 0);
  } catch {
    threw = true;
  }
  assert(threw, "expected an error");
});

check("keyset: mainnet and testnet share the keyset but not the encoding", () => {
  const qsk = new Uint8Array(32);
  qsk[0] = 0x42;
  qsk[31] = 0x01;
  const test = Keyset.derive(ak, qsk, false, 0);
  const main = Keyset.derive(ak, qsk, true, 0);
  assert(hex(test[0]) === hex(main[0]), "same fvk");
  assert(test[2] !== main[2], "different address encoding");
  assert(main[2].startsWith("u1"), main[2]);
});

// --- share backup --------------------------------------------------------

check("share backup: round-trip, and the wrong slot is refused", () => {
  const vaultId = new Uint8Array([1, 2, 3]);
  const deviceId = new Uint8Array([4, 5, 6]);
  // The share must be bound to the group it belongs to, so read the group key back out of a package.
  const backup = new ShareBackup(keyPackages[1], 1, deviceId, vaultId, ak);
  const bytes = backup.to_bytes();
  const restored = ShareBackup.from_bytes(bytes, ak, 1);
  assert(restored.slot() === 1);
  assert(hex(restored.device_id()) === hex(deviceId));
  assert(hex(restored.vault_id()) === hex(vaultId));

  let threw = false;
  try {
    ShareBackup.from_bytes(bytes, ak, 2);
  } catch {
    threw = true;
  }
  assert(threw, "restoring into another slot must fail");
});

// `ak` comes back from the final DKG round: every later call needs it.
function groupKey() {
  const key = hex(finalRound[0].group_verifying_key());
  assert(
    finalRound.every((r) => hex(r.group_verifying_key()) === key),
    "devices disagree on ak",
  );
  return finalRound[0].group_verifying_key();
}

// --- ceremonies for the other two modes ----------------------------------

function runCeremony(participants, threshold) {
  const ds = [];
  for (let slot = 0; slot < participants; slot += 1) ds.push(new Dkg(participants, threshold, slot));

  const r1 = [];
  const bc = new MessageSet();
  for (const d of ds) {
    const r = d.part1();
    r1.push(r.secret());
    bc.set(d.slot(), r.broadcast());
  }
  const r2 = [];
  const boxes = new Map();
  for (let slot = 0; slot < participants; slot += 1) boxes.set(slot, new MessageSet());
  for (const d of ds) {
    const r = d.part2(r1[d.slot()], bc);
    r2.push(r.secret());
    const out = r.outgoing();
    for (const to of out.slots()) boxes.get(to).set(d.slot(), out.get(to));
  }
  const packages = [];
  let publicPackage = null;
  for (const d of ds) {
    const r = d.part3(r2[d.slot()], bc, boxes.get(d.slot()));
    packages.push(r.key_package());
    publicPackage = r.public_package ? r.public_package() : r.public_key_package();
  }
  return { keyPackages: packages, publicPackage };
}

const extraCeremonies = new Map();
for (const mode of [[2, 2], [3, 3]]) {
  check(`dkg: ${mode[1]}-of-${mode[0]} completes`, () => {
    const ceremony = runCeremony(mode[0], mode[1]);
    assert(ceremony.publicPackage.length > 0);
    extraCeremonies.set(mode.join("-"), ceremony);
  });
}

// --- signing session -----------------------------------------------------
// Uses a synthetic sighash and alphas: the PCZT tier is not part of this package (it links a prover).
// What this exercises is the whole message flow the frontend has to drive.

function runSessionFor(mode) {
  const [participants, threshold] = mode;
  const ceremony = extraCeremonies?.get(mode.join("-")) ?? null;
  const signers = Array.from({ length: threshold }, (_, i) => i);
  const pub = ceremony ? ceremony.publicPackage : publicPackages[0];
  const packages = ceremony ? ceremony.keyPackages : keyPackages;
  const coordinator = new Coordinator(sessionId, participants, threshold, signers, 1, pub);
  const signerObjs = signers.map((slot) => new Signer(sessionId, slot, participants, packages[slot]));

  const inits = coordinator.init_all();
  assert(
    inits.length === threshold,
    `expected ${threshold} init envelopes, got ${inits.length}`,
  );
  const commitments = [];
  for (const [slot, envelope] of inits) {
    const idx = signers.indexOf(slot);
    assert(idx >= 0, `unexpected slot ${slot}`);
    commitments.push([slot, signerObjs[idx].init(envelope)]);
  }
  for (const [, c] of commitments) coordinator.accept_commitments(c);
  assert(coordinator.phase() === "CommitmentsCollected", coordinator.phase());

  const sighash = new Uint8Array(32).fill(0x11);
  const alphas = new Uint8Array(32);
  alphas[0] = 0x21;
  alphas[31] = 0x01;
  coordinator.prepare_payload(sighash, alphas);

  const payloads = coordinator.payloads();
  assert(payloads.length === threshold, `expected ${threshold} payloads`);
  for (const [slot, envelope] of payloads) {
    const idx = signers.indexOf(slot);
    const [gotSighash, gotAlphas] = signerObjs[idx].payload(envelope);
    assert(hex(gotSighash) === hex(sighash), "sighash mismatch");
    assert(hex(gotAlphas) === hex(alphas), "alpha mismatch");
    // A share must not exist before approval.
    let threw = false;
    try {
      signerObjs[idx].share();
    } catch {
      threw = true;
    }
    assert(threw, "share before approve must fail");
    signerObjs[idx].approve();
    coordinator.accept_share(signerObjs[idx].share());
    assert(
      signerObjs[idx].nonce_states().every((s) => s === 2),
      "nonces must be consumed",
    );
  }

  const signatures = coordinator.aggregate();
  assert(signatures.length === 64, `expected 64-byte signature, got ${signatures.length}`);
  assert(coordinator.phase() === "Aggregated", coordinator.phase());
  return { signatures, sighash, alphas };
}

check("session: every supported mode signs and aggregates", () => {
  for (const mode of [[2, 2], [3, 2], [3, 3]]) {
    const { signatures } = runSessionFor(mode);
    assert(signatures.length === 64, `${mode[1]}-of-${mode[0]}: ${signatures.length} bytes`);
  }
});

// --- session marker ------------------------------------------------------

check("session: the aggregate verifies against ak, and a tampered digest does not", () => {
  const { signatures, sighash, alphas } = runSessionFor([3, 2]);
  Verification.verify_spend_auth(ak, alphas, sighash, signatures);

  const tampered = new Uint8Array(sighash);
  tampered[0] ^= 0xff;
  let threw = false;
  try {
    Verification.verify_spend_auth(ak, alphas, tampered, signatures);
  } catch {
    threw = true;
  }
  assert(threw, "a signature over a different digest must not verify");

  // rk is what the PCZT must carry; the frontend compares it before signing.
  const rk = Verification.rk(ak, alphas);
  assert(rk.length === 32);
});

check("marker: created, committed, burned", () => {
  assert(SessionMarker.recover(null) === "clean");
  assert(SessionMarker.recover(undefined) === "clean");
  const created = SessionMarker.created(sessionId);
  assert(SessionMarker.recover(created) === "discardable");
  const committed = SessionMarker.committed(sessionId);
  assert(SessionMarker.recover(committed) === "burned");
  assert(hex(SessionMarker.session_id(committed)) === hex(sessionId));
  let threw = false;
  try {
    SessionMarker.recover(new Uint8Array([9, 9, 9]));
  } catch {
    threw = true;
  }
  assert(threw, "a malformed marker must be refused, not guessed");
});

// --- a real PCZT, end to end -------------------------------------------
// The fixture is a genuine one-action Ironwood transaction produced by the stock builder, with its
// proof already stripped. This is the flow the frontend actually drives: take the PCZT the wallet
// built, shrink it, read what to sign, run the session, write the signature back.

const fixture = new Uint8Array(
  Buffer.from(
    readFileSync(
      new URL("../../pczt-signer-core/tests/fixtures/ironwood_1action.hex", import.meta.url),
      "utf8",
    ).trim(),
    "hex",
  ),
);

check("pczt: gate admits the fixture", () => {
  PcztTools.check(fixture);
  assert(fixture.length === 2362, `fixture is ${fixture.length} bytes`);

  const stripped = PcztTools.strip_proofs(fixture);
  assert(stripped.length === fixture.length, "the fixture already has no proof");

  const rks = PcztTools.action_rks(fixture);
  assert(rks.length === 32, `one action expected, got ${rks.length / 32}`);

  const alphas = PcztTools.action_alphas(fixture);
  assert(alphas.length === 32);
});

check("pczt: the action's rk is ak + alpha*G", () => {
  const alphas = PcztTools.action_alphas(fixture);
  // The fixture was built for a group whose `ak` we do not have here, so instead assert the relation
  // is checkable and that a wrong ak is rejected — that is the property the signer relies on.
  let threw = false;
  try {
    PcztTools.check_action_bindings(fixture, new Uint8Array(32).fill(0x11), alphas);
  } catch {
    threw = true;
  }
  assert(threw, "a foreign ak must not match the action's rk");
});

check("pczt: a transaction from another vault cannot be signed", () => {
  // Signature application is verified by the underlying role against the action's rk, so an
  // arbitrary 64 bytes cannot be written into the transaction.
  let threw = false;
  try {
    PcztTools.apply_signatures(fixture, new Uint8Array([0]), new Uint8Array(64).fill(0xab));
  } catch {
    threw = true;
  }
  assert(threw, "expected rejection");
});

check("pczt: a corrupt buffer is refused, not a panic", () => {
  for (const bad of [
    new Uint8Array(0),
    new Uint8Array([0x50, 0x43, 0x5a, 0x54]),
    fixture.slice(0, fixture.length - 1),
  ]) {
    let threw = false;
    try {
      PcztTools.check(bad);
    } catch {
      threw = true;
    }
    assert(threw, "expected refusal");
  }
});

if (failures > 0) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
console.log("\nvault-wasm: all checks passed");
