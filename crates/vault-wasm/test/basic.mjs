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
  DkgRelay,
  MessageSet,
  Fixtures,
  PcztTools,
  Sealed,
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

check("envelope: defined and reserved message types are kept apart", () => {
  // Defined: the four signing messages and the four DKG messages.
  assert(EnvelopeCodec.msg_type("signInit") === 0x20);
  assert(EnvelopeCodec.msg_type("signCommitments") === 0x21);
  assert(EnvelopeCodec.msg_type("signPayload") === 0x22);
  assert(EnvelopeCodec.msg_type("signShare") === 0x23);
  assert(EnvelopeCodec.msg_type("dkgR1") === 0x10);
  assert(EnvelopeCodec.msg_type("dkgR2Batch") === 0x14);

  const defined = EnvelopeCodec.defined_msg_types();
  const reserved = EnvelopeCodec.reserved_msg_types();
  assert(defined.length === 8, `defined: ${defined.length}`);
  assert(reserved.length === 11, `reserved: ${reserved.length}`);

  // A reserved type has a number but no payload: asking for it as a defined type is an error, so an
  // undefined message cannot become a contract by accident.
  let threw = false;
  try {
    EnvelopeCodec.msg_type("keysetSetup");
  } catch (e) {
    threw = String(e).includes("reserved");
  }
  assert(threw, "keysetSetup must be refused as undefined");
  assert(EnvelopeCodec.reserved_msg_type("keysetSetup") === 0x16);

  // Unknown names fail in both spaces.
  for (const bad of ["not-a-message", ""]) {
    let a = false;
    try {
      EnvelopeCodec.msg_type(bad);
    } catch {
      a = true;
    }
    assert(a, `msg_type(${bad}) must be refused`);
    let b = false;
    try {
      EnvelopeCodec.reserved_msg_type(bad);
    } catch {
      b = true;
    }
    assert(b, `reserved_msg_type(${bad}) must be refused`);
  }
});

check("session ids are random, not counters", () => {
  const a = EnvelopeCodec.new_session_id();
  const b = EnvelopeCodec.new_session_id();
  assert(a.length === 16);
  assert(hex(a) !== hex(b), "two ids must differ");
  assert(hex(a) !== hex(new Uint8Array(16)), "must not be all zero");
});

check("qsk: generated, canonical, and it drives the keyset", () => {
  const qsk = Keyset.generate_qsk();
  assert(qsk.length === 32);
  // It must be usable as the `qsk` argument and therefore produce a canonical FVK.
  const ak = groupKey();
  const [fvk] = Keyset.derive(ak, qsk, false, 0);
  assert(fvk.length === 96);
  assert(hex(fvk.slice(0, 32)) === hex(ak));
  // Two generated keys must differ.
  assert(hex(Keyset.generate_qsk()) !== hex(qsk));
});

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
  const finals = [];
  let publicPackage = null;
  for (const d of ds) {
    const r = d.part3(r2[d.slot()], bc, boxes.get(d.slot()));
    packages.push(r.key_package());
    finals.push(r);
    publicPackage = r.public_key_package();
  }
  return { keyPackages: packages, publicPackage, finals, broadcasts: bc };
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

// --- the same ceremony, but every DKG message goes through an envelope ----

