//! JS bindings for the FROST Secure Vault.
//!
//! Everything crosses the boundary as `Uint8Array` or a plain number, per the handoff contract: the
//! frontend owns transport, storage and UI, and the wasm owns protocol state. That means there are
//! no JS-implemented traits here — anything the host must remember is a byte blob it can store
//! however it likes.
//!
//! Design notes worth knowing before reading further:
//!
//! * Roster slots are 0-based; the FROST identifier for a slot is derived inside the crate. The
//!   frontend never sees or carries identifiers.
//! * Secrets (key package, DKG round secrets) are held inside the Rust objects, not passed back and
//!   forth. The frontend gets a blob only when it needs to persist or transmit one.
//! * A failed call returns `Err`; a wasm panic would abort the instance, so nothing here is allowed
//!   to panic on hostile input. The decoders underneath are covered by adversarial tests.

use std::collections::BTreeMap;

use frost_vault_core::custody;
use frost_vault_core::protocol::{self, Envelope, MsgType, COORDINATOR_SLOT};
use frost_vault_core::seal;
use frost_vault_core::session::{CoordinatorSession, SessionConfig, SignerSession};
use frost_vault_core::state::{NonceState, SignPhase};
use frost_vault_core::threshold::{
    self, DkgFinal, DkgRound1, DkgRound2, KeyPackage, PublicKeyPackage,
};
use rand_core::OsRng;
use wasm_bindgen::prelude::*;

type Rng = OsRng;

fn rng() -> Rng {
    OsRng
}

fn err(e: impl core::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn to_js_bytes(bytes: &[u8]) -> js_sys::Uint8Array {
    js_sys::Uint8Array::from(bytes)
}

/// A set of messages indexed by peer slot.
///
/// The frontend fills this with whatever its transport received, and reads it back after a round.
/// Exists so the wire format of a message batch stays inside the wasm.
#[wasm_bindgen]
pub struct MessageSet {
    inner: BTreeMap<u8, Vec<u8>>,
}

#[wasm_bindgen]
impl MessageSet {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    /// Add or replace the payload for a peer.
    pub fn set(&mut self, slot: u8, bytes: &[u8]) {
        self.inner.insert(slot, bytes.to_vec());
    }

    pub fn get(&self, slot: u8) -> Option<js_sys::Uint8Array> {
        self.inner.get(&slot).map(|b| to_js_bytes(b))
    }

    pub fn slots(&self) -> Vec<u8> {
        self.inner.keys().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Pack for transport as one envelope payload.
    pub fn to_bytes(&self) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(
            &protocol::pack_batch(&self.inner).map_err(err)?,
        ))
    }

    /// Unpack an envelope payload back into a set.
    #[wasm_bindgen]
    pub fn from_bytes(bytes: &[u8]) -> Result<MessageSet, JsError> {
        Ok(Self {
            inner: protocol::unpack_batch(bytes).map_err(err)?,
        })
    }
}

impl Default for MessageSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode/decode an envelope. The frontend may treat envelopes as opaque bytes, but exposing the codec
/// lets it validate framing before spending a QR frame on it.
#[wasm_bindgen]
pub struct EnvelopeCodec;

#[wasm_bindgen]
impl EnvelopeCodec {
    /// Build a `SIGN_INIT`-shaped envelope. Mostly useful for transport tests.
    #[wasm_bindgen]
    pub fn build(
        msg_type: u8,
        round: u8,
        sender: u8,
        recipient: u8,
        sequence: u32,
        session_id: &[u8],
        payload: &[u8],
    ) -> Result<js_sys::Uint8Array, JsError> {
        let session_id: [u8; 16] = session_id
            .try_into()
            .map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        let msg_type =
            MsgType::from_byte(msg_type).ok_or_else(|| JsError::new("unknown msg_type"))?;
        let env = Envelope::new(
            msg_type,
            round,
            sender,
            recipient,
            sequence,
            session_id,
            payload.to_vec(),
        )
        .map_err(err)?;
        Ok(to_js_bytes(&env.encode()))
    }

    /// Decode an envelope into a JSON-ish object: `{ msgType, round, sender, recipient, sequence,
    /// sessionId: Uint8Array, payload: Uint8Array }` as a JS array, to keep the surface primitive.
    #[wasm_bindgen]
    pub fn decode(bytes: &[u8]) -> Result<js_sys::Array, JsError> {
        let env = Envelope::decode(bytes).map_err(err)?;
        let out = js_sys::Array::new();
        out.push(&JsValue::from(env.msg_type.to_byte()));
        out.push(&JsValue::from(env.round));
        out.push(&JsValue::from(env.sender));
        out.push(&JsValue::from(env.recipient));
        out.push(&JsValue::from(env.sequence));
        out.push(&to_js_bytes(&env.session_id));
        out.push(&to_js_bytes(&env.payload));
        Ok(out)
    }

    /// The slot reserved for the coordinator, which may hold no share.
    #[wasm_bindgen(getter)]
    pub fn coordinator_slot() -> u8 {
        COORDINATOR_SLOT
    }

    /// A fresh 16-byte session id, from this module's OS-backed entropy.
    ///
    /// Use this rather than a counter or a timestamp: a repeated id would make a stale session
    /// marker look like the current session after a restart, which is exactly the case the marker
    /// exists to catch.
    #[wasm_bindgen]
    pub fn new_session_id() -> js_sys::Uint8Array {
        to_js_bytes(&custody::new_session_id(&mut rng()))
    }

