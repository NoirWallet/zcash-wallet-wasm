//! The Noise channel that sits under the BLE transport.
//!
//! `Noise_XX_25519_ChaChaPoly_BLAKE2s`: XX because neither side has the other's static key before the
//! handshake, and BLAKE2s because that is what the rest of the Zcash Noise usage picks. `snow` is the
//! same library — and the same pure-Rust resolver — that already seals our DKG round-2 packages.
//!
//! It lives here rather than in the browser deliberately: WebCrypto has no BLAKE2s, and a hand-written
//! Noise state machine in JavaScript would be a third implementation of a security-critical protocol.
//!
//! Layering note: this is cryptography, not I/O. GATT reads and writes stay in the host; this only turns
//! plaintext into the bytes to write and back.
//!
//! **Prologue.** Both ends must pass the same value, and it is a fixed constant for the channel's whole
//! lifetime rather than something keyed by a session id: the session id is chosen after the handshake,
//! so binding it into the prologue would be circular, and XX's fresh ephemerals already make a recorded
//! transcript useless in another handshake.

use snow::params::NoiseParams;

/// Agreed with the mobile client. Changing it breaks every existing pairing.
pub const PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Agreed prologue for this channel.
pub const PROLOGUE: &[u8] = b"noir/pair/v1";

/// Noise's own message ceiling (`MAXMSGLEN`), including the 16-byte tag.
pub const MAX_MESSAGE: usize = 65535;

/// Largest plaintext that still fits in one Noise message.
pub const MAX_PLAINTEXT: usize = MAX_MESSAGE - 16;

#[derive(Debug, PartialEq, Eq)]
pub enum NoiseError {
    /// The pattern, the keys, or the prologue were rejected.
    Setup,
    /// A handshake message did not authenticate, or arrived in the wrong order.
    Handshake,
    /// A transport message did not authenticate.
    Decrypt,
    /// The message or payload exceeded what one Noise message can carry.
    TooLarge,
    /// The operation needs a completed handshake.
    NotEstablished,
    /// No handshake message is expected from this side right now.
    NotOurTurn,
    /// The channel is dead: a message failed to authenticate, so the nonce counters no longer agree.
    /// The host must drop it and handshake again.
    Poisoned,
}

impl core::fmt::Display for NoiseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Setup => f.write_str("noise parameters or keys rejected"),
            Self::Handshake => f.write_str("handshake message did not authenticate"),
            Self::Decrypt => f.write_str("transport message did not authenticate"),
            Self::TooLarge => f.write_str("message exceeds one noise packet"),
            Self::NotEstablished => f.write_str("handshake is not complete"),
            Self::NotOurTurn => f.write_str("no handshake message is expected from this side"),
            Self::Poisoned => f.write_str("channel is dead after an authentication failure; handshake again"),
        }
    }
}

impl std::error::Error for NoiseError {}

fn params() -> Result<NoiseParams, NoiseError> {
    PATTERN.parse().map_err(|_| NoiseError::Setup)
}

/// The X25519 public key belonging to a static private key.
///
/// The host stores the private half and needs the public half for the roster, so this must agree with
/// what `snow` derives internally — both go through `x25519-dalek`, so they do.
pub fn static_public_from(private: &[u8; 32]) -> [u8; 32] {
    let secret = x25519_dalek::StaticSecret::from(*private);
    x25519_dalek::PublicKey::from(&secret).to_bytes()
}

/// A fresh static keypair for the channel.
pub fn generate_keypair() -> Result<([u8; 32], [u8; 32]), NoiseError> {
    let builder = snow::Builder::new(params()?);
    let keypair = builder.generate_keypair().map_err(|_| NoiseError::Setup)?;
    let private: [u8; 32] = keypair.private.as_slice().try_into().map_err(|_| NoiseError::Setup)?;
    let public: [u8; 32] = keypair.public.as_slice().try_into().map_err(|_| NoiseError::Setup)?;
    Ok((private, public))
}

enum State {
    Handshake(Box<snow::HandshakeState>),
    Transport(Box<snow::TransportState>),
}

/// One end of a Noise XX channel.
pub struct NoiseChannel {
    state: State,
    /// Set once a message fails to authenticate. Noise advances the nonce before it knows whether the
    /// tag verifies, so a failure leaves the two sides out of step for ever: the only correct move is to
    /// refuse all further use and re-handshake. Enforcing that here means a caller cannot accidentally
    /// keep talking on a broken channel.
    poisoned: bool,
    /// Captured when the handshake completes, because `into_transport_mode` consumes the state that
    /// owns these values.
    handshake_hash: Option<[u8; 32]>,
}

