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

impl MsgType {
    /// The `round` this message type belongs to, as agreed with the mobile client.
    ///
    /// Round 1 is the commitment round; round 2 is the coordinator building the PCZT locally and is not
    /// a message; round 3 is the signing round. A message carrying a different round is refused, so the
    /// field is a checked part of the protocol rather than a label.
    /// Whether this type belongs to a signing session.
    ///
    /// Signing messages carry the session's per-round payloads; the DKG types are shaped differently and
    /// go through their own helpers.
    pub const fn is_signing(self) -> bool {
        use MsgType::*;
        matches!(self, SignInit | SignCommitments | SignPayload | SignShare | SignReject | SignResult)
    }

    pub const fn round(self) -> u8 {
        use MsgType::*;
        match self {
            // DKG: the ceremony's own rounds.
            DkgR1 | DkgR1Bundle | DkgR1Echo | DkgR1Cert => 1,
            DkgR2Batch | DkgR2Inbox => 2,
            // Pairing and roster happen before any round.
            PairInvite | PairResponse | RosterLock => 0,
            // Finalisation messages belong to the ceremony's last round. These four values were wrong here
            // for a while (2, 2, 2 and 0), which only mattered once `round_of` exposed the table to callers
            // that fill the field themselves; the spec's message table (6.1) is the authority.
            KeysetSetup | DkgFinal | DkgActivate | SessionAbort => 3,
            // Signing.
            SignInit | SignCommitments => 1,
            SignPayload | SignShare | SignReject | SignResult => 3,
        }
    }
}

// ---------------------------------------------------------------------------
// Rejection and result payloads
//
// Small, fixed layouts so the signer and the coordinator cannot disagree about them; both sides are
// already exchanging envelopes with these types, and an undefined payload would be invented per client.
// ---------------------------------------------------------------------------

/// Why a signer refused to produce a share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectCode {
    /// The user declined.
    UserDeclined,
    /// This signer's own verification of the PCZT failed.
    VerificationFailed,
    /// The payload could not be parsed, or was for another session.
    Malformed,
    /// The signing session had already expired.
    Expired,
}

impl RejectCode {
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::UserDeclined => 1,
            Self::VerificationFailed => 2,
            Self::Malformed => 3,
            Self::Expired => 4,
        }
    }

    pub const fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Self::UserDeclined,
            2 => Self::VerificationFailed,
            3 => Self::Malformed,
            4 => Self::Expired,
            _ => return None,
        })
    }
}

/// Outcome the coordinator reports back to the signers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultStatus {
    /// Aggregated and broadcast.
    Broadcasted,
    /// Aggregation or broadcast failed.
    Failed,
}

impl ResultStatus {
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::Broadcasted => 0,
            Self::Failed => 1,
        }
    }

    pub const fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Broadcasted,
            1 => Self::Failed,
            _ => return None,
        })
    }
}

/// Longest human-readable note these payloads will carry. They are for a UI line, not for data.
pub const MAX_NOTE: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadError {
    /// The code byte is not one this build knows.
    UnknownCode(u8),
    /// Truncated, over-long, or carrying trailing bytes.
    Malformed,
    /// A `Broadcasted` result without a transaction id, or a `Failed` one with one.
    ResultShape,
}

impl core::fmt::Display for PayloadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownCode(c) => write!(f, "unknown status code {c}"),
            Self::Malformed => f.write_str("malformed payload"),
            Self::ResultShape => f.write_str("result status and transaction id disagree"),
        }
    }
}

impl std::error::Error for PayloadError {}