    /// Wire value of a message type **whose payload this crate defines**.
    ///
    /// Eight types qualify today: the four signing messages, whose payloads the session produces, and
    /// the four DKG messages, whose payloads are this crate's own DKG bytes and packed batches.
    ///
    /// The protocol table also reserves numbers for messages that have no payload format yet —
    /// `PAIR_INVITE`, `ROSTER_LOCK`, `KEYSET_SETUP` and others. Those are deliberately refused here so
    /// that an undefined type cannot become a de-facto contract by accident; use
    /// [`Self::reserved_msg_type`] if you genuinely need the number, and know that nothing defines
    /// what goes in it.
    #[wasm_bindgen]
    pub fn msg_type(name: &str) -> Result<u8, JsError> {
        match defined_msg_type(name) {
            Some(msg_type) => Ok(msg_type.to_byte()),
            None => match reserved_msg_type(name) {
                Some(_) => Err(JsError::new(&format!(
                    "{name} is reserved: its payload format is not defined yet"
                ))),
                None => Err(JsError::new(&format!("unknown message type {name}"))),
            },
        }
    }

    /// Wire value of a **reserved** type: the number exists in the protocol table, but no payload
    /// format is defined for it. Prefer [`Self::msg_type`]; this exists so a caller can decode a
    /// message it received rather than invent one it should not send.
    #[wasm_bindgen]
    pub fn reserved_msg_type(name: &str) -> Result<u8, JsError> {
        reserved_msg_type(name)
            .map(|msg_type| msg_type.to_byte())
            .ok_or_else(|| JsError::new(&format!("{name} is not a reserved message type")))
    }

    /// The type names that have a defined payload.
    #[wasm_bindgen]
    pub fn defined_msg_types() -> js_sys::Array {
        let out = js_sys::Array::new();
        for name in DEFINED_MSG_TYPES {
            out.push(&JsValue::from(*name));
        }
        out
    }

    /// The type names that are reserved without a payload format.
    #[wasm_bindgen]
    pub fn reserved_msg_types() -> js_sys::Array {
        let out = js_sys::Array::new();
        for name in RESERVED_MSG_TYPES {
            out.push(&JsValue::from(*name));
        }
        out
    }
}

/// Message types whose payload this crate defines.
const DEFINED_MSG_TYPES: &[&str] = &[
    "dkgR1",
    "dkgR1Bundle",
    "dkgR2Batch",
    "dkgR2Inbox",
    "signInit",
    "signCommitments",
    "signPayload",
    "signShare",
];

/// Types that appear in the protocol table with no payload format. Named here so the split is
/// visible from the API rather than only from documentation.
const RESERVED_MSG_TYPES: &[&str] = &[
    "pairInvite",
    "pairResponse",
    "rosterLock",
    "dkgR1Echo",
    "dkgR1Cert",
    "keysetSetup",
    "dkgFinal",
    "dkgActivate",
    "signReject",
    "signResult",
    "sessionAbort",
];

fn defined_msg_type(name: &str) -> Option<MsgType> {
    Some(match name {
        "dkgR1" => MsgType::DkgR1,
        "dkgR1Bundle" => MsgType::DkgR1Bundle,
        "dkgR2Batch" => MsgType::DkgR2Batch,
        "dkgR2Inbox" => MsgType::DkgR2Inbox,
        "signInit" => MsgType::SignInit,
        "signCommitments" => MsgType::SignCommitments,
        "signPayload" => MsgType::SignPayload,
        "signShare" => MsgType::SignShare,
        _ => return None,
    })
}

fn reserved_msg_type(name: &str) -> Option<MsgType> {
    Some(match name {
        "pairInvite" => MsgType::PairInvite,
        "pairResponse" => MsgType::PairResponse,
        "rosterLock" => MsgType::RosterLock,
        "dkgR1Echo" => MsgType::DkgR1Echo,
        "dkgR1Cert" => MsgType::DkgR1Cert,
        "keysetSetup" => MsgType::KeysetSetup,
        "dkgFinal" => MsgType::DkgFinal,
        "dkgActivate" => MsgType::DkgActivate,
        "signReject" => MsgType::SignReject,
        "signResult" => MsgType::SignResult,
        "sessionAbort" => MsgType::SessionAbort,
        _ => return None,
    })
}

/// Round 1 of a distributed key generation: a secret to keep, a broadcast to send.
#[wasm_bindgen]
pub struct Dkg1 {
    inner: DkgRound1,
}

#[wasm_bindgen]
impl Dkg1 {
    /// Secret state for round 2. Persist it under the device keystore; it never leaves the device.
    pub fn secret(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.secret)
    }

    /// Public broadcast for every other participant.
    pub fn broadcast(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.broadcast)
    }

    /// The broadcast wrapped in a `DKGR1` envelope addressed to the coordinator.
    ///
    /// Use this instead of shipping the raw bytes: the envelope is what binds the payload to this
    /// session, this sender and this round, and what lets the relay reject replays.
    pub fn broadcast_envelope(
        &self,
        session_id: &[u8],
        sequence: u32,
        my_slot: u8,
    ) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(&dkg_envelope(
            MsgType::DkgR1,
            1,
            my_slot,
            COORDINATOR_SLOT,
            sequence,
            session_id,
            self.inner.broadcast.clone(),
        )?))
    }
}

/// Round 2 output: a secret to keep plus one sealed package per peer.
#[wasm_bindgen]
pub struct Dkg2 {
    inner: DkgRound2,
}

#[wasm_bindgen]
impl Dkg2 {
    pub fn secret(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.secret)
    }

    /// Packages to deliver, indexed by recipient slot.
    pub fn outgoing(&self) -> MessageSet {
        MessageSet {
            inner: self.inner.outgoing.clone(),
        }
    }

    /// The whole batch wrapped in a `DKGR2BATCH` envelope addressed to the coordinator.
    pub fn outgoing_envelope(
        &self,
        session_id: &[u8],
        sequence: u32,
        my_slot: u8,
    ) -> Result<js_sys::Uint8Array, JsError> {
        let payload = protocol::pack_batch(&self.inner.outgoing).map_err(err)?;
        Ok(to_js_bytes(&dkg_envelope(
            MsgType::DkgR2Batch,
            2,
            my_slot,
            COORDINATOR_SLOT,
            sequence,
            session_id,
            payload,
        )?))
    }
}

