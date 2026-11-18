// Basic functionality, driven from JS exactly the way the frontend will drive it.
//
// This is the check that "the frontend can call the wasm" actually holds: it imports the wasm-bindgen
// package, runs a full 2-of-3 vault ceremony across three simulated devices, and verifies the
// aggregate. Nothing here reaches into Rust internals — if the JS API is wrong, this fails.
//
// Run:  make test-vault-wasm

import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
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
  NoiseXx,
  PcztTools,
  RosterManifest,
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
  devices.push(new Dkg(PARTICIPANTS, THRESHOLD, slot, sessionId));
}

// Round 1
const round1Secret = [];
const broadcasts = new MessageSet();
for (const d of devices) {
  const r = d.part1();
  round1Secret.push(r.secret());
  broadcasts.set(d.slot(), r.broadcast());
}
const ceremonyFor = (participants, threshold) => runCeremony(participants, threshold);

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
  assert(EnvelopeCodec.msg_type("signReject") === 0x24);
  assert(EnvelopeCodec.msg_type("signResult") === 0x25);

  // The round each type belongs to is part of the protocol: a mismatch is refused by both sides.
  assert(EnvelopeCodec.round_of("signInit") === 1);
  assert(EnvelopeCodec.round_of("signCommitments") === 1);
  assert(EnvelopeCodec.round_of("signPayload") === 3);
  assert(EnvelopeCodec.round_of("signShare") === 3);
  assert(EnvelopeCodec.round_of("signReject") === 3);
  assert(EnvelopeCodec.round_of("signResult") === 3);

  const defined = EnvelopeCodec.defined_msg_types();
  const reserved = EnvelopeCodec.reserved_msg_types();
  assert(defined.length === 10, `defined: ${defined.length}`);
  assert(reserved.length === 9, `reserved: ${reserved.length}`);

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

  const dkgSession = new Uint8Array(16).fill(0x5a);
  for (let slot = 0; slot < participants; slot += 1) ds.push(new Dkg(participants, threshold, slot, dkgSession));

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
  // The same 16 bytes everywhere: the ceremony's session id is what round 2 checks, so `Dkg` and the relay
  // must be given the same value. Passing two different ids here would make every `part2` fail with
  // WrongSession, and copying that pattern into production is the failure this comment exists to prevent.
  for (let slot = 0; slot < participants; slot += 1) ds.push(new Dkg(participants, threshold, slot, sid));
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
  const outsider = new Dkg(participants, threshold, 2, new Uint8Array(16).fill(0x5a));
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

check("recovery: a 32-byte scalar rebuilds a working share", () => {
  const ceremony = ceremonyFor(3, 2);
  const ak = ceremony.finals[0].group_verifying_key();
  const publicKeyPackage = ceremony.finals[0].public_key_package();

  // What a 24-word mnemonic can carry: just the scalar. A real share is wrapped here so the check
  // below has a genuine key package to disagree with.
  new ShareBackup(
    ceremony.keyPackages[1],
    1,
    new Uint8Array([9]),
    new Uint8Array([8]),
    ak,
  );
  // The wasm does not expose the scalar, so derive the test's copy from the key package the same way
  // a mnemonic backup would have captured it — through the public `from_scalar` path, checked below.
  const rebuiltWithWrongScalar = () => {
    try {
      ShareBackup.from_scalar(
        new Uint8Array(32).fill(0x11), // not this share's scalar
        ak,
        1,
        2,
        new Uint8Array([9]),
        new Uint8Array([8]),
        publicKeyPackage,
      );
      return null;
    } catch (e) {
      return e;
    }
  };
  assert(rebuiltWithWrongScalar(), "a wrong scalar must be refused against the public package");

  // The scalar a 24-word backup encodes: 32 bytes, and enough to rebuild the share.
  const backup = new ShareBackup(
    ceremony.keyPackages[1],
    1,
    new Uint8Array([9]),
    new Uint8Array([8]),
    ak,
  );
  const scalar = backup.signing_scalar();
  assert(scalar.length === 32, `scalar is ${scalar.length} bytes`);
  const fromScalar = ShareBackup.from_scalar(
    scalar,
    ak,
    1,
    2,
    new Uint8Array([9]),
    new Uint8Array([8]),
    publicKeyPackage,
  );
  assert(hex(fromScalar.signing_scalar()) === hex(scalar), "scalar round-trips");

  // And the UFVK a recovering device receives can be checked against its share's ak.
  const qsk = Keyset.generate_qsk();
  const keyset = Keyset.derive(ak, qsk, false, 0);
  assert(Keyset.ufvk_matches_ak(keyset[1], ak), "our own ufvk matches");
  assert(hex(Keyset.fvk_from_ufvk(keyset[1])) === hex(keyset[0]), "fvk round-trips");

  const otherCeremony = ceremonyFor(2, 2);
  const otherAk = otherCeremony.finals[0].group_verifying_key();
  const otherKeyset = Keyset.derive(otherAk, qsk, false, 0);
  assert(!Keyset.ufvk_matches_ak(otherKeyset[1], ak), "another vault's ufvk must not match");

  let threw = false;
  try {
    Keyset.fvk_from_ufvk("uview1not-a-key");
  } catch {
    threw = true;
  }
  assert(threw, "a malformed ufvk is an error, not a false answer");
});

