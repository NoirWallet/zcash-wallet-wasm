//! Wire envelope, message batching and message-type lookups.

use super::*;

/// A set of messages indexed by peer slot.
///
/// The frontend fills this with whatever its transport received, and reads it back after a round.
/// Exists so the wire format of a message batch stays inside the wasm.
#[wasm_bindgen]
pub struct MessageSet {
    pub(super) inner: BTreeMap<u8, Vec<u8>>,
}

#[wasm_bindgen]
impl MessageSet {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { inner: BTreeMap::new() }
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
        Ok(to_js_bytes(&protocol::pack_batch(&self.inner).map_err(err)?))
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
    pub fn build(msg_type: u8, round: u8, sender: u8, recipient: u8, sequence: u32, session_id: &[u8], payload: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let session_id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        let msg_type = MsgType::from_byte(msg_type).ok_or_else(|| JsError::new("unknown msg_type"))?;
        let env = Envelope::new(msg_type, round, sender, recipient, sequence, session_id, payload.to_vec()).map_err(err)?;
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

    /// Build a signing-session envelope; the round comes from the message type.
    ///
    /// Use this instead of `build` for signing messages: `build` takes the round as an argument, which is
    /// how a caller ends up sending a round the receiver refuses.
    #[wasm_bindgen(js_name = buildSigning)]
    pub fn build_signing(msg_type: u8, sender: u8, recipient: u8, sequence: u32, session_id: &[u8], payload: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let session_id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        let msg_type = MsgType::from_byte(msg_type).ok_or_else(|| JsError::new("unknown msg_type"))?;
        let bytes = protocol::signing_envelope(msg_type, session_id, sender, recipient, sequence, payload.to_vec()).map_err(err)?;
        Ok(to_js_bytes(&bytes))
    }

    /// Read a signing-session envelope, refusing a type outside the signing set and a round that disagrees
    /// with its type. Returns the same shape as `decode`.
    #[wasm_bindgen(js_name = readSigning)]
    pub fn read_signing(bytes: &[u8]) -> Result<js_sys::Array, JsError> {
        let env = protocol::read_signing_envelope(bytes).map_err(err)?;
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

    /// Build a `SIGN_REJECT` envelope. `code`: 1 user declined, 2 verification failed, 3 malformed, 4 expired.
    #[wasm_bindgen(js_name = buildSignReject)]
    pub fn build_sign_reject(session_id: &[u8], sender: u8, recipient: u8, sequence: u32, code: u8, note: &str) -> Result<js_sys::Uint8Array, JsError> {
        let session_id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        let code = RejectCode::from_byte(code).ok_or_else(|| JsError::new("unknown reject code"))?;
        let bytes = protocol::encode_sign_reject(session_id, sender, recipient, sequence, code, note).map_err(err)?;
        Ok(to_js_bytes(&bytes))
    }

    /// Decode a `SIGN_REJECT` envelope into `[sender, sequence, sessionId, code, note]`.
    #[wasm_bindgen(js_name = readSignReject)]
    pub fn read_sign_reject(bytes: &[u8]) -> Result<js_sys::Array, JsError> {
        let (envelope, code, note) = protocol::decode_sign_reject(bytes).map_err(err)?;
        let out = js_sys::Array::new();
        out.push(&JsValue::from(envelope.sender));
        out.push(&JsValue::from(envelope.sequence));
        out.push(&to_js_bytes(&envelope.session_id));
        out.push(&JsValue::from(code.to_byte()));
        out.push(&JsValue::from(note));
        Ok(out)
    }

    /// Build a `SIGN_RESULT` envelope. `status`: 0 broadcasted (txid required), 1 failed (no txid).
    #[wasm_bindgen(js_name = buildSignResult)]
    pub fn build_sign_result(
        session_id: &[u8],
        sender: u8,
        recipient: u8,
        sequence: u32,
        status: u8,
        txid: Option<Vec<u8>>,
        note: &str,
    ) -> Result<js_sys::Uint8Array, JsError> {
        let session_id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        let status = ResultStatus::from_byte(status).ok_or_else(|| JsError::new("unknown result status"))?;
        let txid = match txid {
            None => None,
            Some(bytes) => Some(<[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| JsError::new("txid must be 32 bytes"))?),
        };
        let bytes = protocol::encode_sign_result(session_id, sender, recipient, sequence, status, txid, note).map_err(err)?;
        Ok(to_js_bytes(&bytes))
    }

    /// Decode a `SIGN_RESULT` envelope into `[sender, sequence, sessionId, status, txid|null, note]`.
    #[wasm_bindgen(js_name = readSignResult)]
    pub fn read_sign_result(bytes: &[u8]) -> Result<js_sys::Array, JsError> {
        let (envelope, status, txid, note) = protocol::decode_sign_result(bytes).map_err(err)?;
        let out = js_sys::Array::new();
        out.push(&JsValue::from(envelope.sender));
        out.push(&JsValue::from(envelope.sequence));
        out.push(&to_js_bytes(&envelope.session_id));
        out.push(&JsValue::from(status.to_byte()));
        out.push(&match txid {
            None => JsValue::NULL,
            Some(txid) => to_js_bytes(&txid).into(),
        });
        out.push(&JsValue::from(note));
        Ok(out)
    }

    /// Wire value of a reject code, by the name the phone uses.
    #[wasm_bindgen(js_name = rejectCode)]
    pub fn reject_code(name: &str) -> Result<u8, JsError> {
        Ok(match name {
            "userDeclined" => RejectCode::UserDeclined,
            "verificationFailed" => RejectCode::VerificationFailed,
            "malformed" => RejectCode::Malformed,
            "expired" => RejectCode::Expired,
            other => return Err(JsError::new(&format!("unknown reject code {other}"))),
        }
        .to_byte())
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
                Some(_) => Err(JsError::new(&format!("{name} is reserved: its payload format is not defined yet"))),
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

    /// The `round` a message type belongs to, per the spec's message table.
    ///
    /// The value is enforced only inside a session: `readSigning` and the signing sessions refuse an
    /// envelope whose round disagrees with its type. Nothing checks the DKG rounds, so this is a lookup
    /// for callers that fill the field themselves — a wrong value produces a message the peer refuses,
    /// not a local error.
    #[wasm_bindgen]
    pub fn round_of(name: &str) -> Result<u8, JsError> {
        let msg_type = defined_msg_type(name)
            .or_else(|| reserved_msg_type(name))
            .ok_or_else(|| JsError::new(&format!("unknown message type {name}")))?;
        Ok(msg_type.round())
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
    "signReject",
    "signResult",
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
        "signReject" => MsgType::SignReject,
        "signResult" => MsgType::SignResult,
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
        "sessionAbort" => MsgType::SessionAbort,
        _ => return None,
    })
}