/// Round 3 output: this device's key package and the group's public package.
#[wasm_bindgen]
pub struct Dkg3 {
    inner: DkgFinal,
}

#[wasm_bindgen]
impl Dkg3 {
    /// The share. Persist it as a [`ShareBackup`]; never transmit it.
    pub fn key_package(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.key_package)
    }

    /// The public group state. Safe to share; every device must end up with the same bytes.
    pub fn public_key_package(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.public_key_package)
    }

    /// `ak`, the group's spend validating key. Every other call needs it: the keyset derivation, the
    /// share backup binding and the signature verification all take it.
    pub fn group_verifying_key(&self) -> Result<js_sys::Uint8Array, JsError> {
        let package = PublicKeyPackage::deserialize(&self.inner.public_key_package).map_err(err)?;
        let bytes = package.verifying_key().serialize().map_err(err)?;
        Ok(to_js_bytes(&bytes))
    }

    /// This ceremony's static public key. Publish it (it already travels in the round-1 broadcast).
    pub fn static_public(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.static_public)
    }

    /// The matching static secret, for sealing later ceremony messages (KEYSET_SETUP) to peers.
    ///
    /// Keep it on the device. It cannot spend; it only lets peers send this device a message nobody
    /// else can read.
    pub fn static_secret(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.static_secret)
    }
}

/// Sealed, authenticated messages between ceremony participants.
///
/// The DKG needs this for its round-2 packages (handled internally), and KEYSET_SETUP needs it for
/// the `qsk`, which is the `nk`/`rivk` source and therefore a scanning secret: whoever relays it must
/// not read it.
///
/// Both parties' static keys are known from the DKG, so sealing is a single message with no round
/// trip. `prologue` binds the participant set: a device that is shown a different roster derives a
/// different key schedule and cannot open the message.
#[wasm_bindgen]
pub struct Sealed;

#[wasm_bindgen]
impl Sealed {
    /// The prologue for a participant set, given their static public keys by slot.
    #[wasm_bindgen]
    pub fn prologue(statics: &MessageSet) -> Result<js_sys::Uint8Array, JsError> {
        let pairs: Vec<(u8, Vec<u8>)> = statics
            .inner
            .iter()
            .map(|(slot, key)| (*slot, key.clone()))
            .collect();
        if pairs.is_empty() {
            return Err(JsError::new("prologue needs at least one participant"));
        }
        Ok(to_js_bytes(&seal::ceremony_prologue(&pairs)))
    }

    /// Seal `plaintext` so only the holder of `recipient_public` can read it.
    #[wasm_bindgen]
    pub fn seal(
        sender_secret: &[u8],
        recipient_public: &[u8],
        prologue: &[u8],
        plaintext: &[u8],
    ) -> Result<js_sys::Uint8Array, JsError> {
        let sealed =
            seal::seal(sender_secret, recipient_public, prologue, plaintext).map_err(err)?;
        Ok(to_js_bytes(&sealed))
    }

    /// Open a message from `sender_public`. Fails if the pair, the ceremony or the bytes are wrong.
    #[wasm_bindgen]
    pub fn open(
        recipient_secret: &[u8],
        sender_public: &[u8],
        prologue: &[u8],
        ciphertext: &[u8],
    ) -> Result<js_sys::Uint8Array, JsError> {
        let opened =
            seal::open(recipient_secret, sender_public, prologue, ciphertext).map_err(err)?;
        Ok(to_js_bytes(&opened))
    }
}

/// One device's participation in a distributed key generation.
#[wasm_bindgen]
pub struct Dkg {
    participants: u16,
    threshold: u16,
    slot: u8,
}

#[wasm_bindgen]
impl Dkg {
    #[wasm_bindgen(constructor)]
    pub fn new(participants: u16, threshold: u16, slot: u8) -> Result<Dkg, JsError> {
        if slot as u16 >= participants {
            return Err(JsError::new("slot is outside the roster"));
        }
        threshold::slot_identifier(slot).map_err(err)?;
        Ok(Self {
            participants,
            threshold,
            slot,
        })
    }

    pub fn slot(&self) -> u8 {
        self.slot
    }

    /// Round 1: generate this device's contribution.
    pub fn part1(&self) -> Result<Dkg1, JsError> {
        let inner = threshold::dkg_part1(self.participants, self.threshold, self.slot, &mut rng())
            .map_err(err)?;
        Ok(Dkg1 { inner })
    }

    /// Round 2: seal one package per peer from everyone's broadcasts.
    pub fn part2(&self, secret: &[u8], broadcasts: &MessageSet) -> Result<Dkg2, JsError> {
        let peers = strip_self(&broadcasts.inner, self.slot);
        let inner = threshold::dkg_part2(secret, &peers).map_err(err)?;
        Ok(Dkg2 { inner })
    }

    /// Round 3: finalize with the broadcasts and the packages addressed to this device.
    pub fn part3(
        &self,
        secret: &[u8],
        broadcasts: &MessageSet,
        incoming: &MessageSet,
    ) -> Result<Dkg3, JsError> {
        let peers = strip_self(&broadcasts.inner, self.slot);
        let inner = threshold::dkg_part3(secret, &peers, &incoming.inner).map_err(err)?;
        Ok(Dkg3 { inner })
    }
}

fn dkg_envelope(
    msg_type: MsgType,
    round: u8,
    sender: u8,
    recipient: u8,
    sequence: u32,
    session_id: &[u8],
    payload: Vec<u8>,
) -> Result<Vec<u8>, JsError> {
    let session_id: [u8; 16] = session_id
        .try_into()
        .map_err(|_| JsError::new("session_id must be 16 bytes"))?;
    let envelope = Envelope::new(
        msg_type, round, sender, recipient, sequence, session_id, payload,
    )
    .map_err(err)?;
    Ok(envelope.encode())
}

