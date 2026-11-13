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
//!      3     1  sender slot          (roster index, or COORDINATOR_SLOT)
//!      4     1  recipient slot       (roster index, or COORDINATOR_SLOT)
//!      5     4  sequence             (per-sender, strictly increasing)
//!      9    16  session id
//!     25     4  payload length
//!     29     n  payload
//! ```
//!
//! 29 bytes of header is under a sixth of one 200-byte QR frame, and the slot byte pair keeps two
//! full public keys (64 bytes) out of every message: identity is established once when the roster
//! is locked, not per message. Both slots are bound because §12.3 requires it: a message addressed
//! to one signer must not be usable by another.
//!
//! The coordinator is not necessarily a key holder (§8.3's `B+C` recovery path), so it cannot be
//! given a roster index. [`COORDINATOR_SLOT`] is reserved for it, which keeps "who sent this" total
//! without inventing a second envelope shape.
//!
//! `session_id` is defined as `H(domain || group_id || roster_hash || session_nonce)`. Binding the
//! session therefore binds the group and the locked participant set, which is how §12.3's group
//! binding is satisfied without spending 32 bytes per message on the group id.
//!
//! Confidentiality and integrity are applied *outside* this encoding: the payload is sealed with
//! an AEAD whose associated data is the 28-byte header, so a message cannot be re-targeted to a
//! different session, round, sender or recipient without failing to open.

pub const VERSION: u8 = 1;

pub const HEADER_LEN: usize = 29;

/// Reserved slot for the coordinator, which may hold no key share (§8.3 `B+C` recovery).
pub const COORDINATOR_SLOT: u8 = 0xFE;
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
    pub recipient: u8,
    pub sequence: u32,
    pub session_id: [u8; 16],
    pub payload: Vec<u8>,
}

/// A slot is either a roster index or the coordinator's reserved slot.
fn valid_slot(b: u8) -> bool {
    (b as u16) < crate::threshold::MAX_PARTICIPANTS || b == COORDINATOR_SLOT
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeError {
    /// Fewer bytes than a header.
    TooShort,
    /// `version` is not [`VERSION`].
    UnsupportedVersion(u8),
    /// `msg_type` is not a known [`MsgType`].
    UnknownMsgType(u8),
    /// A slot byte is neither a roster index nor [`COORDINATOR_SLOT`].
    BadSlot(u8),
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
            Self::BadSlot(i) => write!(f, "slot byte 0x{i:02x} is neither a roster index nor the coordinator"),
            Self::PayloadTooLarge(n) => {
                write!(f, "declared payload length {n} exceeds {MAX_PAYLOAD}")
            }
            Self::LengthMismatch { declared, actual } => {
                write!(f, "declared payload length {declared} but {actual} bytes present")
            }
        }
    }
}

impl std::error::Error for EnvelopeError {}