fn put_note(out: &mut Vec<u8>, note: &str) -> Result<(), PayloadError> {
    let bytes = note.as_bytes();
    if bytes.len() > MAX_NOTE {
        return Err(PayloadError::Malformed);
    }
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn take_note(bytes: &[u8], pos: &mut usize) -> Result<String, PayloadError> {
    let end = pos.checked_add(2).ok_or(PayloadError::Malformed)?;
    if end > bytes.len() {
        return Err(PayloadError::Malformed);
    }
    let len = u16::from_le_bytes([bytes[*pos], bytes[*pos + 1]]) as usize;
    *pos = end;
    if len > MAX_NOTE {
        return Err(PayloadError::Malformed);
    }
    let end = pos.checked_add(len).ok_or(PayloadError::Malformed)?;
    if end > bytes.len() {
        return Err(PayloadError::Malformed);
    }
    let note = core::str::from_utf8(&bytes[*pos..end]).map_err(|_| PayloadError::Malformed)?;
    *pos = end;
    Ok(note.to_string())
}

/// `code:u8 || len:u16 || utf8 note`
pub fn encode_reject(code: RejectCode, note: &str) -> Result<Vec<u8>, PayloadError> {
    let mut out = Vec::with_capacity(3 + note.len());
    out.push(code.to_byte());
    put_note(&mut out, note)?;
    Ok(out)
}

pub fn decode_reject(payload: &[u8]) -> Result<(RejectCode, String), PayloadError> {
    let code = *payload.first().ok_or(PayloadError::Malformed)?;
    let code = RejectCode::from_byte(code).ok_or(PayloadError::UnknownCode(code))?;
    let mut pos = 1;
    let note = take_note(payload, &mut pos)?;
    if pos != payload.len() {
        return Err(PayloadError::Malformed);
    }
    Ok((code, note))
}

/// `status:u8 || txid:32 || len:u16 || utf8 note`, with the transaction id present only on success.
pub fn encode_result(status: ResultStatus, txid: Option<[u8; 32]>, note: &str) -> Result<Vec<u8>, PayloadError> {
    match (status, txid.is_some()) {
        (ResultStatus::Broadcasted, false) | (ResultStatus::Failed, true) => return Err(PayloadError::ResultShape),
        _ => {}
    }
    let mut out = Vec::with_capacity(35 + note.len());
    out.push(status.to_byte());
    out.extend_from_slice(&txid.unwrap_or([0u8; 32]));
    put_note(&mut out, note)?;
    Ok(out)
}

pub fn decode_result(payload: &[u8]) -> Result<(ResultStatus, Option<[u8; 32]>, String), PayloadError> {
    if payload.len() < 35 {
        return Err(PayloadError::Malformed);
    }
    let status = ResultStatus::from_byte(payload[0]).ok_or(PayloadError::UnknownCode(payload[0]))?;
    let mut txid = [0u8; 32];
    txid.copy_from_slice(&payload[1..33]);
    let mut pos = 33;
    let note = take_note(payload, &mut pos)?;
    if pos != payload.len() {
        return Err(PayloadError::Malformed);
    }
    let txid = match status {
        ResultStatus::Broadcasted => Some(txid),
        ResultStatus::Failed => None,
    };
    Ok((status, txid, note))
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
/// A signing-session message could not be built or read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigningError {
    Envelope(EnvelopeError),
    Payload(PayloadError),
    /// The type does not belong to a signing session, so these helpers do not apply.
    NotSigning(MsgType),
    /// The envelope names another ceremony.
    WrongSession,
    /// The envelope names another sender.
    WrongSender {
        expected: u8,
        got: u8,
    },
    /// The envelope carries a round other than the one its type is fixed to.
    WrongRound {
        msg_type: MsgType,
        expected: u8,
        got: u8,
    },
}

impl std::fmt::Display for SigningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Envelope(e) => write!(f, "envelope: {e}"),
            Self::Payload(e) => write!(f, "payload: {e}"),
            Self::NotSigning(msg_type) => write!(f, "{msg_type:?} is not a signing message"),
            Self::WrongSession => f.write_str("the envelope belongs to another ceremony"),
            Self::WrongSender { expected, got } => {
                write!(f, "the envelope is from slot {got}, expected {expected}")
            }
            Self::WrongRound { msg_type, expected, got } => write!(f, "{msg_type:?} is fixed to round {expected}, got {got}"),
        }
    }
}

impl std::error::Error for SigningError {}

impl From<EnvelopeError> for SigningError {
    fn from(e: EnvelopeError) -> Self {
        Self::Envelope(e)
    }
}

impl From<PayloadError> for SigningError {
    fn from(e: PayloadError) -> Self {
        Self::Payload(e)
    }
}