/// The coordinator's side of the DKG message flow.
///
/// Signers send `DKGR1` / `DKGR2BATCH` envelopes; the coordinator validates who sent them and in what
/// order, then forwards a `DKGR1BUNDLE` / `DKGR2INBOX` to each peer. This type exists so that the
/// session binding, roster check and replay rejection are not left to the app to reimplement.
///
/// Round-2 packages are sealed to their recipient inside the DKG implementation (Noise, single
/// message, ceremony as prologue), so a relay can forward them but cannot read them.
#[wasm_bindgen]
pub struct DkgRelay {
    session_id: [u8; 16],
    participants: u16,
    last_seq: BTreeMap<u8, u32>,
    seq: u32,
}

#[wasm_bindgen]
impl DkgRelay {
    #[wasm_bindgen(constructor)]
    pub fn new(session_id: &[u8], participants: u16) -> Result<DkgRelay, JsError> {
        let session_id: [u8; 16] = session_id
            .try_into()
            .map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        Ok(Self {
            session_id,
            participants,
            last_seq: BTreeMap::new(),
            seq: 0,
        })
    }

    /// Validate an envelope from a signer and return `[senderSlot, payload]`.
    ///
    /// `expected` is the message type name (`"dkgR1"` or `"dkgR2Batch"`). Anything that is not from a
    /// roster slot, not addressed to the coordinator, not the expected type, or not newer than the
    /// last message from that sender is refused.
    pub fn accept(&mut self, envelope: &[u8], expected: &str) -> Result<js_sys::Array, JsError> {
        let want = MsgType::from_byte(EnvelopeCodec::msg_type(expected)?)
            .ok_or_else(|| JsError::new("unknown message type"))?;
        let env = Envelope::decode(envelope).map_err(err)?;
        if env.session_id != self.session_id {
            return Err(JsError::new("envelope belongs to another session"));
        }
        if env.recipient != COORDINATOR_SLOT {
            return Err(JsError::new("envelope is not addressed to the coordinator"));
        }
        if env.msg_type != want {
            return Err(JsError::new("unexpected message type"));
        }
        if env.sender as u16 >= self.participants {
            return Err(JsError::new("sender is outside the roster"));
        }
        if let Some(last) = self.last_seq.get(&env.sender) {
            if env.sequence <= *last {
                return Err(JsError::new("replayed or out-of-order envelope"));
            }
        }
        self.last_seq.insert(env.sender, env.sequence);

        let out = js_sys::Array::new();
        out.push(&JsValue::from(env.sender));
        out.push(&to_js_bytes(&env.payload));
        Ok(out)
    }

    /// Forward a bundle to one recipient: `DKGR1BUNDLE` in round 1, `DKGR2INBOX` in round 2.
    pub fn bundle(
        &mut self,
        round: u8,
        recipient: u8,
        messages: &MessageSet,
    ) -> Result<js_sys::Uint8Array, JsError> {
        let msg_type = match round {
            1 => MsgType::DkgR1Bundle,
            2 => MsgType::DkgR2Inbox,
            other => return Err(JsError::new(&format!("no bundle type for round {other}"))),
        };
        let payload = protocol::pack_batch(&messages.inner).map_err(err)?;
        self.seq = self.seq.wrapping_add(1);
        Ok(to_js_bytes(&dkg_envelope(
            msg_type,
            round,
            COORDINATOR_SLOT,
            recipient,
            self.seq,
            &self.session_id,
            payload,
        )?))
    }
}

/// `[[slot, bytes], ...]`, so a caller can route each envelope without parsing it.
fn slot_and_bytes(envelopes: Vec<Envelope>) -> js_sys::Array {
    let out = js_sys::Array::new();
    for env in envelopes {
        let pair = js_sys::Array::new();
        pair.push(&JsValue::from(env.recipient));
        pair.push(&to_js_bytes(&env.encode()));
        out.push(&pair);
    }
    out
}

fn strip_self(all: &BTreeMap<u8, Vec<u8>>, me: u8) -> BTreeMap<u8, Vec<u8>> {
    all.iter()
        .filter(|(slot, _)| **slot != me)
        .map(|(slot, bytes)| (*slot, bytes.clone()))
        .collect()
}

/// A device's key share, bound to a group and slot.
#[wasm_bindgen]
pub struct ShareBackup {
    inner: custody::ShareBackup,
}

#[wasm_bindgen]
impl ShareBackup {
    /// Wrap a key package for storage. `device_id` and `vault_id` are opaque to the wasm and must be
    /// stable across restores.
    #[wasm_bindgen(constructor)]
    pub fn new(
        key_package: &[u8],
        slot: u8,
        device_id: &[u8],
        vault_id: &[u8],
        group_verifying_key: &[u8],
    ) -> Result<ShareBackup, JsError> {
        let key_package = KeyPackage::deserialize(key_package).map_err(err)?;
        let group: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let actual: [u8; 32] = key_package
            .verifying_key()
            .serialize()
            .map_err(err)?
            .as_slice()
            .try_into()
            .map_err(|_| JsError::new("key package has no group key"))?;
        if actual != group {
            return Err(JsError::new("key package belongs to another group"));
        }
        let inner = custody::ShareBackup::new(
            &key_package,
            custody::ShareIdentity {
                slot,
                device_id: device_id.to_vec(),
                vault_id: vault_id.to_vec(),
            },
        )
        .map_err(err)?;
        Ok(Self { inner })
    }