impl Envelope {
    /// A fresh envelope. `sender` must be a valid roster slot; everything else is the caller's
    /// session bookkeeping.
    pub fn new(msg_type: MsgType, round: u8, sender: u8, recipient: u8, sequence: u32, session_id: [u8; 16], payload: Vec<u8>) -> Result<Self, EnvelopeError> {
        if !valid_slot(sender) {
            return Err(EnvelopeError::BadSlot(sender));
        }
        if !valid_slot(recipient) {
            return Err(EnvelopeError::BadSlot(recipient));
        }
        if payload.len() > MAX_PAYLOAD {
            return Err(EnvelopeError::PayloadTooLarge(payload.len() as u32));
        }
        Ok(Self {
            msg_type,
            round,
            sender,
            recipient,
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
        out.push(self.recipient);
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
        let msg_type = MsgType::from_byte(bytes[1]).ok_or(EnvelopeError::UnknownMsgType(bytes[1]))?;
        let round = bytes[2];
        let sender = bytes[3];
        let recipient = bytes[4];
        if !valid_slot(sender) {
            return Err(EnvelopeError::BadSlot(sender));
        }
        if !valid_slot(recipient) {
            return Err(EnvelopeError::BadSlot(recipient));
        }
        let sequence = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
        let mut session_id = [0u8; 16];
        session_id.copy_from_slice(&bytes[9..25]);
        let declared = u32::from_le_bytes([bytes[25], bytes[26], bytes[27], bytes[28]]);
        if declared as usize > MAX_PAYLOAD {
            return Err(EnvelopeError::PayloadTooLarge(declared));
        }
        let body = &bytes[HEADER_LEN..];
        if body.len() != declared as usize {
            return Err(EnvelopeError::LengthMismatch { declared, actual: body.len() });
        }
        Ok(Self {
            msg_type,
            round,
            sender,
            recipient,
            sequence,
            session_id,
            payload: body.to_vec(),
        })
    }
}

// ---------------------------------------------------------------------------
// Message batches
//
// DKG rounds are all-to-all, so one envelope usually carries several small payloads: every peer's
// round-1 broadcast, or every peer's round-2 package. The batch format below is the payload of such
// an envelope, and it is deliberately as strict as the envelope itself.
// ---------------------------------------------------------------------------

/// Largest number of entries a batch may carry: one per roster slot.
pub const MAX_BATCH: usize = crate::threshold::MAX_PARTICIPANTS as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchError {
    /// More entries than there are roster slots.
    TooMany(usize),
    /// Slot byte outside the roster.
    BadSlot(u8),
    /// Two entries for one slot.
    Duplicate(u8),
    /// Length field disagrees with the bytes present, or the encoding is truncated.
    Malformed,
}

impl core::fmt::Display for BatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooMany(n) => write!(f, "{n} entries exceeds the roster size"),
            Self::BadSlot(s) => write!(f, "slot {s} is outside the roster"),
            Self::Duplicate(s) => write!(f, "slot {s} appears twice"),
            Self::Malformed => f.write_str("malformed message batch"),
        }
    }
}

impl std::error::Error for BatchError {}

/// `count:u8 || count * (slot:u8 || len:u16 || bytes)`, slots ascending.
pub fn pack_batch(messages: &std::collections::BTreeMap<u8, Vec<u8>>) -> Result<Vec<u8>, BatchError> {
    if messages.len() > MAX_BATCH {
        return Err(BatchError::TooMany(messages.len()));
    }
    let mut out = Vec::new();
    out.push(messages.len() as u8);
    for (slot, bytes) in messages {
        if (*slot as usize) >= MAX_BATCH {
            return Err(BatchError::BadSlot(*slot));
        }
        let len = u16::try_from(bytes.len()).map_err(|_| BatchError::Malformed)?;
        out.push(*slot);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(bytes);
    }
    Ok(out)
}

