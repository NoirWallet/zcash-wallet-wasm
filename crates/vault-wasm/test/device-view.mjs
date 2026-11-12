// One device at a time.
//
// basic.mjs drives every device from one place, so it can reach across objects — which is how a
// missing "how do I get the peer's static key" API slipped through review. This file is the
// counterpart: each device holds its own handles in `#` private fields and the test can only move
// `Uint8Array`s between them. If the wasm forgets to expose something a device needs to complete its
// own side, this file cannot be made to pass.
//
// Run:  node crates/vault-wasm/test/device-view.mjs

import {
  Coordinator,
  Dkg,
  DkgRelay,
  EnvelopeCodec,
  Keyset,
  MessageSet,
  Sealed,
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

/**
 * A single device. Everything it owns is private: no other device and no test code can reach into it.
 * The only things that leave are `Uint8Array`s, which is exactly what the wire carries.
 */
class Device {
  #slot;
  #participants;
  #threshold;
  #sessionId;
  #dkg;
  #dkg1 = null;
  #dkg2 = null;
  #dkg3 = null;
  #signer = null;

  constructor(participants, threshold, slot, sessionId) {
    this.#participants = participants;
    this.#threshold = threshold;
    this.#slot = slot;
    this.#sessionId = sessionId;
    this.#dkg = new Dkg(participants, threshold, slot);
  }

  slot() {
    return this.#slot;
  }

  /** Round 1: produce the envelope to send to the coordinator. */
  round1() {
    this.#dkg1 = this.#dkg.part1();
    return this.#dkg1.broadcast_envelope(this.#sessionId, 1, this.#slot);
  }

  /** Round 2: consume the bundled round-1 broadcasts, return the envelope for the coordinator. */
  round2(bundlePayload) {
    const broadcasts = MessageSet.from_bytes(bundlePayload);
    this.#dkg2 = this.#dkg.part2(this.#dkg1.secret(), broadcasts);
    return this.#dkg2.outgoing_envelope(this.#sessionId, 2, this.#slot);
  }

  /** Round 3: consume the bundled round-2 packages and finalize. */
  round3(bundlePayload, broadcastsPayload) {
    const incoming = MessageSet.from_bytes(bundlePayload);
    const broadcasts = MessageSet.from_bytes(broadcastsPayload);
    this.#dkg3 = this.#dkg.part3(this.#dkg2.secret(), broadcasts, incoming);
  }

  // --- The public surface a device can offer about itself ---

  /** `ak`, safe to compare with peers. */
  groupKey() {
    return this.#dkg3.group_verifying_key();
  }

  /** Every participant's static key, learned from the broadcasts this device was handed. */
  statics() {
    return this.#dkg3.statics();
  }

  /** Seal `plaintext` to another slot, using only this device's state. */
  sealTo(slot, plaintext) {
    const statics = this.statics();
    const recipient = statics.get(slot);
    assert(recipient, `no static key for slot ${slot}`);
    return Sealed.seal(
      this.#dkg3.static_secret(),
      recipient,
      Sealed.prologue(statics),
      plaintext,
    );
  }

  /** Open a message from another slot. */
  openFrom(slot, ciphertext) {
    const statics = this.statics();
    const sender = statics.get(slot);
    assert(sender, `no static key for slot ${slot}`);
    return Sealed.open(
      this.#dkg3.static_secret(),
      sender,
      Sealed.prologue(statics),
      ciphertext,
    );
  }

  /** This device's key package bytes, for its own signer handle. */
  keyPackage() {
    return this.#dkg3.key_package();
  }

  /** Public group state, as the coordinator would hold it. */
  publicPackage() {
    return this.#dkg3.public_key_package();
  }

  // --- Signing, owned by the device ---

  asSigner() {
    this.#signer = new Signer(
      this.#sessionId,
      this.#slot,
      this.#participants,
      this.keyPackage(),
    );
    return this.#signer;
  }

  asCoordinator(signers, actions) {
    return new Coordinator(
      this.#sessionId,
      this.#participants,
      this.#threshold,
      signers,
      actions,
      this.publicPackage(),
    );
  }
}

/** The coordinator's message plumbing. Sees only bytes. */
class Relay {
  #relay;
  constructor(sessionId, participants) {
    this.#relay = new DkgRelay(sessionId, participants);
  }
  accept(envelope, expected) {
    return this.#relay.accept(envelope, expected);
  }
  bundle(round, recipient, set) {
    return this.#relay.bundle(round, recipient, set);
  }
}

