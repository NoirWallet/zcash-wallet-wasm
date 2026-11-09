//! Wire envelope for FROST round messages.
//!
//! Why this exists at all, when the Keystone-compatible PCZT envelope already exists in the
//! ecosystem: that envelope opens with `[0x53][0x04][tx_type]` and carries nothing else — no
//! group, session, sender, recipient, round or sequence. It was designed for a single-device
//! air-gap exchange of one request and one response, and it is used here for exactly that (the
//! PCZT bytes riding *inside* this envelope, or over `ur:zcash-pczt` directly). Everything the
//! design doc's §12.3 requires is bound here instead.
//!
//! Layout (little-endian, fixed offsets, no self-describing encoding — "deliberately dumb"):
//!
//! ```text
//! offset  size  field
//!      0     1  version              (currently 1; any other value is refused)
//!      1     1  message type         (unknown values are refused)
//!      2     1  round / phase
//!      3     1  sender roster index  (0..=MAX_PARTICIPANTS-1)
//!      4     4  sequence             (per-session, strictly increasing)
//!      8    16  session id
//!     24     4  payload length
//!     28     n  payload
//! ```
//!
//! 28 bytes of header is under a sixth of one 200-byte QR frame, and the roster index keeps two
//! full public keys (64 bytes) out of every message: identity is established once when the roster
//! is locked, not per message.
//!
//! `session_id` is defined as `H(domain || group_id || roster_hash || session_nonce)`. Binding the
//! session therefore binds the group and the locked participant set, which is how §12.3's group
//! binding is satisfied without spending 32 bytes per message on the group id.
//!
//! Confidentiality and integrity are applied *outside* this encoding: the payload is sealed with
//! an AEAD whose associated data is the 28-byte header, so a message cannot be re-targeted to a
//! different session, round, sender or recipient without failing to open.

pub const VERSION: u8 = 1;

pub const HEADER_LEN: usize = 28;
/// Bound on payload size. The largest thing we ever ship is a redacted PCZT (several KB) or a
/// batch envelope; 1 MiB is far above that and keeps a hostile length field from forcing an
/// allocation.
pub const MAX_PAYLOAD: usize = 1 << 20;

/// The value pool a spend belongs to.
///
/// Only Ironwood exists here. A vault is created fresh, and since NU6.3 the Orchard pool forbids
/// cross-address transfers, so a fee-paying party can only ever create Ironwood outputs for a
/// vault's receiver — a vault never holds an Orchard note. An Orchard-pool action in a vault PCZT
/// is therefore not something to interpret or forward: it is either a bug or a hostile
/// construction, and it is refused by name rather than as an unknown tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValuePool {
    /// Tag 1 on the wire, matching `pczt::orchard::ValuePool`.
    Ironwood,
}

impl ValuePool {
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::Ironwood => 1,
        }
    }

    /// Decode a wire tag. `0` is Orchard, which this protocol deliberately does not carry.
    pub fn from_byte(b: u8) -> Result<Self, PoolError> {
        match b {
            1 => Ok(Self::Ironwood),
            0 => Err(PoolError::OrchardNotSupported),
            other => Err(PoolError::UnknownTag(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolError {
    /// Tag 0: Orchard pool. Impossible in a freshly created vault (see [`ValuePool`]).
    OrchardNotSupported,
    UnknownTag(u8),
}

impl core::fmt::Display for PoolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OrchardNotSupported => f.write_str(
                "orchard-pool action refused: a fresh vault holds no orchard notes and the orchard \
                 pool forbids cross-address transfers",
            ),
            Self::UnknownTag(t) => write!(f, "unknown value pool tag {t}"),
        }
    }
}

impl std::error::Error for PoolError {}

/// Message types. Explicit numbering, stable on the wire, and a hard rejection of anything not
/// listed — an old signer that does not know a type must fail closed rather than guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgType {
    // --- DKG (§12.1) ---
    PairInvite,
    PairResponse,
    RosterLock,
    DkgR1,
    DkgR1Bundle,
    DkgR1Echo,
    DkgR1Cert,
    DkgR2Batch,
    DkgR2Inbox,
    KeysetSetup,
    DkgFinal,
    DkgActivate,
    // --- Signing (§12.2) ---
    SignInit,
    SignCommitments,
    SignPayload,
    SignShare,
    SignReject,
    SignResult,
    // --- Shared ---
    SessionAbort,
}