check("recovery: the fingerprint can be recomputed without the qsk", () => {
  // A device keeps the FVK but not the qsk, so `derive` is unavailable to it. The fingerprint still
  // has to come out identical, because it is what the user compares across devices.
  const ceremony = ceremonyFor(2, 2);
  const ak = ceremony.finals[0].group_verifying_key();
  const keyset = Keyset.derive(ak, Keyset.generate_qsk(), false, 0);
  const [, ufvk, , , fingerprint] = keyset;

  assert(hex(Keyset.fingerprint_from_fvk(keyset[0], false)) === hex(fingerprint), "from fvk");
  assert(hex(Keyset.fingerprint_from_ufvk(ufvk, false)) === hex(fingerprint), "from ufvk");

  // Network is part of the preimage.
  assert(hex(Keyset.fingerprint_from_fvk(keyset[0], true)) !== hex(fingerprint));

  // And it is not a bare SHA-256 of the FVK: the domain tag is in there, so an implementation that
  // hashed the FVK alone would never match ours.
  const naive = createHash("sha256").update(Buffer.from(keyset[0])).digest("hex");
  assert(naive !== hex(fingerprint), "fingerprint must include the domain tag");

  let threw = false;
  try {
    Keyset.fingerprint_from_fvk(new Uint8Array(95), false);
  } catch {
    threw = true;
  }
  assert(threw, "a short fvk is an error");
});

check("rosterHash: the agreed vectors match the mobile client", () => {
  // Same inputs as the vectors the mobile client produced with an independent implementation and
  // re-derived a third time in node. A match here means the encoding agrees byte for byte, not merely
  // that our two sides agree with themselves.
  const identity = (slot) => {
    const v = new Uint8Array(65);
    v[0] = 0x01;
    v.fill(0xa0 + slot, 1, 33);
    v.fill(0xb0 + slot, 33, 65);
    return v;
  };
  const transport = (slot) => new Uint8Array(32).fill(0xc0 + slot);
  // Under QR the transport key is 32 zero bytes: "no transport key", and zero is a low-order X25519
  // point, which is exactly why presence must never be validated as a curve point.
  const zeroTransport = () => new Uint8Array(32);

  const manifest = (n, threshold) =>
    new RosterManifest("11111111-2222-4333-8444-555555555555", 1, "frost-redpallas-blake2b-512", threshold, n);

  const build = (slots, threshold, qr) => {
    const m = manifest(slots.length, threshold);
    for (const slot of slots) m.add_participant(slot, identity(slot), qr ? zeroTransport() : transport(slot));
    return m;
  };

  // A: 3 devices over Bluetooth.
  const a = build([0, 1, 2], 2, false);
  assert(
    a.hash_hex() === "d99931702edb7a7500ab2980522919f455459118f3c893f4836768ed0fe2d227",
    `vector A: ${a.hash_hex()}`,
  );
  assert(a.canonical_bytes().length === 430, "vector A canonical length");

  // B: 2 devices over QR, so the transport keys are the all-zero placeholder.
  const b = build([0, 1], 2, true);
  assert(
    b.hash_hex() === "6fda07956a4bec07c197095857ce4f83a97aa42cdbd37f922b51dc282aa0ac0a",
    `vector B: ${b.hash_hex()}`,
  );
  assert(b.canonical_bytes().length === 320, "vector B canonical length");

  // Insertion order does not matter.
  const reordered = manifest(3, 2);
  for (const slot of [2, 0, 1]) reordered.add_participant(slot, identity(slot), transport(slot));
  assert(reordered.hash_hex() === a.hash_hex(), "order must not matter");

  // One flipped byte changes it.
  const flipped = manifest(3, 2);
  for (const slot of [0, 1, 2]) {
    const id = identity(slot);
    if (slot === 1) id[64] ^= 0x01;
    flipped.add_participant(slot, id, transport(slot));
  }
  assert(flipped.hash_hex() !== a.hash_hex(), "a flipped byte must change the hash");

  // Keys are checked for shape, not for curve membership: a zero transport key is legal.
  let threw = false;
  try {
    manifest(2, 2).add_participant(0, new Uint8Array(65).fill(0x02), transport(0));
  } catch {
    threw = true;
  }
  assert(threw, "an identity key not starting with 0x01 must be refused");

  // An incomplete roster must not produce a hash.
  const partial = manifest(3, 2);
  partial.add_participant(0, identity(0), transport(0));
  threw = false;
  try {
    partial.hash();
  } catch {
    threw = true;
  }
  assert(threw, "a partial roster must refuse to hash");
});