/// Build an envelope for a signing-session message.
///
/// The round is taken from the message type rather than passed in. That is the whole point of this
/// constructor: the round table is a cross-implementation contract, so a caller cannot get it wrong by
/// forgetting to set a field — the value is derived from the only thing that determines it.
pub fn signing_envelope(msg_type: MsgType, session_id: [u8; 16], sender: u8, recipient: u8, sequence: u32, payload: Vec<u8>) -> Result<Vec<u8>, SigningError> {
    if !msg_type.is_signing() {
        return Err(SigningError::NotSigning(msg_type));
    }
    Ok(Envelope::new(msg_type, msg_type.round(), sender, recipient, sequence, session_id, payload)?.encode())
}

/// Read a signing-session envelope, refusing a type outside the signing set and a round that disagrees
/// with its type.
///
/// The sender, session and sequence are *not* checked here: those belong to the caller's session state,
/// which is the only place that knows what it has already accepted.
pub fn read_signing_envelope(bytes: &[u8]) -> Result<Envelope, SigningError> {
    let envelope = Envelope::decode(bytes)?;
    if !envelope.msg_type.is_signing() {
        return Err(SigningError::NotSigning(envelope.msg_type));
    }
    let expected = envelope.msg_type.round();
    if envelope.round != expected {
        return Err(SigningError::WrongRound {
            msg_type: envelope.msg_type,
            expected,
            got: envelope.round,
        });
    }
    Ok(envelope)
}

/// Check a *relayed* envelope — one this device did not receive directly — against everything the header
/// can prove: the ceremony, the sender, the type, and the round that type fixes.
///
/// Relaying is where a coordinator can substitute one participant's message for another's, so the fields
/// that say "who said this, in which ceremony" must be checked against the roster and the session rather
/// than taken from the envelope. Signature verification is deliberately not here: it belongs with the
/// roster's identity keys, which this crate does not hold.
pub fn check_relayed(bytes: &[u8], session_id: [u8; 16], sender: u8, msg_type: MsgType) -> Result<Envelope, SigningError> {
    let envelope = Envelope::decode(bytes)?;
    if envelope.session_id != session_id {
        return Err(SigningError::WrongSession);
    }
    if envelope.sender != sender {
        return Err(SigningError::WrongSender {
            expected: sender,
            got: envelope.sender,
        });
    }
    if envelope.msg_type != msg_type {
        return Err(SigningError::NotSigning(envelope.msg_type));
    }
    let expected = msg_type.round();
    if envelope.round != expected {
        return Err(SigningError::WrongRound {
            msg_type,
            expected,
            got: envelope.round,
        });
    }
    Ok(envelope)
}

/// Build a `SIGN_REJECT` envelope.
pub fn encode_sign_reject(session_id: [u8; 16], sender: u8, recipient: u8, sequence: u32, code: RejectCode, note: &str) -> Result<Vec<u8>, SigningError> {
    signing_envelope(MsgType::SignReject, session_id, sender, recipient, sequence, encode_reject(code, note)?)
}

/// Read a `SIGN_REJECT` envelope and its payload.
pub fn decode_sign_reject(bytes: &[u8]) -> Result<(Envelope, RejectCode, String), SigningError> {
    let envelope = read_signing_envelope(bytes)?;
    if envelope.msg_type != MsgType::SignReject {
        return Err(SigningError::NotSigning(envelope.msg_type));
    }
    let (code, note) = decode_reject(&envelope.payload)?;
    Ok((envelope, code, note))
}

/// Build a `SIGN_RESULT` envelope.
pub fn encode_sign_result(
    session_id: [u8; 16],
    sender: u8,
    recipient: u8,
    sequence: u32,
    status: ResultStatus,
    txid: Option<[u8; 32]>,
    note: &str,
) -> Result<Vec<u8>, SigningError> {
    signing_envelope(MsgType::SignResult, session_id, sender, recipient, sequence, encode_result(status, txid, note)?)
}

/// Read a `SIGN_RESULT` envelope and its payload.
pub fn decode_sign_result(bytes: &[u8]) -> Result<(Envelope, ResultStatus, Option<[u8; 32]>, String), SigningError> {
    let envelope = read_signing_envelope(bytes)?;
    if envelope.msg_type != MsgType::SignResult {
        return Err(SigningError::NotSigning(envelope.msg_type));
    }
    let (status, txid, note) = decode_result(&envelope.payload)?;
    Ok((envelope, status, txid, note))
}

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
