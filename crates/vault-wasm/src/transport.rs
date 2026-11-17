//! Transport helpers: roster manifest, Noise identity and sealed payloads.

use super::*;

/// The Noise XX channel used under BLE.
///
/// WebCrypto has no BLAKE2s, so this cannot be written in the extension: a hand-rolled Noise state
/// machine in JavaScript would be a third implementation of a security-critical protocol. GATT reads and
/// writes stay in the host — this only turns plaintext into bytes and back.
///
/// XX is three messages: initiator writes, responder replies, initiator finishes. After that both sides
/// can `encrypt`/`decrypt`.
///
/// **Message 1's payload is plaintext.** Noise has no key before the first DH, so nothing that matters may
/// be carried there; the initiator's static key is only revealed and proved in message 3. Use message 1
/// for nothing, or for data you would be happy to see forged.
#[wasm_bindgen]
pub struct NoiseXx {
    inner: noise::NoiseChannel,
}

#[wasm_bindgen]
impl NoiseXx {
    /// The X25519 public key belonging to a static private key.
    #[wasm_bindgen]
    pub fn static_public_from(private_key: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let private: [u8; 32] = private_key.try_into().map_err(|_| JsError::new("static private key must be 32 bytes"))?;
        Ok(to_js_bytes(&noise::static_public_from(&private)))
    }

    /// Start the channel as the connecting side.
    #[wasm_bindgen]
    pub fn initiator(static_private: &[u8], prologue: &[u8]) -> Result<NoiseXx, JsError> {
        Ok(Self {
            inner: noise::NoiseChannel::initiator(static_private, prologue).map_err(err)?,
        })
    }

    /// Start the channel as the accepting side.
    #[wasm_bindgen]
    pub fn responder(static_private: &[u8], prologue: &[u8]) -> Result<NoiseXx, JsError> {
        Ok(Self {
            inner: noise::NoiseChannel::responder(static_private, prologue).map_err(err)?,
        })
    }

    /// Write the next handshake message for this side.
    pub fn write_handshake(&mut self, payload: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        self.inner.write_handshake(payload).map(|bytes| to_js_bytes(&bytes)).map_err(err)
    }

    /// Read a handshake message from the peer, returning its payload.
    pub fn read_handshake(&mut self, message: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        self.inner.read_handshake(message).map(|bytes| to_js_bytes(&bytes)).map_err(err)
    }

    pub fn is_handshake_finished(&self) -> bool {
        self.inner.is_handshake_finished()
    }

    /// Whether the channel is dead after an authentication failure.
    ///
    /// Noise advances the nonce before verifying the tag, so one bad message leaves the two sides out of
    /// step permanently. When this is true the host must drop the channel and handshake again — every
    /// later call returns an error.
    pub fn is_poisoned(&self) -> bool {
        self.inner.is_poisoned()
    }

    /// The handshake hash. Both ends must agree; unavailable until the handshake completes.
    pub fn handshake_hash(&self) -> Result<js_sys::Uint8Array, JsError> {
        self.inner.handshake_hash().map(|hash| to_js_bytes(&hash)).map_err(err)
    }

    /// The peer's static public key, once the handshake has revealed it.
    pub fn remote_static(&self) -> Option<js_sys::Uint8Array> {
        self.inner.remote_static().map(|key| to_js_bytes(&key))
    }

    /// Encrypt one payload. Completes the handshake first if it is finished but not yet converted.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        self.inner.encrypt(plaintext).map(|bytes| to_js_bytes(&bytes)).map_err(err)
    }

    /// Decrypt one payload from the peer.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        self.inner.decrypt(ciphertext).map(|bytes| to_js_bytes(&bytes)).map_err(err)
    }

    /// Rotate the sending and receiving keys, as Noise's `rekey` specifies.
    pub fn rekey(&mut self) -> Result<(), JsError> {
        self.inner.rekey().map_err(err)
    }
}