    /// Bytes for the host to seal and store. The wasm does not encrypt.
    pub fn to_bytes(&self) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(
            &self
                .inner
                .to_bytes()
                .map_err(|e| JsError::new(&format!("{e:?}")))?,
        ))
    }

    /// Load and validate against the expected group and slot.
    pub fn from_bytes(
        bytes: &[u8],
        group_verifying_key: &[u8],
        expected_slot: u8,
    ) -> Result<ShareBackup, JsError> {
        let group: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let inner = custody::ShareBackup::from_bytes(bytes, &group, expected_slot).map_err(err)?;
        Ok(Self { inner })
    }

    pub fn slot(&self) -> u8 {
        self.inner.identity().slot
    }

    pub fn device_id(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.identity().device_id)
    }

    pub fn vault_id(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.identity().vault_id)
    }

    pub fn group_verifying_key(&self) -> js_sys::Uint8Array {
        to_js_bytes(self.inner.group_verifying_key())
    }

    /// The key package, for handing to a [`SignerSession`]. Kept out of `to_bytes` on purpose: the
    /// backup blob is what the host stores, not what it needs in memory.
    pub fn key_package(&self) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(
            &self
                .inner
                .key_package()
                .map_err(err)?
                .serialize()
                .map_err(|e| JsError::new(&format!("{e:?}")))?,
        ))
    }
}

/// Session marker helpers: what the host persists to know a session was in flight.
#[wasm_bindgen]
pub struct SessionMarker;

#[wasm_bindgen]
impl SessionMarker {
    /// Build the marker to persist right after a session starts.
    #[wasm_bindgen]
    pub fn created(session_id: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let id: [u8; 16] = session_id
            .try_into()
            .map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        Ok(to_js_bytes(&custody::marker_bytes(
            id,
            custody::SessionProgress::Created,
        )))
    }

    /// Advance a marker once commitments have left the device.
    #[wasm_bindgen]
    pub fn committed(session_id: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let id: [u8; 16] = session_id
            .try_into()
            .map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        Ok(to_js_bytes(&custody::marker_bytes(
            id,
            custody::SessionProgress::Committed,
        )))
    }

    /// What to do about a persisted marker: `"clean"`, `"discardable"` or `"burned"`.
    #[wasm_bindgen]
    pub fn recover(marker: Option<Vec<u8>>) -> Result<String, JsError> {
        let recovery = custody::recover_from_marker(marker.as_deref()).map_err(err)?;
        Ok(match recovery {
            custody::Recovery::Clean => "clean".to_string(),
            custody::Recovery::Discardable { .. } => "discardable".to_string(),
            custody::Recovery::Burned { .. } => "burned".to_string(),
        })
    }

    /// The session a marker refers to, as 16 bytes, or `None` when clean.
    #[wasm_bindgen]
    pub fn session_id(marker: Option<Vec<u8>>) -> Result<Option<js_sys::Uint8Array>, JsError> {
        let recovery = custody::recover_from_marker(marker.as_deref()).map_err(err)?;
        Ok(recovery.session_id().map(|id| to_js_bytes(&id)))
    }
}

/// Coordinator side of a signing session.
#[wasm_bindgen]
pub struct Coordinator {
    inner: CoordinatorSession,
}

#[wasm_bindgen]
impl Coordinator {
    /// `signers` is the selected roster slots (length must equal the threshold).
    #[wasm_bindgen(constructor)]
    pub fn new(
        session_id: &[u8],
        participants: u16,
        threshold: u16,
        signers: &[u8],
        actions: usize,
        public_key_package: &[u8],
    ) -> Result<Coordinator, JsError> {
        let session_id: [u8; 16] = session_id
            .try_into()
            .map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        let mut identifiers = BTreeMap::new();
        for slot in 0..participants {
            identifiers.insert(
                slot as u8,
                threshold::slot_identifier(slot as u8).map_err(err)?,
            );
        }
        let config = SessionConfig {
            session_id,
            participants,
            threshold,
            signers: signers.to_vec(),
            identifiers,
        };
        let pubkeys = PublicKeyPackage::deserialize(public_key_package).map_err(err)?;
        let inner = CoordinatorSession::new(config, pubkeys, actions).map_err(err)?;
        Ok(Self { inner })
    }

    /// The whole `SIGN_INIT` batch: `[[slot, envelope], ...]`. One call, because the session moves to
    /// `InitSent` as a whole.
    pub fn init_all(&mut self) -> Result<js_sys::Array, JsError> {
        let envelopes = self.inner.init().map_err(err)?;
        Ok(slot_and_bytes(envelopes))
    }

    /// Collect a `SIGN_COMMITMENTS` envelope from a signer.
    pub fn accept_commitments(&mut self, envelope: &[u8]) -> Result<(), JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        self.inner.on_commitments(&env).map_err(err)
    }

    /// Hand over the sighash and per-action alphas read from the built transaction.
    pub fn prepare_payload(&mut self, sighash: &[u8], alphas: &[u8]) -> Result<(), JsError> {
        let sighash: [u8; 32] = sighash
            .try_into()
            .map_err(|_| JsError::new("sighash must be 32 bytes"))?;
        if alphas.len() % 32 != 0 {
            return Err(JsError::new("alphas must be a multiple of 32 bytes"));
        }
        let alphas: Vec<[u8; 32]> = alphas
            .chunks_exact(32)
            .map(|c| c.try_into().expect("chunk size checked"))
            .collect();
        self.inner.prepare_payload(sighash, alphas).map_err(err)
    }

    /// The whole `SIGN_PAYLOAD` batch: `[[slot, envelope], ...]`.
    pub fn payloads(&mut self) -> Result<js_sys::Array, JsError> {
        let envelopes = self.inner.payloads().map_err(err)?;
        Ok(slot_and_bytes(envelopes))
    }

    /// Collect a `SIGN_SHARE` envelope.
    pub fn accept_share(&mut self, envelope: &[u8]) -> Result<(), JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        self.inner.on_share(&env).map_err(err)
    }

    /// Aggregate once every signer has returned every share. Returns the signatures, 64 bytes each.
    pub fn aggregate(&mut self) -> Result<js_sys::Uint8Array, JsError> {
        let signatures = self.inner.aggregate().map_err(err)?;
        let mut flat = Vec::with_capacity(signatures.len() * 64);
        for signature in &signatures {
            flat.extend_from_slice(signature);
        }
        Ok(to_js_bytes(&flat))
    }

    pub fn phase(&self) -> String {
        format!("{:?}", self.inner.phase())
    }

    pub fn actions(&self) -> usize {
        self.inner.actions()
    }
}