impl NoiseChannel {
    fn build(private: &[u8], prologue: &[u8], initiator: bool) -> Result<Self, NoiseError> {
        Self::build_with_ephemeral(private, prologue, initiator, None)
    }

    fn build_with_ephemeral(private: &[u8], prologue: &[u8], initiator: bool, fixed_ephemeral: Option<&[u8]>) -> Result<Self, NoiseError> {
        if private.len() != 32 {
            return Err(NoiseError::Setup);
        }
        let builder = snow::Builder::new(params()?).local_private_key(private).prologue(prologue);
        let builder = match fixed_ephemeral {
            Some(ephemeral) => builder.fixed_ephemeral_key_for_testing_only(ephemeral),
            None => builder,
        };
        let handshake = if initiator { builder.build_initiator() } else { builder.build_responder() }.map_err(|_| NoiseError::Setup)?;
        Ok(Self {
            state: State::Handshake(Box::new(handshake)),
            handshake_hash: None,
            poisoned: false,
        })
    }

    /// Start a channel as the connecting side.
    pub fn initiator(static_private: &[u8], prologue: &[u8]) -> Result<Self, NoiseError> {
        Self::build(static_private, prologue, true)
    }

    /// Start a channel as the accepting side.
    pub fn responder(static_private: &[u8], prologue: &[u8]) -> Result<Self, NoiseError> {
        Self::build(static_private, prologue, false)
    }

    pub fn is_handshake_finished(&self) -> bool {
        match &self.state {
            State::Handshake(hs) => hs.is_handshake_finished(),
            State::Transport(_) => true,
        }
    }

    /// Write the next handshake message for this side. XX is three messages: initiator, responder,
    /// initiator.
    pub fn write_handshake(&mut self, payload: &[u8]) -> Result<Vec<u8>, NoiseError> {
        self.ensure_live()?;
        if payload.len() > MAX_PLAINTEXT {
            return Err(NoiseError::TooLarge);
        }
        match &mut self.state {
            State::Handshake(hs) => {
                let mut buf = vec![0u8; MAX_MESSAGE];
                let len = hs.write_message(payload, &mut buf).map_err(|_| {
                    self.poisoned = true;
                    NoiseError::Handshake
                })?;
                buf.truncate(len);
                Ok(buf)
            }
            State::Transport(_) => Err(NoiseError::NotOurTurn),
        }
    }

    /// Read a handshake message from the peer and return its payload.
    pub fn read_handshake(&mut self, message: &[u8]) -> Result<Vec<u8>, NoiseError> {
        self.ensure_live()?;
        if message.len() > MAX_MESSAGE {
            return Err(NoiseError::TooLarge);
        }
        match &mut self.state {
            State::Handshake(hs) => {
                let mut buf = vec![0u8; MAX_MESSAGE];
                let len = hs.read_message(message, &mut buf).map_err(|_| {
                    self.poisoned = true;
                    NoiseError::Handshake
                })?;
                buf.truncate(len);
                Ok(buf)
            }
            State::Transport(_) => Err(NoiseError::NotEstablished),
        }
    }

    /// The peer's static public key, once the handshake has revealed it: after message 3 for the
    /// responder (the initiator's static key is encrypted into message 3), after message 2 for the
    /// initiator (the responder's).
    pub fn remote_static(&self) -> Option<Vec<u8>> {
        match &self.state {
            State::Handshake(hs) => hs.get_remote_static().map(|k| k.to_vec()),
            State::Transport(ts) => ts.get_remote_static().map(|k| k.to_vec()),
        }
    }

    /// The handshake hash. Both ends must agree; it is only available once the handshake is complete.
    pub fn handshake_hash(&self) -> Result<[u8; 32], NoiseError> {
        if let Some(hash) = self.handshake_hash {
            return Ok(hash);
        }
        match &self.state {
            State::Handshake(hs) if hs.is_handshake_finished() => hs.get_handshake_hash().try_into().map_err(|_| NoiseError::Setup),
            _ => Err(NoiseError::NotEstablished),
        }
    }

    /// Move to transport mode. Called automatically by `encrypt`/`decrypt` once the handshake is done.
    pub fn finish_handshake(&mut self) -> Result<(), NoiseError> {
        self.into_transport()
    }

    fn ensure_live(&self) -> Result<(), NoiseError> {
        if self.poisoned {
            return Err(NoiseError::Poisoned);
        }
        Ok(())
    }