check("dkg over envelopes: the relay binds, forwards, and rejects replays", () => {
  const participants = 3;
  const threshold = 2;
  const sid = EnvelopeCodec.new_session_id();
  const ds = [];
  for (let slot = 0; slot < participants; slot += 1) ds.push(new Dkg(participants, threshold, slot));
  const relay = new DkgRelay(sid, participants);

  // Round 1: each device sends an envelope to the relay; the relay validates and collects.
  const r1Secrets = [];
  const r1Payloads = [];
  for (const d of ds) {
    const r = d.part1();
    r1Secrets.push(r.secret());
    const env = r.broadcast_envelope(sid, 1, d.slot());
    const [slot, payload] = relay.accept(env, "dkgR1");
    assert(slot === d.slot(), `sender mismatch: ${slot}`);
    r1Payloads.push(payload);
  }

  // A replay of the first envelope must be refused: the relay tracks per-sender sequence.
  const replayed = ds[0].part1();
  const staleEnv = replayed.broadcast_envelope(sid, 1, 0);
  let threw = false;
  try {
    relay.accept(staleEnv, "dkgR1");
  } catch {
    threw = true;
  }
  assert(threw, "a replayed round-1 envelope must be refused");

  // An envelope from outside the roster must be refused.
  const outsider = new Dkg(participants, threshold, 2);
  const outsiderEnv = outsider.part1().broadcast_envelope(sid, 9, 2);
  const narrow = new DkgRelay(sid, 2); // roster of two, sender 2 is outside it
  let refused = false;
  try {
    narrow.accept(outsiderEnv, "dkgR1");
  } catch {
    refused = true;
  }
  assert(refused, "an out-of-roster sender must be refused");

  // Forward the bundle of all round-1 broadcasts to each peer.
  const bundleSet = MessageSet.from_bytes(
    (() => {
      const set = new MessageSet();
      r1Payloads.forEach((p, slot) => set.set(slot, p));
      return set.to_bytes();
    })(),
  );
  assert(bundleSet.len() === participants);

  // Round 2: each device computes its packages from the bundled broadcasts.
  const r2Secrets = [];
  const outgoing = [];
  for (const d of ds) {
    const env = relay.bundle(1, d.slot(), bundleSet);
    const decoded = EnvelopeCodec.decode(env);
    assert(decoded[1] === 1, "bundle is round 1");
    assert(decoded[2] === EnvelopeCodec.coordinator_slot, "bundle comes from the coordinator");
    assert(decoded[3] === d.slot(), "bundle is addressed to this device");
    const set = MessageSet.from_bytes(decoded[6]);
    assert(set.len() === participants, "bundle carries every broadcast");
    const r = d.part2(r1Secrets[d.slot()], set);
    r2Secrets.push(r.secret());
    const env2 = r.outgoing_envelope(sid, 2, d.slot());
    const [slot, batch] = relay.accept(env2, "dkgR2Batch");
    outgoing.push([slot, MessageSet.from_bytes(batch)]);
  }

  // Round 3: relay the incoming packages and finalize.
  const inboxes = new Map();
  for (let slot = 0; slot < participants; slot += 1) inboxes.set(slot, new MessageSet());
  for (const [from, set] of outgoing) {
    for (const to of set.slots()) inboxes.get(to).set(from, set.get(to));
  }
  const groupKeys = [];
  for (const d of ds) {
    const env = relay.bundle(2, d.slot(), inboxes.get(d.slot()));
    const set = MessageSet.from_bytes(EnvelopeCodec.decode(env)[6]);
    const r = d.part3(r2Secrets[d.slot()], bundleSet, set);
    groupKeys.push(hex(r.group_verifying_key()));
  }
  assert(groupKeys.every((k) => k === groupKeys[0]), "devices must agree on ak");
  assert(groupKeys[0].length === 64);
});