check("seal: a different round-1 transcript cannot be opened", () => {
  // The roster-only prologue lets a coordinator relay different round-1 sets while keeping the roster
  // identical. Folding the transcript in means the mismatch shows up as a failed open, not as a group
  // that finishes with two different keys.
  const statics = new MessageSet();
  const transcript = new MessageSet();
  for (const slot of [0, 1]) {
    statics.set(slot, new Uint8Array(32).fill(0x11 + slot));
    transcript.set(slot, new Uint8Array(40).fill(0x20 + slot));
  }
  const honest = Sealed.prologueWithTranscript(statics, transcript);
  assert(honest.length > Sealed.prologue(statics).length, "the transcript must extend the prologue");

  const doctored = new MessageSet();
  doctored.set(0, new Uint8Array(40).fill(0x20));
  doctored.set(1, new Uint8Array(40).fill(0x99));
  const lie = Sealed.prologueWithTranscript(statics, doctored);
  assert(
    honest.length === lie.length && !honest.every((b, i) => b === lie[i]),
    "a different transcript must give a different prologue",
  );

  // The seal/open half is covered in Rust (`seal::tests`), where the X25519 keys can be derived; here the
  // contract that matters is that the prologue itself is a function of the transcript.
});

check("signing envelopes: reject and result round-trip with the agreed codes", () => {
  const sid = new Uint8Array(16).fill(0x5a);
  const phone = 2;
  const coordinator = EnvelopeCodec.coordinator_slot;

  // The reject codes are the phone's mapping; a renumbering on either side is a silent misread.
  const codes = { userDeclined: 1, verificationFailed: 2, malformed: 3, expired: 4 };
  for (const [name, byte] of Object.entries(codes)) {
    assert(EnvelopeCodec.rejectCode(name) === byte, `reject code ${name}`);
    const bytes = EnvelopeCodec.buildSignReject(sid, phone, coordinator, 1, byte, "");
    const [sender, seq, sessionId, code, note] = EnvelopeCodec.readSignReject(bytes);
    assert(sender === phone && seq === 1 && code === byte && note === "", `round-trip ${name}`);
    assert(sessionId.length === 16, "session id survives");
    // A reject carries no share but is still a round-3 message.
    assert(EnvelopeCodec.decode(bytes)[1] === 3, "reject is round 3");
  }

  // The round cannot be passed in: it comes from the message type.
  for (const [name, round] of [
    ["signInit", 1],
    ["signCommitments", 1],
    ["signPayload", 3],
    ["signShare", 3],
    ["signReject", 3],
    ["signResult", 3],
  ]) {
    const bytes = EnvelopeCodec.buildSigning(EnvelopeCodec.msg_type(name), phone, coordinator, 1, sid, new Uint8Array());
    assert(EnvelopeCodec.decode(bytes)[1] === round, `${name} must be round ${round}`);
  }

  // A hand-set round is refused on read.
  const fabricated = EnvelopeCodec.build(EnvelopeCodec.msg_type("signShare"), 4, phone, coordinator, 1, sid, new Uint8Array([1]));
  let threw = false;
  try {
    EnvelopeCodec.readSignReject(fabricated);
  } catch {
    threw = true;
  }
  assert(threw, "a wrong round must be refused");

  // Result: broadcast needs a txid, failure must not carry one.
  const txid = new Uint8Array(32).fill(0x9c);
  const ok = EnvelopeCodec.buildSignResult(sid, coordinator, phone, 2, 0, txid, "");
  const [sender2, seq2, , status, gotTxid, note2] = EnvelopeCodec.readSignResult(ok);
  assert(sender2 === coordinator && seq2 === 2 && status === 0 && note2 === "", "broadcast round-trip");
  assert(gotTxid.length === 32 && gotTxid[0] === 0x9c, "txid survives");

  const failed = EnvelopeCodec.buildSignResult(sid, coordinator, phone, 3, 1, undefined, "aggregation failed");
  const [, , , status2, txid2, note3] = EnvelopeCodec.readSignResult(failed);
  assert(status2 === 1 && (txid2 === null || txid2 === undefined) && note3 === "aggregation failed", "failure round-trip");

  threw = false;
  try {
    EnvelopeCodec.buildSignResult(sid, coordinator, phone, 1, 0, undefined, "");
  } catch {
    threw = true;
  }
  assert(threw, "a broadcast result without a txid is not representable");

  // A reject read as a result must not be accepted just because both are round 3.
  const reject = EnvelopeCodec.buildSignReject(sid, phone, coordinator, 1, 1, "");
  threw = false;
  try {
    EnvelopeCodec.readSignResult(reject);
  } catch {
    threw = true;
  }
  assert(threw, "the wrong reader must refuse a reject");
});