/// Signer side of a signing session. Holds the key share and the nonces.
#[wasm_bindgen]
pub struct Signer {
    inner: SignerSession,
    key_package: KeyPackage,
}

#[wasm_bindgen]
impl Signer {
    #[wasm_bindgen(constructor)]
    pub fn new(
        session_id: &[u8],
        my_slot: u8,
        participants: u16,
        key_package: &[u8],
    ) -> Result<Signer, JsError> {
        let session_id: [u8; 16] = session_id
            .try_into()
            .map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        let mut identifiers = BTreeMap::new();
        for slot in 0..participants {
            identifiers.insert(
                slot as u8,
                threshold::slot_identifier(slot as u8).map_err(err)?,
            );
        }
        let key_package = KeyPackage::deserialize(key_package).map_err(err)?;
        Ok(Self {
            inner: SignerSession::new(session_id, my_slot, identifiers),
            key_package,
        })
    }

    /// Consume `SIGN_INIT`, produce the `SIGN_COMMITMENTS` envelope to send back.
    pub fn init(&mut self, envelope: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        let out = self
            .inner
            .on_init(&env, &self.key_package, &mut rng())
            .map_err(err)?;
        Ok(to_js_bytes(&out.encode()))
    }

    /// Consume `SIGN_PAYLOAD`. Returns `[sighash: Uint8Array, alphas: Uint8Array]` for the caller to
    /// verify against the PCZT **before** approving.
    pub fn payload(&mut self, envelope: &[u8]) -> Result<js_sys::Array, JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        let material = self.inner.on_payload(&env).map_err(err)?;
        let mut alphas = Vec::with_capacity(material.alphas.len() * 32);
        for alpha in &material.alphas {
            alphas.extend_from_slice(alpha);
        }
        let out = js_sys::Array::new();
        out.push(&to_js_bytes(&material.sighash));
        out.push(&to_js_bytes(&alphas));
        Ok(out)
    }

    /// Record that the user approved. Only after this can a share exist.
    pub fn approve(&mut self) -> Result<(), JsError> {
        self.inner.approve().map_err(err)
    }

    /// Produce the `SIGN_SHARE` envelope. Consumes the nonces.
    pub fn share(&mut self) -> Result<js_sys::Uint8Array, JsError> {
        let out = self.inner.share(&self.key_package).map_err(err)?;
        Ok(to_js_bytes(&out.encode()))
    }

    pub fn phase(&self) -> String {
        format!("{:?}", self.inner.phase())
    }

    /// `"committed"` / `"consumed"` per action, so the host can persist the nonce state.
    pub fn nonce_states(&self) -> Vec<u8> {
        self.inner
            .nonce_states()
            .iter()
            .map(|s| match s {
                NonceState::Created => 0,
                NonceState::Committed => 1,
                NonceState::Consumed => 2,
                NonceState::Destroyed => 3,
            })
            .collect()
    }
}

/// Keyset setup: `ak` plus the recovery key to a receiving address.
#[wasm_bindgen]
pub struct Keyset;

#[wasm_bindgen]
impl Keyset {
    /// Returns `[fvk(96), ufvk, address, rawAddress(43), fingerprint(32)]`.
    #[wasm_bindgen]
    pub fn derive(
        group_verifying_key: &[u8],
        qsk: &[u8],
        mainnet: bool,
        diversifier_index: u32,
    ) -> Result<js_sys::Array, JsError> {
        let ak: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let qsk: [u8; 32] = qsk
            .try_into()
            .map_err(|_| JsError::new("qsk must be 32 bytes"))?;
        let network = if mainnet {
            zcash_protocol::consensus::Network::MainNetwork
        } else {
            zcash_protocol::consensus::Network::TestNetwork
        };
        let keyset = vault_keyset::VaultKeyset::derive(&ak, &qsk, network, diversifier_index)
            .map_err(err)?;
        let out = js_sys::Array::new();
        out.push(&to_js_bytes(&keyset.fvk));
        out.push(&JsValue::from(keyset.ufvk));
        out.push(&JsValue::from(keyset.address));
        out.push(&to_js_bytes(&keyset.raw_address));
        out.push(&to_js_bytes(&keyset.fingerprint));
        Ok(out)
    }

    /// A fresh `qsk`: the `nk`/`rivk` source, and the ZIP-2005 recovery candidate.
    ///
    /// Generate it **once**, on one device, then deliver it to every other device over a
    /// confidential, authenticated channel — the same one the DKG round-2 packages need. Every
    /// device must pass the identical `qsk` to [`Self::derive`] or they will land on different
    /// addresses for the same group.
    ///
    /// It cannot spend: spend authorization is `ak`, which lives only in the FROST shares. Holding
    /// `qsk` does not weaken the threshold, which is why backing it up is cheap insurance.
    #[wasm_bindgen]
    pub fn generate_qsk() -> js_sys::Uint8Array {
        let mut bytes = [0u8; 32];
        rand_core::RngCore::fill_bytes(&mut rng(), &mut bytes);
        // A spending key must be a canonical scalar; retry rather than shipping a rejected one.
        while orchard::keys::SpendingKey::from_bytes(bytes)
            .into_option()
            .is_none()
        {
            rand_core::RngCore::fill_bytes(&mut rng(), &mut bytes);
        }
        to_js_bytes(&bytes)
    }