// One pass: nothing is re-read, and the accepted bytes stay in local variables.
function ceremony(participants, threshold) {
  const sessionId = EnvelopeCodec.new_session_id();
  const devices = Array.from(
    { length: participants },
    (_, slot) => new Device(participants, threshold, slot, sessionId),
  );
  const relay = new Relay(sessionId, participants);

  const broadcasts = new MessageSet();
  for (const d of devices) {
    const [slot, payload] = relay.accept(d.round1(), "dkgR1");
    assert(slot === d.slot(), "relay reports the right sender");
    broadcasts.set(slot, payload);
  }

  const batch = new MessageSet();
  for (const d of devices) {
    const bundle = relay.bundle(1, d.slot(), broadcasts);
    const payload = EnvelopeCodec.decode(bundle)[6];
    const env = d.round2(payload);
    const [slot, sealed] = relay.accept(env, "dkgR2Batch");
    // Keep the raw bytes: a MessageSet holds bytes, not other MessageSets.
    batch.set(slot, sealed);
  }

  // Turn "each sender's set of packages" into "each recipient's set of packages". Still bytes.
  const inboxes = new Map();
  for (const d of devices) inboxes.set(d.slot(), new MessageSet());
  for (const sender of batch.slots()) {
    const set = batch.get(sender);
    const parsed = MessageSet.from_bytes(set);
    for (const recipient of parsed.slots()) {
      inboxes.get(recipient).set(sender, parsed.get(recipient));
    }
  }

  for (const d of devices) {
    const bundle = relay.bundle(2, d.slot(), inboxes.get(d.slot()));
    d.round3(EnvelopeCodec.decode(bundle)[6], broadcasts.to_bytes());
  }

  return { devices, sessionId };
}

check("every device finalizes using only its own handles", () => {
  for (const [participants, threshold] of [
    [2, 2],
    [3, 2],
    [3, 3],
  ]) {
    const { devices } = ceremony(participants, threshold);

    const ak = hex(devices[0].groupKey());
    for (const d of devices) {
      assert(hex(d.groupKey()) === ak, `${threshold}-of-${participants}: ak disagrees`);
    }

    // The property that was missing: each device learns the whole roster's static keys from the
    // broadcasts it was handed, without touching another device's object.
    for (const d of devices) {
      assert(d.statics().len() === participants, `slot ${d.slot()} sees ${d.statics().len()}`);
    }
    for (let slot = 0; slot < participants; slot += 1) {
      const views = devices.map((d) => hex(d.statics().get(slot)));
      assert(views.every((k) => k === views[0]), `devices disagree about slot ${slot}`);
      assert(views[0].length === 64, "a static key is 32 bytes");
    }
  }
});

check("a device can seal the qsk to a peer and the peer opens it", () => {
  const { devices } = ceremony(3, 2);
  const coordinator = devices[0];

  const qsk = Keyset.generate_qsk();
  const ak = coordinator.groupKey();
  const forB = coordinator.sealTo(1, qsk);

  // The recipient opens it with its own state and derives the same keyset.
  const material = devices[1].openFrom(0, forB);
  assert(hex(material) === hex(qsk), "the qsk survived the round trip");

  const mine = Keyset.derive(ak, qsk, false, 0);
  const theirs = Keyset.derive(ak, material, false, 0);
  assert(hex(mine[4]) === hex(theirs[4]), "fingerprints agree");
  assert(mine[2] === theirs[2], "addresses agree");
  assert(Keyset.address_is_shielded_only(mine[2]));
});

check("a third device cannot open a package addressed to another", () => {
  const { devices } = ceremony(3, 2);
  const forB = devices[0].sealTo(1, Keyset.generate_qsk());
  let threw = false;
  try {
    devices[2].openFrom(0, forB);
  } catch {
    threw = true;
  }
  assert(threw, "slot 2 must not open slot 1's package");
});

check("a signing session runs with each device owning its own handle", () => {
  const participants = 3;
  const threshold = 2;
  const { devices, sessionId } = ceremony(participants, threshold);
  const ak = devices[0].groupKey();
  const signers = [0, 1];

  const coordinator = devices[0].asCoordinator(signers, 1);
  const signerHandles = signers.map((slot) => devices[slot].asSigner());

  // A synthetic digest: the PCZT tier is exercised elsewhere; this is about ownership.
  const sighash = new Uint8Array(32).fill(0x11);
  const alphas = new Uint8Array(32);
  alphas[0] = 0x31;
  alphas[31] = 0x01;

  for (const [slot, envelope] of coordinator.init_all()) {
    const idx = signers.indexOf(slot);
    coordinator.accept_commitments(signerHandles[idx].init(envelope));
  }
  coordinator.prepare_payload(sighash, alphas);
  for (const [slot, envelope] of coordinator.payloads()) {
    const idx = signers.indexOf(slot);
    signerHandles[idx].payload(envelope);
    signerHandles[idx].approve();
    coordinator.accept_share(signerHandles[idx].share());
  }

  const signatures = coordinator.aggregate();
  assert(signatures.length === 64);
  Verification.verify_spend_auth(ak, alphas, sighash, signatures);
});

if (failures > 0) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
console.log("\nvault-wasm (single device view): all checks passed");