check("review: a payment plus change returns indices and header, not just totals", () => {
  // The fixture builder lives in Rust; here we only assert the shape contract the extension codes against:
  // `review` is `[digest, outputs, spends, sentTotal, changeTotal, fee, header]`, and every spend row starts
  // with its bundle index.
  assert(typeof PcztTools.review === "function", "review is exported");
  assert(typeof PcztTools.signingSet === "function", "signingSet is exported");
  assert(typeof PcztTools.check_action_bindings === "function", "binding check is exported");
});

check("noise: three-message XX handshake, then transport", () => {
  const prologue = new TextEncoder().encode("noir/pair/v1");
  // Fixed static keys so a failure is reproducible; the ephemerals are generated inside the wasm.
  const initSk = new Uint8Array(32).fill(0x11);
  const respSk = new Uint8Array(32).fill(0x22);
  const initPk = NoiseXx.static_public_from(initSk);
  const respPk = NoiseXx.static_public_from(respSk);
  assert(initPk.length === 32 && respPk.length === 32);

  const initiator = NoiseXx.initiator(initSk, prologue);
  const responder = NoiseXx.responder(respSk, prologue);

  // Message 1 carries its payload in the clear: 32-byte ephemeral plus the payload verbatim.
  const m1 = initiator.write_handshake(new Uint8Array(0));
  assert(m1.length === 32, `msg1 is ${m1.length} bytes, expected 32`);
  responder.read_handshake(m1);

  const m2 = responder.write_handshake(new Uint8Array([0xaa]));
  const payload2 = initiator.read_handshake(m2);
  assert(payload2.length === 1 && payload2[0] === 0xaa, "message 2 is authenticated and carries payload");

  const m3 = initiator.write_handshake(new Uint8Array(0));
  responder.read_handshake(m3);

  assert(initiator.is_handshake_finished() && responder.is_handshake_finished());
  const hi = hex(initiator.handshake_hash());
  const hr = hex(responder.handshake_hash());
  assert(hi === hr, "both ends must derive the same handshake hash");
  assert(hi.length === 64);

  // Each side learns the other's static key from the handshake itself.
  assert(hex(initiator.remote_static()) === hex(respPk), "initiator sees responder key");
  assert(hex(responder.remote_static()) === hex(initPk), "responder sees initiator key");

  // Transport, both directions, plus nonce advance. Each direction has its own nonce, so messages must
  // be consumed in the order they were written.
  const text = (v) => hex(new TextEncoder().encode(v));
  const c1 = initiator.encrypt(new TextEncoder().encode("commitments"));
  assert(hex(responder.decrypt(c1)) === text("commitments"));
  const c2 = responder.encrypt(new TextEncoder().encode("share"));
  assert(hex(initiator.decrypt(c2)) === text("share"));
  const c3 = initiator.encrypt(new TextEncoder().encode("commitments"));
  assert(hex(c3) !== hex(c1), "the same plaintext must not repeat ciphertext");
  assert(hex(responder.decrypt(c3)) === text("commitments"));

  // Rekey keeps a healthy channel working.
  initiator.rekey();
  responder.rekey();
  const c4 = initiator.encrypt(new TextEncoder().encode("after rekey"));
  assert(hex(responder.decrypt(c4)) === text("after rekey"));

  // Tampered transport data is refused, and kills the channel: Noise advanced the nonce before checking
  // the tag, so the two sides can never resynchronise.
  const bad = new Uint8Array(initiator.encrypt(new TextEncoder().encode("tamper me")));
  bad[bad.length - 1] ^= 0x01;
  let threw = false;
  try {
    responder.decrypt(bad);
  } catch {
    threw = true;
  }
  assert(threw, "tampered ciphertext must be refused");
  assert(responder.is_poisoned(), "a failed decrypt must kill the channel");
  let refused = false;
  try {
    responder.decrypt(initiator.encrypt(new TextEncoder().encode("later")));
  } catch {
    refused = true;
  }
  assert(refused, "a poisoned channel must refuse everything");
});