    /// Whether the derived address carries only an Orchard receiver. Must always be true; the
    /// frontend should refuse to show an address where it is not.
    #[wasm_bindgen]
    pub fn address_is_shielded_only(address: &str) -> bool {
        use zcash_address::unified::{Address as UnifiedAddress, Encoding};
        match UnifiedAddress::decode(address) {
            Ok((_, _, ua)) => {
                ua.has_receiver_of_type(zcash_protocol::PoolType::ORCHARD)
                    && !ua.has_receiver_of_type(zcash_protocol::PoolType::TRANSPARENT)
                    && !ua.has_receiver_of_type(zcash_protocol::PoolType::SAPLING)
            }
            Err(_) => false,
        }
    }
}

/// PCZT handling the frontend needs around a signing session: shrink the request, read what to sign,
/// put the signatures back.
///
/// Deliberately no proving: building and proving belongs to the wallet's own transaction path, and
/// linking Halo2 here would double the module size for something already present.
#[wasm_bindgen]
pub struct PcztTools;

#[wasm_bindgen]
impl PcztTools {
    /// Fail unless this transaction is one the vault is allowed to sign (V6, Ironwood only, no
    /// transparent or Sapling part).
    #[wasm_bindgen]
    pub fn check(bytes: &[u8]) -> Result<(), JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let shape = pczt_signer_core::gate::PcztShape::of(&pczt);
        pczt_signer_core::gate::check(&shape).map_err(err)
    }

    /// Drop the zk proof: the transport copy. The coordinator keeps its own proven copy for applying
    /// signatures.
    #[wasm_bindgen]
    pub fn strip_proofs(bytes: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let stripped = pczt_signer_core::material::strip_proofs(pczt);
        Ok(to_js_bytes(
            &stripped
                .serialize()
                .map_err(|e| JsError::new(&format!("{e:?}")))?,
        ))
    }

    /// The shielded sighash, recomputed from the PCZT. Signers must use this value, never one handed
    /// to them by the coordinator.
    #[wasm_bindgen]
    pub fn shielded_sighash(bytes: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let sighash = pczt_signer_core::sighash::shielded_sighash(pczt).map_err(err)?;
        Ok(to_js_bytes(&sighash))
    }

    /// Every Ironwood action's `rk`, 32 bytes each, in action order.
    #[wasm_bindgen]
    pub fn action_rks(bytes: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let rks = pczt_signer_core::material::ironwood_action_rks(&pczt).map_err(err)?;
        let mut flat = Vec::with_capacity(rks.len() * 32);
        for rk in &rks {
            flat.extend_from_slice(&rk.rk);
        }
        Ok(to_js_bytes(&flat))
    }

    /// Every Ironwood action's `alpha`, 32 bytes each, in action order. This is what the coordinator
    /// hands to the signers, and what `Verification::rk` needs.
    #[wasm_bindgen]
    pub fn action_alphas(bytes: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let alphas = pczt_signer_core::material::extract_alphas(&pczt).map_err(err)?;
        let mut flat = Vec::with_capacity(alphas.len() * 32);
        for alpha in &alphas {
            flat.extend_from_slice(alpha);
        }
        Ok(to_js_bytes(&flat))
    }

    /// Whether every action's `rk` equals `ak + alpha·G`. Run this on the signer before approving.
    #[wasm_bindgen]
    pub fn check_action_bindings(
        bytes: &[u8],
        group_verifying_key: &[u8],
        alphas: &[u8],
    ) -> Result<(), JsError> {
        let ak: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        if alphas.len() % 32 != 0 {
            return Err(JsError::new("alphas must be a multiple of 32 bytes"));
        }
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let rks = pczt_signer_core::material::ironwood_action_rks(&pczt).map_err(err)?;
        let alphas: Vec<[u8; 32]> = alphas
            .chunks_exact(32)
            .map(|c| c.try_into().expect("chunk size checked"))
            .collect();
        let material = pczt_signer_core::material::bind_alphas(&rks, &alphas).map_err(err)?;
        for m in &material {
            m.verify_rk(&ak).map_err(err)?;
        }
        Ok(())
    }

    /// Write signatures back. `signatures` is 64 bytes per action, positionally matching `indices`.
    #[wasm_bindgen]
    pub fn apply_signatures(
        bytes: &[u8],
        indices: &[u8],
        signatures: &[u8],
    ) -> Result<js_sys::Uint8Array, JsError> {
        if signatures.len() != indices.len() * 64 {
            return Err(JsError::new("expected 64 bytes of signature per index"));
        }
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let pairs: Vec<(u32, [u8; 64])> = indices
            .iter()
            .enumerate()
            .map(|(i, index)| {
                let sig: [u8; 64] = signatures[i * 64..(i + 1) * 64]
                    .try_into()
                    .expect("slice size checked");
                (*index as u32, sig)
            })
            .collect();
        let signed = pczt_signer_core::apply::apply_all(pczt, &pairs).map_err(err)?;
        Ok(to_js_bytes(
            &signed
                .serialize()
                .map_err(|e| JsError::new(&format!("{e:?}")))?,
        ))
    }
}

/// Verification helpers the frontend needs before it broadcasts anything.
#[wasm_bindgen]
pub struct Verification;

#[wasm_bindgen]
impl Verification {
    /// `rk = ak + alpha·G`, as the transaction carries it.
    #[wasm_bindgen]
    pub fn rk(group_verifying_key: &[u8], alpha: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let ak: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let alpha: [u8; 32] = alpha
            .try_into()
            .map_err(|_| JsError::new("alpha must be 32 bytes"))?;
        let rk = threshold::randomized_verifying_key(&ak, &alpha).map_err(err)?;
        Ok(to_js_bytes(&rk))
    }