impl MsgType {
    pub fn to_byte(self) -> u8 {
        use MsgType::*;
        match self {
            PairInvite => 0x01,
            PairResponse => 0x02,
            RosterLock => 0x03,
            DkgR1 => 0x10,
            DkgR1Bundle => 0x11,
            DkgR1Echo => 0x12,
            DkgR1Cert => 0x13,
            DkgR2Batch => 0x14,
            DkgR2Inbox => 0x15,
            KeysetSetup => 0x16,
            DkgFinal => 0x17,
            DkgActivate => 0x18,
            SignInit => 0x20,
            SignCommitments => 0x21,
            SignPayload => 0x22,
            SignShare => 0x23,
            SignReject => 0x24,
            SignResult => 0x25,
            SessionAbort => 0x7f,
        }
    }

    pub fn from_byte(b: u8) -> Option<Self> {
        use MsgType::*;
        Some(match b {
            0x01 => PairInvite,
            0x02 => PairResponse,
            0x03 => RosterLock,
            0x10 => DkgR1,
            0x11 => DkgR1Bundle,
            0x12 => DkgR1Echo,
            0x13 => DkgR1Cert,
            0x14 => DkgR2Batch,
            0x15 => DkgR2Inbox,
            0x16 => KeysetSetup,
            0x17 => DkgFinal,
            0x18 => DkgActivate,
            0x20 => SignInit,
            0x21 => SignCommitments,
            0x22 => SignPayload,
            0x23 => SignShare,
            0x24 => SignReject,
            0x25 => SignResult,
            0x7f => SessionAbort,
            _ => return None,
        })
    }
}

/// A decoded envelope. Owns its payload; the header fields are all the associated data an AEAD
/// needs, so [`Envelope::aad`] reproduces the exact bytes a sender authenticated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    pub msg_type: MsgType,
    pub round: u8,
    pub sender: u8,
    pub sequence: u32,
    pub session_id: [u8; 16],
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeError {
    /// Fewer bytes than a header.
    TooShort,
    /// `version` is not [`VERSION`].
    UnsupportedVersion(u8),
    /// `msg_type` is not a known [`MsgType`].
    UnknownMsgType(u8),
    /// Sender index is not a valid roster slot.
    BadSender(u8),
    /// Declared payload length exceeds [`MAX_PAYLOAD`].
    PayloadTooLarge(u32),
    /// Declared payload length does not match the bytes present.
    LengthMismatch { declared: u32, actual: usize },
}

impl core::fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort => write!(f, "payload shorter than a {HEADER_LEN}-byte header"),
            Self::UnsupportedVersion(v) => {
                write!(f, "unsupported envelope version {v} (expected {VERSION})")
            }
            Self::UnknownMsgType(t) => write!(f, "unknown message type 0x{t:02x}"),
            Self::BadSender(i) => write!(f, "sender roster index {i} out of range"),
            Self::PayloadTooLarge(n) => {
                write!(f, "declared payload length {n} exceeds {MAX_PAYLOAD}")
            }
            Self::LengthMismatch { declared, actual } => {
                write!(
                    f,
                    "declared payload length {declared} but {actual} bytes present"
                )
            }
        }
    }
}

impl std::error::Error for EnvelopeError {}

impl Envelope {
    /// A fresh envelope. `sender` must be a valid roster slot; everything else is the caller's
    /// session bookkeeping.
    pub fn new(
        msg_type: MsgType,
        round: u8,
        sender: u8,
        sequence: u32,
        session_id: [u8; 16],
        payload: Vec<u8>,
    ) -> Result<Self, EnvelopeError> {
        if sender as u16 >= crate::threshold::MAX_PARTICIPANTS {
            return Err(EnvelopeError::BadSender(sender));
        }
        if payload.len() > MAX_PAYLOAD {
            return Err(EnvelopeError::PayloadTooLarge(payload.len() as u32));
        }
        Ok(Self {
            msg_type,
            round,
            sender,
            sequence,
            session_id,
            payload,
        })
    }

    /// The associated data an AEAD must authenticate: header fields with the length set to the
    /// real payload length, which is what [`Envelope::encode`] writes.
    pub fn aad(&self) -> Vec<u8> {
        let mut out = self.encode_header(self.payload.len() as u32);
        out.truncate(HEADER_LEN);
        out
    }

