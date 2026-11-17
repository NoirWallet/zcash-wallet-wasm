//! DKG rounds: the per-round objects the host relays between devices.

use super::*;

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

    /// This device's static public key. Publish it; peers seal later ceremony messages to it.
    pub fn static_public(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.static_public)
    }

    /// The broadcast wrapped in a `DKGR1` envelope addressed to the coordinator.
    ///
    /// Use this instead of shipping the raw bytes: the envelope is what binds the payload to this
    /// session, this sender and this round, and what lets the relay reject replays.
    pub fn broadcast_envelope(&self, session_id: &[u8], sequence: u32, my_slot: u8) -> Result<js_sys::Uint8Array, JsError> {
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

    /// Every participant's static public key, by slot, including this device's — learned from the
    /// round-1 broadcasts. This is what KEYSET_SETUP seals to; there is no need to parse the broadcast
    /// framing to get it.
    pub fn statics(&self) -> MessageSet {
        MessageSet {
            inner: self.inner.statics.clone(),
        }
    }

    /// The whole batch wrapped in a `DKGR2BATCH` envelope addressed to the coordinator.
    pub fn outgoing_envelope(&self, session_id: &[u8], sequence: u32, my_slot: u8) -> Result<js_sys::Uint8Array, JsError> {
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

    /// Every participant's static public key, by slot, including this device's own.
    pub fn statics(&self) -> MessageSet {
        MessageSet {
            inner: self.inner.statics.clone(),
        }
    }

    /// The matching static secret, for sealing later ceremony messages (KEYSET_SETUP) to peers.
    ///
    /// Keep it on the device. It cannot spend; it only lets peers send this device a message nobody
    /// else can read.
    pub fn static_secret(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.static_secret)
    }
}

/// One device's participation in a distributed key generation.
#[wasm_bindgen]
pub struct Dkg {
    participants: u16,
    threshold: u16,
    slot: u8,
    session_id: [u8; 16],
}

#[wasm_bindgen]
impl Dkg {
    #[wasm_bindgen(constructor)]
    pub fn new(participants: u16, threshold: u16, slot: u8, session_id: &[u8]) -> Result<Dkg, JsError> {
        if slot as u16 >= participants {
            return Err(JsError::new("slot is outside the roster"));
        }
        threshold::slot_identifier(slot).map_err(err)?;
        let session_id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        Ok(Self {
            participants,
            threshold,
            slot,
            session_id,
        })
    }

    pub fn slot(&self) -> u8 {
        self.slot
    }

    /// Round 1: generate this device's contribution.
    ///
    /// The contribution is bound to this ceremony's session id: a later round refuses a broadcast that
    /// names a different one, so a round-1 secret cannot be carried into a second ceremony.
    pub fn part1(&self) -> Result<Dkg1, JsError> {
        let inner = threshold::dkg_part1(self.session_id, self.participants, self.threshold, self.slot, &mut rng()).map_err(err)?;
        Ok(Dkg1 { inner })
    }

    /// Round 2: seal one package per peer from everyone's broadcasts.
    pub fn part2(&self, secret: &[u8], broadcasts: &MessageSet) -> Result<Dkg2, JsError> {
        let peers = strip_self(&broadcasts.inner, self.slot);
        let inner = threshold::dkg_part2(secret, &peers).map_err(err)?;
        Ok(Dkg2 { inner })
    }

    /// The static public key carried inside a round-1 broadcast.
    ///
    /// A coordinator that handles one device at a time has each broadcast as bytes and needs the
    /// sender's key in order to seal KEYSET_SETUP to it. Use this rather than taking the framing apart.
    #[wasm_bindgen]
    pub fn static_public_of_broadcast(broadcast: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(&threshold::static_public_of_broadcast(broadcast).map_err(err)?))
    }

    /// Round 3: finalize with the broadcasts and the packages addressed to this device.
    pub fn part3(&self, secret: &[u8], broadcasts: &MessageSet, incoming: &MessageSet) -> Result<Dkg3, JsError> {
        let peers = strip_self(&broadcasts.inner, self.slot);
        let inner = threshold::dkg_part3(secret, &peers, &incoming.inner).map_err(err)?;
        Ok(Dkg3 { inner })
    }
}

fn dkg_envelope(msg_type: MsgType, round: u8, sender: u8, recipient: u8, sequence: u32, session_id: &[u8], payload: Vec<u8>) -> Result<Vec<u8>, JsError> {
    let session_id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
    let envelope = Envelope::new(msg_type, round, sender, recipient, sequence, session_id, payload).map_err(err)?;
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
        let session_id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
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
        let want = MsgType::from_byte(EnvelopeCodec::msg_type(expected)?).ok_or_else(|| JsError::new("unknown message type"))?;
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
    pub fn bundle(&mut self, round: u8, recipient: u8, messages: &MessageSet) -> Result<js_sys::Uint8Array, JsError> {
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

fn strip_self(all: &BTreeMap<u8, Vec<u8>>, me: u8) -> BTreeMap<u8, Vec<u8>> {
    all.iter()
        .filter(|(slot, _)| **slot != me)
        .map(|(slot, bytes)| (*slot, bytes.clone()))
        .collect()
}