    /// Check an aggregated spend authorization signature. Run this before broadcasting.
    #[wasm_bindgen]
    pub fn verify_spend_auth(
        group_verifying_key: &[u8],
        alpha: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), JsError> {
        let ak: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let alpha: [u8; 32] = alpha
            .try_into()
            .map_err(|_| JsError::new("alpha must be 32 bytes"))?;
        let signature: [u8; 64] = signature
            .try_into()
            .map_err(|_| JsError::new("signature must be 64 bytes"))?;
        threshold::verify_with_group_key_and_alpha(&ak, &alpha, message, &signature).map_err(err)
    }
}

/// Deposit denomination policy.
#[wasm_bindgen]
pub struct Deposit;

#[wasm_bindgen]
impl Deposit {
    /// Split an amount into canonical outputs. Returns the output values (`Float64Array`) or an error
    /// when the amount cannot be sent.
    #[wasm_bindgen]
    pub fn plan(amount: f64) -> Result<js_sys::Float64Array, JsError> {
        if amount < 0.0 || amount > u64::MAX as f64 {
            return Err(JsError::new("amount out of range"));
        }
        let plan = vault_deposit::plan_deposit(amount as u64).map_err(err)?;
        let out = js_sys::Float64Array::new_with_length(plan.outputs.len() as u32);
        for (i, value) in plan.outputs.iter().enumerate() {
            out.set_index(i as u32, *value as f64);
        }
        Ok(out)
    }

    /// Residual left behind, in zatoshi.
    #[wasm_bindgen]
    pub fn residual(amount: f64) -> Result<f64, JsError> {
        let plan = vault_deposit::plan_deposit(amount as u64).map_err(err)?;
        Ok(plan.residual as f64)
    }

    #[wasm_bindgen]
    pub fn is_canonical(value: f64) -> bool {
        vault_deposit::is_canonical(value as u64)
    }

    #[wasm_bindgen]
    pub fn denom_cap() -> f64 {
        vault_deposit::DENOM_CAP_ZATS as f64
    }

    #[wasm_bindgen]
    pub fn canary_amount() -> f64 {
        vault_deposit::CANARY_AMOUNT_ZATS as f64
    }
}

/// Canary gate. `phase` is one of `"notStarted"`, `"awaitingArrival"`, `"awaitingSpend"`,
/// `"proven"`; `allows` answers whether a deposit may be started now.
#[wasm_bindgen]
pub struct Canary {
    inner: vault_deposit::Canary,
}

#[wasm_bindgen]
impl Canary {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: vault_deposit::Canary::new(),
        }
    }

    pub fn phase(&self) -> String {
        match self.inner.phase() {
            vault_deposit::CanaryPhase::NotStarted => "notStarted".to_string(),
            vault_deposit::CanaryPhase::AwaitingArrival => "awaitingArrival".to_string(),
            vault_deposit::CanaryPhase::AwaitingSpend => "awaitingSpend".to_string(),
            vault_deposit::CanaryPhase::Proven => "proven".to_string(),
        }
    }

    /// Register the small first deposit.
    pub fn begin(&mut self, amount: f64) -> Result<(), JsError> {
        self.inner.begin(amount as u64).map_err(err)
    }

    pub fn arrival_confirmed(&mut self) -> Result<(), JsError> {
        self.inner.arrival_confirmed().map_err(err)
    }

    pub fn spend_confirmed(&mut self) -> Result<(), JsError> {
        self.inner.spend_confirmed().map_err(err)
    }

    pub fn allows(&self, amount: f64) -> bool {
        self.inner.allows(amount as u64)
    }
}

impl Default for Canary {
    fn default() -> Self {
        Self::new()
    }
}

/// Synthetic transactions for the JS test harness. **Not part of the shipping API.**
///
/// Available only when the `test-fixtures` feature is on, which the `test-vault-wasm` target enables.
/// It exists so the frontend's own harness can run the success path — build, sign, apply, verify —
/// without a synced wallet.
#[cfg(feature = "test-fixtures")]
#[wasm_bindgen]
pub struct Fixtures;

#[cfg(feature = "test-fixtures")]
#[wasm_bindgen]
impl Fixtures {
    /// An unproven, unsigned Ironwood PCZT spending from the vault's own address.
    ///
    /// `fvk` is the 96-byte viewing key from [`Keyset::derive`]; `spend` and `pay` are zatoshi.
    #[wasm_bindgen]
    pub fn ironwood_pczt(fvk: &[u8], spend: f64, pay: f64) -> Result<js_sys::Uint8Array, JsError> {
        let fvk: [u8; 96] = fvk
            .try_into()
            .map_err(|_| JsError::new("fvk must be 96 bytes"))?;
        let pczt = vault_fixtures::ironwood_pczt(
            &fvk,
            orchard::ValuePool::Ironwood,
            spend as u64,
            pay as u64,
            rand::rng(),
        )
        .ok_or_else(|| JsError::new("could not build the fixture transaction"))?;
        Ok(to_js_bytes(
            &pczt
                .serialize()
                .map_err(|e| JsError::new(&format!("{e:?}")))?,
        ))
    }
}

/// Which signing phase a session is in, for logging.
#[wasm_bindgen]
pub fn signing_phases() -> js_sys::Array {
    let out = js_sys::Array::new();
    for phase in [
        SignPhase::Created,
        SignPhase::InitSent,
        SignPhase::CommitmentsCollected,
        SignPhase::PcztReady,
        SignPhase::Verifying,
        SignPhase::Approved,
        SignPhase::SharesCollected,
        SignPhase::Aggregated,
    ] {
        out.push(&JsValue::from(format!("{phase:?}")));
    }
    out
}