    fn encode_header(&self, payload_len: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + payload_len as usize);
        out.push(VERSION);
        out.push(self.msg_type.to_byte());
        out.push(self.round);
        out.push(self.sender);
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.session_id);
        out.extend_from_slice(&payload_len.to_le_bytes());
        out
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.encode_header(self.payload.len() as u32);
        out.extend_from_slice(&self.payload);
        out
    }

    /// Strict decode: no trailing bytes, no unknown types, no length lies.
    pub fn decode(bytes: &[u8]) -> Result<Self, EnvelopeError> {
        if bytes.len() < HEADER_LEN {
            return Err(EnvelopeError::TooShort);
        }
        let version = bytes[0];
        if version != VERSION {
            return Err(EnvelopeError::UnsupportedVersion(version));
        }
        let msg_type =
            MsgType::from_byte(bytes[1]).ok_or(EnvelopeError::UnknownMsgType(bytes[1]))?;
        let round = bytes[2];
        let sender = bytes[3];
        if sender as u16 >= crate::threshold::MAX_PARTICIPANTS {
            return Err(EnvelopeError::BadSender(sender));
        }
        let sequence = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let mut session_id = [0u8; 16];
        session_id.copy_from_slice(&bytes[8..24]);
        let declared = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        if declared as usize > MAX_PAYLOAD {
            return Err(EnvelopeError::PayloadTooLarge(declared));
        }
        let body = &bytes[HEADER_LEN..];
        if body.len() != declared as usize {
            return Err(EnvelopeError::LengthMismatch {
                declared,
                actual: body.len(),
            });
        }
        Ok(Self {
            msg_type,
            round,
            sender,
            sequence,
            session_id,
            payload: body.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Envelope {
        Envelope::new(
            MsgType::SignCommitments,
            2,
            1,
            7,
            [9u8; 16],
            vec![1, 2, 3, 4],
        )
        .unwrap()
    }

    #[test]
    fn roundtrips() {
        let env = sample();
        assert_eq!(Envelope::decode(&env.encode()).unwrap(), env);
    }

    #[test]
    fn header_is_twenty_eight_bytes() {
        let env = sample();
        assert_eq!(env.encode().len(), HEADER_LEN + 4);
        assert_eq!(env.aad().len(), HEADER_LEN);
    }

    #[test]
    fn rejects_short_input() {
        assert_eq!(Envelope::decode(&[0u8; 5]), Err(EnvelopeError::TooShort));
    }

    #[test]
    fn rejects_wrong_version() {
        let mut bytes = sample().encode();
        bytes[0] = 2;
        assert_eq!(
            Envelope::decode(&bytes),
            Err(EnvelopeError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn rejects_unknown_msg_type() {
        let mut bytes = sample().encode();
        bytes[1] = 0x99;
        assert_eq!(
            Envelope::decode(&bytes),
            Err(EnvelopeError::UnknownMsgType(0x99))
        );
    }

    #[test]
    fn rejects_out_of_roster_sender() {
        let mut bytes = sample().encode();
        bytes[3] = 3; // MAX_PARTICIPANTS is 3, so index 3 is out of range
        assert_eq!(Envelope::decode(&bytes), Err(EnvelopeError::BadSender(3)));
    }

    #[test]
    fn rejects_length_lies() {
        let mut bytes = sample().encode();
        bytes[24] = 9; // claim nine payload bytes, only four present
        assert_eq!(
            Envelope::decode(&bytes),
            Err(EnvelopeError::LengthMismatch {
                declared: 9,
                actual: 4
            })
        );
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = sample().encode();
        bytes.push(0);
        assert_eq!(
            Envelope::decode(&bytes),
            Err(EnvelopeError::LengthMismatch {
                declared: 4,
                actual: 5
            })
        );
    }

    #[test]
    fn ironwood_is_the_only_pool() {
        assert_eq!(ValuePool::Ironwood.to_byte(), 1);
        assert_eq!(ValuePool::from_byte(1), Ok(ValuePool::Ironwood));
        // Orchard is refused by name, not treated as an unknown tag: it is a decision, not a gap.
        assert_eq!(ValuePool::from_byte(0), Err(PoolError::OrchardNotSupported));
        assert_eq!(ValuePool::from_byte(9), Err(PoolError::UnknownTag(9)));
    }
}