    fn into_transport(&mut self) -> Result<(), NoiseError> {
        let State::Handshake(hs) = &self.state else {
            return Ok(());
        };
        if !hs.is_handshake_finished() {
            return Err(NoiseError::NotEstablished);
        }
        let hash: [u8; 32] = hs.get_handshake_hash().try_into().map_err(|_| NoiseError::Setup)?;
        // `into_transport_mode` consumes the handshake state, so take ownership of it first.
        let placeholder = State::Handshake(Box::new(
            snow::Builder::new(params()?)
                .local_private_key(&[0u8; 32])
                .prologue(&[])
                .build_initiator()
                .map_err(|_| NoiseError::Setup)?,
        ));
        let state = core::mem::replace(&mut self.state, placeholder);
        let State::Handshake(hs) = state else {
            unreachable!("we just checked the variant")
        };
        let transport = hs.into_transport_mode().map_err(|_| NoiseError::Handshake)?;
        self.state = State::Transport(Box::new(transport));
        self.handshake_hash = Some(hash);
        Ok(())
    }

    /// Encrypt one payload for the peer. Completes the handshake first if it is finished but has not been
    /// converted yet.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        self.ensure_live()?;
        if plaintext.len() > MAX_PLAINTEXT {
            return Err(NoiseError::TooLarge);
        }
        self.into_transport()?;
        match &mut self.state {
            State::Transport(ts) => {
                let mut buf = vec![0u8; MAX_MESSAGE];
                let len = ts.write_message(plaintext, &mut buf).map_err(|_| NoiseError::Decrypt)?;
                buf.truncate(len);
                Ok(buf)
            }
            State::Handshake(_) => Err(NoiseError::NotEstablished),
        }
    }

    /// Decrypt one payload from the peer.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        self.ensure_live()?;
        if ciphertext.len() > MAX_MESSAGE {
            return Err(NoiseError::TooLarge);
        }
        self.into_transport()?;
        match &mut self.state {
            State::Transport(ts) => {
                let mut buf = vec![0u8; MAX_MESSAGE];
                let len = ts.read_message(ciphertext, &mut buf).map_err(|_| {
                    // Noise advances the nonce before checking the tag, so the two sides can never
                    // resynchronise. Refuse every later operation rather than let the host keep talking
                    // on a channel that can no longer work.
                    self.poisoned = true;
                    NoiseError::Decrypt
                })?;
                buf.truncate(len);
                Ok(buf)
            }
            State::Handshake(_) => Err(NoiseError::NotEstablished),
        }
    }

    /// Whether the channel is dead after an authentication failure.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    /// Rotate the sending and receiving keys as Noise's `rekey` specifies.
    pub fn rekey(&mut self) -> Result<(), NoiseError> {
        self.ensure_live()?;
        match &mut self.state {
            State::Transport(ts) => {
                ts.rekey_outgoing();
                ts.rekey_incoming();
                Ok(())
            }
            State::Handshake(_) => Err(NoiseError::NotEstablished),
        }
    }
}

/// Handshake constructors that accept a caller-supplied ephemeral key.
///
/// Only for producing interop vectors: with the ephemeral pinned, a handshake becomes a pure function of
/// its inputs, so two implementations can be compared byte for byte. Production code must let `snow`
/// generate ephemerals from the system RNG.
#[cfg(feature = "test-vectors")]
impl NoiseChannel {
    pub fn initiator_with_ephemeral(static_private: &[u8], prologue: &[u8], ephemeral_private: &[u8]) -> Result<Self, NoiseError> {
        Self::build_with_ephemeral(static_private, prologue, true, Some(ephemeral_private))
    }

    pub fn responder_with_ephemeral(static_private: &[u8], prologue: &[u8], ephemeral_private: &[u8]) -> Result<Self, NoiseError> {
        Self::build_with_ephemeral(static_private, prologue, false, Some(ephemeral_private))
    }
}

/// The timing above is behaviour, not commentary: message 1 carries no static key at all.
#[cfg(all(test, feature = "test-vectors"))]
mod timing_tests {
    use super::*;

    #[test]
    fn the_responder_learns_the_initiator_static_only_after_message_3() {
        let mut initiator = NoiseChannel::initiator_with_ephemeral(&[0x11; 32], b"noir/pair/v1", &[0x33; 32]).unwrap();
        let mut responder = NoiseChannel::responder_with_ephemeral(&[0x22; 32], b"noir/pair/v1", &[0x44; 32]).unwrap();
        let msg1 = initiator.write_handshake(b"").unwrap();
        responder.read_handshake(&msg1).unwrap();
        assert!(responder.remote_static().is_none(), "message 1 carries no static key");
        let msg2 = responder.write_handshake(b"").unwrap();
        initiator.read_handshake(&msg2).unwrap();
        assert!(initiator.remote_static().is_some(), "the initiator learns it from message 2");
        assert!(responder.remote_static().is_none(), "still unknown to the responder");
        let msg3 = initiator.write_handshake(b"").unwrap();
        responder.read_handshake(&msg3).unwrap();
        assert!(responder.remote_static().is_some(), "message 3 reveals it");
    }
}