/// Strict inverse of [`pack_batch`]: no trailing bytes, no duplicates, bounded count.
pub fn unpack_batch(bytes: &[u8]) -> Result<std::collections::BTreeMap<u8, Vec<u8>>, BatchError> {
    let mut out = std::collections::BTreeMap::new();
    let mut pos = 0usize;
    let take = |pos: &mut usize, n: usize| -> Result<&[u8], BatchError> {
        let end = pos.checked_add(n).ok_or(BatchError::Malformed)?;
        if end > bytes.len() {
            return Err(BatchError::Malformed);
        }
        let slice = &bytes[*pos..end];
        *pos = end;
        Ok(slice)
    };
    let count = take(&mut pos, 1)?[0] as usize;
    if count > MAX_BATCH {
        return Err(BatchError::TooMany(count));
    }
    for _ in 0..count {
        let slot = take(&mut pos, 1)?[0];
        if (slot as usize) >= MAX_BATCH {
            return Err(BatchError::BadSlot(slot));
        }
        let len_bytes = take(&mut pos, 2)?;
        let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
        let payload = take(&mut pos, len)?.to_vec();
        if out.insert(slot, payload).is_some() {
            return Err(BatchError::Duplicate(slot));
        }
    }
    if pos != bytes.len() {
        return Err(BatchError::Malformed);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Envelope {
        Envelope::new(MsgType::SignCommitments, 2, 1, COORDINATOR_SLOT, 7, [9u8; 16], vec![1, 2, 3, 4]).unwrap()
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
        assert_eq!(Envelope::decode(&bytes), Err(EnvelopeError::UnsupportedVersion(2)));
    }

    #[test]
    fn rejects_unknown_msg_type() {
        let mut bytes = sample().encode();
        bytes[1] = 0x99;
        assert_eq!(Envelope::decode(&bytes), Err(EnvelopeError::UnknownMsgType(0x99)));
    }

    #[test]
    fn accepts_the_coordinator_slot() {
        let env = Envelope::new(MsgType::SignInit, 1, COORDINATOR_SLOT, 2, 1, [3u8; 16], vec![]).unwrap();
        assert_eq!(Envelope::decode(&env.encode()).unwrap(), env);
    }

    #[test]
    fn rejects_out_of_roster_sender() {
        let mut bytes = sample().encode();
        bytes[3] = 3; // MAX_PARTICIPANTS is 3, so index 3 is out of range
        assert_eq!(Envelope::decode(&bytes), Err(EnvelopeError::BadSlot(3)));
    }

    #[test]
    fn rejects_length_lies() {
        let mut bytes = sample().encode();
        bytes[25] = 9; // claim nine payload bytes, only four present
        assert_eq!(Envelope::decode(&bytes), Err(EnvelopeError::LengthMismatch { declared: 9, actual: 4 }));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = sample().encode();
        bytes.push(0);
        assert_eq!(Envelope::decode(&bytes), Err(EnvelopeError::LengthMismatch { declared: 4, actual: 5 }));
    }

    #[test]
    fn ironwood_is_the_only_pool() {
        assert_eq!(ValuePool::Ironwood.to_byte(), 1);
        assert_eq!(ValuePool::from_byte(1), Ok(ValuePool::Ironwood));
        // Orchard is refused by name, not treated as an unknown tag: it is a decision, not a gap.
        assert_eq!(ValuePool::from_byte(0), Err(PoolError::OrchardNotSupported));
        assert_eq!(ValuePool::from_byte(9), Err(PoolError::UnknownTag(9)));
    }

    #[test]
    fn a_batch_round_trips() {
        let mut messages = std::collections::BTreeMap::new();
        messages.insert(0u8, vec![1, 2, 3]);
        messages.insert(2u8, vec![]);
        let packed = super::pack_batch(&messages).unwrap();
        assert_eq!(super::unpack_batch(&packed).unwrap(), messages);
    }

    #[test]
    fn a_batch_is_strict() {
        use super::{pack_batch, unpack_batch, BatchError};
        let mut messages = std::collections::BTreeMap::new();
        messages.insert(0u8, vec![9, 9]);
        let packed = pack_batch(&messages).unwrap();

        // Trailing byte.
        let mut longer = packed.clone();
        longer.push(0);
        assert_eq!(unpack_batch(&longer), Err(BatchError::Malformed));
        // Truncation.
        assert_eq!(unpack_batch(&packed[..packed.len() - 1]), Err(BatchError::Malformed));
        // Slot outside the roster.
        assert_eq!(unpack_batch(&[1u8, 7, 0, 0]), Err(BatchError::BadSlot(7)));
        // Duplicate slot.
        assert_eq!(unpack_batch(&[2u8, 0, 1, 0, 7, 0, 1, 0, 8]), Err(BatchError::Duplicate(0)));
        // Empty batch is legal (a round with nothing to forward).
        assert_eq!(unpack_batch(&[0u8]).unwrap().len(), 0);
    }
}