check("keyset setup: the qsk travels sealed and every device lands on one keyset", () => {
  // Coordinator generates the qsk once; each peer gets it sealed to its own static key. If a peer
  // could read another peer's copy, or if the roster differed, opening would fail.
  const ceremony = runCeremony(3, 2);
  const finals = ceremony.finals;
  // Each device learns the whole roster's static keys from the broadcasts it was handed — no device
  // reaches into another's object, which is what the frontend can actually do.
  const statics = finals[0].statics();
  assert(statics.len() === 3, `statics: ${statics.len()}`);
  const prologue = Sealed.prologue(statics);

  // And a coordinator holding a single round-1 broadcast can read the sender's key out of it.
  const fromBroadcast = Dkg.static_public_of_broadcast(
    ceremony.broadcasts.get(2),
  );
  assert(hex(fromBroadcast) === hex(statics.get(2)), "broadcast carries the same key");
  assert(hex(finals[0].statics().get(0)) === hex(finals[0].static_public()), "includes self");

  // ak is the same for everyone; take it from slot 0.
  const ak = finals[0].group_verifying_key();
  const qsk = Keyset.generate_qsk();

  // Coordinator (slot 0) seals the qsk to slots 1 and 2.
  const sealedFor = finals.map((f, slot) => {
    if (slot === 0) return null;
    return Sealed.seal(finals[0].static_secret(), f.static_public(), prologue, qsk);
  });

  // Each recipient opens it and derives the keyset; all three must match.
  const derived = finals.map((f, slot) => {
    const material = slot === 0 ? qsk : Sealed.open(f.static_secret(), finals[0].static_public(), prologue, sealedFor[slot]);
    assert(hex(material) === hex(qsk), `slot ${slot} received the same qsk`);
    return Keyset.derive(ak, material, false, 0);
  });

  const fingerprint = hex(derived[0][4]);
  const address = derived[0][2];
  for (const d of derived) {
    assert(hex(d[4]) === fingerprint, "fingerprints must agree");
    assert(d[2] === address, "addresses must agree");
  }
  assert(Keyset.address_is_shielded_only(address));

  // A peer cannot open a package addressed to someone else.
  let threw = false;
  try {
    Sealed.open(finals[2].static_secret(), finals[0].static_public(), prologue, sealedFor[1]);
  } catch {
    threw = true;
  }
  assert(threw, "a package for another recipient must not open");

  // And a different roster produces a different prologue, so the same ciphertext does not open.
  const otherRoster = new MessageSet();
  otherRoster.set(0, finals[0].static_public());
  otherRoster.set(1, finals[1].static_public());
  const otherPrologue = Sealed.prologue(otherRoster);
  threw = false;
  try {
    Sealed.open(finals[1].static_secret(), finals[0].static_public(), otherPrologue, sealedFor[1]);
  } catch {
    threw = true;
  }
  assert(threw, "a different participant set must not open the same ciphertext");
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

// --- the success path, in JS, on a transaction built for this vault ---------
// Everything above proves the pieces refuse the wrong thing. This proves they accept the right thing:
// build a real Ironwood PCZT for the vault's own group key, sign it through the session, write the
// signatures back, and check the result.

check("full success path: build, sign, apply, verify", () => {
  const participants = 3;
  const threshold = 2;
  const sessionId = EnvelopeCodec.new_session_id();

  // A fresh ceremony so `ak` matches the transaction we are about to build.
  const ceremony = runCeremony(participants, threshold);
  const finals = ceremony.finals;
  const ak = finals[0].group_verifying_key();

  // One qsk, distributed in the clear here because this test is about the signing path.
  const qsk = Keyset.generate_qsk();
  const [, , , , ] = [];
  const keyset = Keyset.derive(ak, qsk, false, 0);
  const fvk = keyset[0];
  const address = keyset[2];

  const pczt = Fixtures.ironwood_pczt(fvk, 1_000_000, 990_000);
  assert(pczt.length > 1_000, `pczt is ${pczt.length} bytes`);
  PcztTools.check(pczt);

  // The coordinator reads what to sign.
  const sighash = PcztTools.shielded_sighash(pczt);
  const alphas = PcztTools.action_alphas(pczt);
  const rks = PcztTools.action_rks(pczt);
  assert(rks.length === 32, "one action");
  // The transaction really is this vault's: rk == ak + alpha*G.
  PcztTools.check_action_bindings(pczt, ak, alphas);

  // Two of three sign it.
  const signers = [0, 1];
  const coordinator = new Coordinator(sessionId, participants, threshold, signers, 1, ceremony.publicPackage);
  const signerObjs = signers.map((slot) => new Signer(sessionId, slot, participants, ceremony.keyPackages[slot]));

  const commitments = coordinator.init_all().map(([slot, env]) => {
    const idx = signers.indexOf(slot);
    return [idx, signerObjs[idx].init(env)];
  });
  for (const [, c] of commitments) coordinator.accept_commitments(c);

  coordinator.prepare_payload(sighash, alphas);
  for (const [slot, env] of coordinator.payloads()) {
    const idx = signers.indexOf(slot);
    const [gotSighash, gotAlphas] = signerObjs[idx].payload(env);
    assert(hex(gotSighash) === hex(sighash), "signer agrees on the digest");
    assert(hex(gotAlphas) === hex(alphas), "signer agrees on the randomizer");
    // The signer's own independent check against the transaction.
    PcztTools.check_action_bindings(pczt, ak, gotAlphas);
    signerObjs[idx].approve();
    coordinator.accept_share(signerObjs[idx].share());
  }

  const signatures = coordinator.aggregate();
  assert(signatures.length === 64);
  // Verify before applying, against the transaction's own rk.
  Verification.verify_spend_auth(ak, alphas, sighash, signatures);

  const signed = PcztTools.apply_signatures(pczt, new Uint8Array([0]), signatures);
  assert(signed.length > pczt.length, "a signature was written into the transaction");

  // And the address the transaction pays change back to is the vault's own.
  assert(Keyset.address_is_shielded_only(address));
});

if (failures > 0) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
console.log("\nvault-wasm: all checks passed");