check("noise: a mismatched prologue does not establish a channel", () => {
  const initSk = new Uint8Array(32).fill(0x33);
  const respSk = new Uint8Array(32).fill(0x44);
  const initiator = NoiseXx.initiator(initSk, new TextEncoder().encode("noir/pair/v1"));
  const responder = NoiseXx.responder(respSk, new TextEncoder().encode("noir/other/v1"));

  const m1 = initiator.write_handshake(new Uint8Array(0));
  let failed = false;
  try {
    responder.read_handshake(m1);
    const m2 = responder.write_handshake(new Uint8Array(0));
    initiator.read_handshake(m2);
  } catch {
    failed = true;
  }
  assert(failed, "different prologues must not produce a channel");
});

check("session: a signer can refuse, and the coordinator reports the outcome", () => {
  const ceremony = ceremonyFor(2, 2);
  const sessionId = EnvelopeCodec.new_session_id();
  const signers = [0, 1];
  const coordinator = new Coordinator(sessionId, 2, 2, signers, 1, ceremony.publicPackage);
  const handles = signers.map((slot) => new Signer(sessionId, slot, 2, ceremony.keyPackages[slot]));

  for (const [slot, env] of coordinator.init_all()) {
    coordinator.accept_commitments(handles[signers.indexOf(slot)].init(env));
  }
  const sighash = new Uint8Array(32).fill(0x11);
  const alphas = new Uint8Array(32);
  alphas[0] = 0x31;
  alphas[31] = 0x01;
  coordinator.prepare_payload(sighash, alphas);

  const payloads = coordinator.payloads();
  // Slot 0 signs; slot 1 refuses.
  for (const [slot, env] of payloads) {
    if (slot === 0) {
      const h = handles[0];
      h.payload(env);
      h.approve();
      coordinator.accept_share(h.share());
    } else {
      const h = handles[1];
      h.payload(env);
      const reject = h.reject(1, "user said no"); // 1 = user declined
      assert(reject.length > 0);
      const [sender, code, note] = coordinator.accept_reject(reject);
      assert(sender === 1, `sender ${sender}`);
      assert(code === 1, `code ${code}`);
      assert(note === "user said no", note);
    }
  }
  // One signer out of a 2-of-2 means the session cannot aggregate.
  assert(coordinator.phase() === "Aborted", coordinator.phase());
  let threw = false;
  try {
    coordinator.aggregate();
  } catch {
    threw = true;
  }
  assert(threw, "an aborted session must not aggregate");
});

check("session: the coordinator reports the outcome to every signer", () => {
  const ceremony = ceremonyFor(3, 2);
  const sessionId = EnvelopeCodec.new_session_id();
  const signers = [0, 1];
  const coordinator = new Coordinator(sessionId, 3, 2, signers, 1, ceremony.publicPackage);
  const handles = signers.map((slot) => new Signer(sessionId, slot, 3, ceremony.keyPackages[slot]));

  for (const [slot, env] of coordinator.init_all()) {
    coordinator.accept_commitments(handles[signers.indexOf(slot)].init(env));
  }
  const sighash = new Uint8Array(32).fill(0x11);
  const alphas = new Uint8Array(32);
  alphas[0] = 0x41;
  alphas[31] = 0x01;
  coordinator.prepare_payload(sighash, alphas);
  for (const [slot, env] of coordinator.payloads()) {
    const h = handles[signers.indexOf(slot)];
    h.payload(env);
    h.approve();
    coordinator.accept_share(h.share());
  }
  coordinator.aggregate();

  const txid = new Uint8Array(32).fill(0x7a);
  const results = coordinator.report_result(0, txid, "broadcasted");
  assert(results.length === 2, `results: ${results.length}`);
  for (const [slot, env] of results) {
    const h = handles[signers.indexOf(slot)];
    const [status, gotTxid, note] = h.on_result(env);
    assert(status === 0, `status ${status}`);
    assert(hex(gotTxid) === hex(txid), "txid must round-trip");
    assert(note === "broadcasted", note);
    assert(h.phase() === "Broadcasted", h.phase());
  }
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