/// The roster hash, computed here so no other language keeps its own copy of the encoding.
///
/// The wire format is fixed by agreement with the mobile client; the domain tag is concatenated raw,
/// every field carries a 4-byte big-endian length, and numbers are canonical decimal text. A different
/// encoding produces a different hash, and two devices that disagree on the roster hash will refuse to
/// proceed — which is the point, but makes a silent drift expensive to debug.
#[wasm_bindgen]
pub struct RosterManifest {
    inner: roster::RosterManifest,
}

#[wasm_bindgen]
impl RosterManifest {
    #[wasm_bindgen(constructor)]
    pub fn new(group_id: &str, protocol_version: u32, ciphersuite: &str, threshold: u16, participants: u16) -> Result<RosterManifest, JsError> {
        Ok(RosterManifest {
            inner: roster::RosterManifest::new(group_id, protocol_version, ciphersuite, threshold, participants).map_err(err)?,
        })
    }

    /// Add a participant. Call order does not matter: the encoding sorts by slot.
    pub fn add_participant(&mut self, slot: u8, identity_public_key: &[u8], transport_public_key: &[u8]) -> Result<(), JsError> {
        self.inner.add_participant(slot, identity_public_key, transport_public_key).map_err(err)
    }

    /// The exact bytes that are hashed. For test vectors and debugging.
    pub fn canonical_bytes(&self) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(&self.inner.canonical_bytes().map_err(err)?))
    }

    /// `SHA-256(canonical_bytes)`.
    pub fn hash(&self) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(&self.inner.hash().map_err(err)?))
    }

    /// Lowercase hex of the hash, for comparing against the agreed vectors.
    pub fn hash_hex(&self) -> Result<String, JsError> {
        self.inner.hash_hex().map_err(err)
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
        let pairs: Vec<(u8, Vec<u8>)> = statics.inner.iter().map(|(slot, key)| (*slot, key.clone())).collect();
        if pairs.is_empty() {
            return Err(JsError::new("prologue needs at least one participant"));
        }
        Ok(to_js_bytes(&seal::ceremony_prologue(&pairs)))
    }

    /// The prologue with the round-1 transcript folded in: the roster plus what each participant said.
    ///
    /// Use this instead of `prologue` once round-1 broadcasts exist. A coordinator that relays a different
    /// round-1 set to different devices then cannot have its round-2 packages opened, because each device
    /// seals under the transcript it was actually shown.
    #[wasm_bindgen(js_name = prologueWithTranscript)]
    pub fn prologue_with_transcript(statics: &MessageSet, round1: &MessageSet) -> Result<js_sys::Uint8Array, JsError> {
        let participants: Vec<(u8, Vec<u8>)> = statics.inner.iter().map(|(slot, key)| (*slot, key.clone())).collect();
        if participants.is_empty() {
            return Err(JsError::new("prologue needs at least one participant"));
        }
        let transcript: Vec<(u8, Vec<u8>)> = round1.inner.iter().map(|(slot, bytes)| (*slot, bytes.clone())).collect();
        Ok(to_js_bytes(&seal::ceremony_prologue_with_transcript(&participants, &transcript)))
    }

    /// Seal `plaintext` so only the holder of `recipient_public` can read it.
    #[wasm_bindgen]
    pub fn seal(sender_secret: &[u8], recipient_public: &[u8], prologue: &[u8], plaintext: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let sealed = seal::seal(sender_secret, recipient_public, prologue, plaintext).map_err(err)?;
        Ok(to_js_bytes(&sealed))
    }

    /// Open a message from `sender_public`. Fails if the pair, the ceremony or the bytes are wrong.
    #[wasm_bindgen]
    pub fn open(recipient_secret: &[u8], sender_public: &[u8], prologue: &[u8], ciphertext: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let opened = seal::open(recipient_secret, sender_public, prologue, ciphertext).map_err(err)?;
        Ok(to_js_bytes(&opened))
    }
}
