//! The signing session: envelope + FROST rounds wired into one state machine.
//!
//! Before this module, [`crate::protocol`] and [`crate::threshold`] were separate pieces that
//! happened to live in the same crate: nothing forced a message to be bound to a session, and
//! nothing stopped a nonce set from reaching two different signing packages. Here they are joined,
//! so the dangerous states stop being representable:
//!
//! * a share cannot exist before this signer's [`SignPhase::Approved`];
//! * nonces are per action and are consumed on use, so signing twice with one set fails;
//! * every message is checked against the session id, the recipient slot, the expected message type
//!   for the current phase, the sender, and a strictly increasing per-sender sequence;
//! * aggregation is only reachable once every selected signer has returned a share for every
//!   action.
//!
//! What this module deliberately does *not* know: how a PCZT is built, what the sighash means, and
//! whether an `alpha` corresponds to the transaction. Building belongs to the wallet tier; the
//! `rk == ak + alpha·G` check belongs to `pczt-signer-core`. The coordinator therefore takes
//! `(sighash, alphas)` as opaque inputs, and the signer hands them back to its caller to verify
//! before [`SignerSession::approve`].

use std::collections::BTreeMap;

use rand_core::{CryptoRng, RngCore};

use crate::protocol::{Envelope, EnvelopeError, MsgType, COORDINATOR_SLOT};
use crate::state::{NonceState, SignPhase, TransitionError};
use crate::threshold::{
    self, commit, Identifier, KeyPackage, PublicKeyPackage, SignatureShare, SigningCommitments,
    SigningNonces,
};

/// Length of a session id on the wire.
pub const SESSION_ID_LEN: usize = 16;

/// Upper bound on actions per signing session.
///
/// The wire field is a `u16`, so a hostile payload could claim 65535 and make a decoder allocate for
/// all of them. 96 is the per-round action budget a hardware signer already works to (and a vault
/// transfer is one to three actions), so anything beyond it is not a vault transaction.
pub const MAX_ACTIONS: usize = 96;

/// Whether an action count from the wire is usable.
fn valid_actions(n: usize) -> bool {
    n > 0 && n <= MAX_ACTIONS
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// Message belongs to another session.
    WrongSession,
    /// Message is addressed to a different slot.
    WrongRecipient(u8),
    /// Message type is not the one this phase expects.
    WrongPhase { expected: MsgType, got: MsgType },
    /// The session is not in a phase this operation may run in.
    WrongStage(SignPhase),
    /// Sender is not a selected signer, or this device is not in the roster.
    UnknownSender(u8),
    /// That sender already sent for this step.
    Duplicate(u8),
    /// Sequence did not advance: a replay, or an out-of-order delivery.
    SequenceRewind { sender: u8, last: u32, got: u32 },
    /// Payload could not be decoded.
    Malformed,
    /// Envelope layer refused it.
    Envelope(EnvelopeError),
    /// Phase transition refused.
    State(TransitionError),
    /// Action / alpha / signer counts disagree.
    Counts { actions: usize, alphas: usize },
    /// Not everyone has sent what this step waits for.
    Incomplete { have: usize, need: usize },
    /// A signer was asked for a share before approval.
    NotApproved,
    /// FROST refused an operation.
    Frost(String),
}

impl core::fmt::Display for SessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WrongSession => f.write_str("message belongs to another session"),
            Self::WrongRecipient(r) => write!(f, "message is addressed to slot 0x{r:02x}"),
            Self::WrongPhase { expected, got } => write!(f, "expected {expected:?}, got {got:?}"),
            Self::WrongStage(p) => write!(f, "operation not allowed in phase {p:?}"),
            Self::UnknownSender(i) => write!(f, "slot {i} is not a selected signer"),
            Self::Duplicate(i) => write!(f, "slot {i} already sent for this step"),
            Self::SequenceRewind { sender, last, got } => write!(
                f,
                "sender {sender} replayed or reordered messages ({got} after {last})"
            ),
            Self::Malformed => f.write_str("malformed payload"),
            Self::Envelope(e) => write!(f, "envelope: {e}"),
            Self::State(e) => write!(f, "state: {e}"),
            Self::Counts { actions, alphas } => {
                write!(f, "{alphas} alpha(s) for {actions} action(s)")
            }
            Self::Incomplete { have, need } => write!(f, "{have} of {need} collected"),
            Self::NotApproved => f.write_str("this signer has not approved the transaction"),
            Self::Frost(e) => write!(f, "frost: {e}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<EnvelopeError> for SessionError {
    fn from(e: EnvelopeError) -> Self {
        Self::Envelope(e)
    }
}

impl From<TransitionError> for SessionError {
    fn from(e: TransitionError) -> Self {
        Self::State(e)
    }
}

impl From<threshold::ThresholdError> for SessionError {
    fn from(e: threshold::ThresholdError) -> Self {
        Self::Frost(e.to_string())
    }
}

fn frost<E: core::fmt::Debug>(e: E) -> SessionError {
    SessionError::Frost(format!("{e:?}"))
}

// ---------------------------------------------------------------------------
// Payload codec: fixed little-endian layouts, every length checked on decode.
// ---------------------------------------------------------------------------

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) -> Result<(), SessionError> {
    let len = u16::try_from(b.len()).map_err(|_| SessionError::Malformed)?;
    put_u16(out, len);
    out.extend_from_slice(b);
    Ok(())
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], SessionError> {
        let end = self.pos.checked_add(n).ok_or(SessionError::Malformed)?;
        if end > self.buf.len() {
            return Err(SessionError::Malformed);
        }
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, SessionError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, SessionError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn array32(&mut self) -> Result<[u8; 32], SessionError> {
        self.take(32)?
            .try_into()
            .map_err(|_| SessionError::Malformed)
    }

    fn bytes(&mut self) -> Result<&'a [u8], SessionError> {
        let n = self.u16()? as usize;
        self.take(n)
    }

    /// No trailing bytes: leftover input means sender and receiver disagree on the layout.
    fn finish(&self) -> Result<(), SessionError> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(SessionError::Malformed)
        }
    }
}

/// Per-signer, per-action commitment sets.
type CommitmentGrid = BTreeMap<u8, Vec<SigningCommitments>>;

struct DecodedPayload {
    sighash: [u8; 32],
    alphas: Vec<[u8; 32]>,
    /// One commitment map per action, keyed by roster slot.
    per_action: Vec<BTreeMap<u8, SigningCommitments>>,
}

fn decode_payload(payload: &[u8]) -> Result<DecodedPayload, SessionError> {
    let mut r = Reader::new(payload);
    let sighash = r.array32()?;
    let actions = r.u16()? as usize;
    if !valid_actions(actions) {
        return Err(SessionError::Counts {
            actions: MAX_ACTIONS,
            alphas: actions,
        });
    }
    let mut alphas = Vec::with_capacity(actions);
    let mut per_action = Vec::with_capacity(actions);
    for _ in 0..actions {
        alphas.push(r.array32()?);
        let n = r.u8()? as usize;
        if n > crate::threshold::MAX_PARTICIPANTS as usize {
            return Err(SessionError::UnknownSender(n as u8));
        }
        let mut grid = BTreeMap::new();
        for _ in 0..n {
            let slot = r.u8()?;
            if slot as u16 >= crate::threshold::MAX_PARTICIPANTS {
                return Err(SessionError::UnknownSender(slot));
            }
            let commitments = SigningCommitments::deserialize(r.bytes()?).map_err(frost)?;
            grid.insert(slot, commitments);
        }
        per_action.push(grid);
    }
    r.finish()?;
    Ok(DecodedPayload {
        sighash,
        alphas,
        per_action,
    })
}

// ---------------------------------------------------------------------------
// Coordinator
// ---------------------------------------------------------------------------

/// Who is in this session and who the coordinator selected.
#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub session_id: [u8; SESSION_ID_LEN],
    /// Total participants in the locked roster (N).
    pub participants: u16,
    /// Threshold (t).
    pub threshold: u16,
    /// Roster indices taking part in this signing: exactly `threshold` of them.
    pub signers: Vec<u8>,
    /// FROST identifier for each roster index, recorded when the roster was locked.
    pub identifiers: BTreeMap<u8, Identifier>,
}

impl SessionConfig {
    pub fn validate(&self) -> Result<(), SessionError> {
        if self.signers.len() != self.threshold as usize {
            return Err(SessionError::Incomplete {
                have: self.signers.len(),
                need: self.threshold as usize,
            });
        }
        for slot in &self.signers {
            if *slot as u16 >= self.participants || !self.identifiers.contains_key(slot) {
                return Err(SessionError::UnknownSender(*slot));
            }
        }
        Ok(())
    }
}

/// Coordinator-side session. Holds no key share, and needs none: the same type serves a device
/// that also owns a share (2-of-2) and a pure relay (§8.3's `B+C` recovery path).
pub struct CoordinatorSession {
    cfg: SessionConfig,
    pubkeys: PublicKeyPackage,
    phase: SignPhase,
    actions: usize,
    seq: u32,
    grid: CommitmentGrid,
    sighash: Option<[u8; 32]>,
    alphas: Vec<[u8; 32]>,
    shares: BTreeMap<u8, Vec<SignatureShare>>,
    last_seq: BTreeMap<u8, u32>,
}

impl CoordinatorSession {
    pub fn new(
        cfg: SessionConfig,
        pubkeys: PublicKeyPackage,
        actions: usize,
    ) -> Result<Self, SessionError> {
        cfg.validate()?;
        if !valid_actions(actions) {
            return Err(SessionError::Counts {
                actions: MAX_ACTIONS,
                alphas: actions,
            });
        }
        Ok(Self {
            cfg,
            pubkeys,
            phase: SignPhase::Created,
            actions,
            seq: 0,
            grid: BTreeMap::new(),
            sighash: None,
            alphas: Vec::new(),
            shares: BTreeMap::new(),
            last_seq: BTreeMap::new(),
        })
    }

    pub fn phase(&self) -> SignPhase {
        self.phase
    }

    pub fn actions(&self) -> usize {
        self.actions
    }

    fn next_seq(&mut self) -> u32 {
        self.seq = self.seq.wrapping_add(1);
        self.seq
    }

    /// `SIGN_INIT` to every selected signer.
    pub fn init(&mut self) -> Result<Vec<Envelope>, SessionError> {
        if self.phase != SignPhase::Created {
            return Err(SessionError::WrongStage(self.phase));
        }
        let mut payload = Vec::new();
        put_u16(&mut payload, self.actions as u16);
        put_u16(&mut payload, self.cfg.participants);
        put_u16(&mut payload, self.cfg.threshold);
        payload.push(u8::try_from(self.cfg.signers.len()).map_err(|_| SessionError::Malformed)?);
        payload.extend_from_slice(&self.cfg.signers);

        self.phase.advance()?; // Created -> InitSent
        let mut out = Vec::with_capacity(self.cfg.signers.len());
        for slot in self.cfg.signers.clone() {
            let seq = self.next_seq();
            out.push(Envelope::new(
                MsgType::SignInit,
                1,
                COORDINATOR_SLOT,
                slot,
                seq,
                self.cfg.session_id,
                payload.clone(),
            )?);
        }
        Ok(out)
    }

    /// Shared inbound validation: session, recipient, roster membership, type, monotone sequence.
    fn accept(&mut self, env: &Envelope, expected: MsgType) -> Result<u8, SessionError> {
        if env.session_id != self.cfg.session_id {
            return Err(SessionError::WrongSession);
        }
        if env.recipient != COORDINATOR_SLOT {
            return Err(SessionError::WrongRecipient(env.recipient));
        }
        if env.msg_type != expected {
            return Err(SessionError::WrongPhase {
                expected,
                got: env.msg_type,
            });
        }
        if !self.cfg.signers.contains(&env.sender) {
            return Err(SessionError::UnknownSender(env.sender));
        }
        if let Some(last) = self.last_seq.get(&env.sender) {
            if env.sequence <= *last {
                return Err(SessionError::SequenceRewind {
                    sender: env.sender,
                    last: *last,
                    got: env.sequence,
                });
            }
        }
        self.last_seq.insert(env.sender, env.sequence);
        Ok(env.sender)
    }

    /// `SIGN_COMMITMENTS` from one signer: one commitment set per action.
    pub fn on_commitments(&mut self, env: &Envelope) -> Result<(), SessionError> {
        let sender = self.accept(env, MsgType::SignCommitments)?;
        if self.phase != SignPhase::InitSent {
            return Err(SessionError::WrongStage(self.phase));
        }
        if self.grid.contains_key(&sender) {
            return Err(SessionError::Duplicate(sender));
        }
        let mut r = Reader::new(&env.payload);
        let count = r.u16()? as usize;
        if !valid_actions(count) || count != self.actions {
            return Err(SessionError::Counts {
                actions: self.actions,
                alphas: count,
            });
        }
        let mut per_action = Vec::with_capacity(count);
        for _ in 0..count {
            per_action.push(SigningCommitments::deserialize(r.bytes()?).map_err(frost)?);
        }
        r.finish()?;
        self.grid.insert(sender, per_action);

        if self.grid.len() == self.cfg.signers.len() {
            self.phase.advance()?; // InitSent -> CommitmentsCollected
        }
        Ok(())
    }

    /// Hand the coordinator what the wallet tier derived from the built transaction.
    ///
    /// Neither value can be invented here: the sighash comes from the transaction, and each `alpha`
    /// is the per-action randomizer already baked into it.
    pub fn prepare_payload(
        &mut self,
        sighash: [u8; 32],
        alphas: Vec<[u8; 32]>,
    ) -> Result<(), SessionError> {
        if self.phase != SignPhase::CommitmentsCollected {
            return Err(SessionError::WrongStage(self.phase));
        }
        if alphas.len() != self.actions {
            return Err(SessionError::Counts {
                actions: self.actions,
                alphas: alphas.len(),
            });
        }
        self.sighash = Some(sighash);
        self.alphas = alphas;
        self.phase.advance()?; // CommitmentsCollected -> PcztReady
        Ok(())
    }

    /// `SIGN_PAYLOAD` to every signer.
    ///
    /// The commitment grid is repeated per action. For one action that is the grid once; for `m`
    /// actions it costs `m` copies, which is still cheaper than a second message the signer would
    /// have to carry across phases.
    pub fn payloads(&mut self) -> Result<Vec<Envelope>, SessionError> {
        if self.phase != SignPhase::PcztReady {
            return Err(SessionError::WrongStage(self.phase));
        }
        let sighash = self.sighash.ok_or(SessionError::Malformed)?;

        let mut payload = Vec::new();
        payload.extend_from_slice(&sighash);
        put_u16(&mut payload, self.actions as u16);
        for action in 0..self.actions {
            payload.extend_from_slice(&self.alphas[action]);
            payload.push(u8::try_from(self.grid.len()).map_err(|_| SessionError::Malformed)?);
            for (slot, per_action) in &self.grid {
                payload.push(*slot);
                put_bytes(
                    &mut payload,
                    &per_action[action].serialize().map_err(frost)?,
                )?;
            }
        }

        self.phase.advance()?; // PcztReady -> Verifying
        let mut out = Vec::with_capacity(self.cfg.signers.len());
        for slot in self.cfg.signers.clone() {
            let seq = self.next_seq();
            out.push(Envelope::new(
                MsgType::SignPayload,
                3,
                COORDINATOR_SLOT,
                slot,
                seq,
                self.cfg.session_id,
                payload.clone(),
            )?);
        }
        Ok(out)
    }

    /// `SIGN_SHARE` from one signer: one share per action.
    pub fn on_share(&mut self, env: &Envelope) -> Result<(), SessionError> {
        let sender = self.accept(env, MsgType::SignShare)?;
        if self.phase != SignPhase::Verifying {
            return Err(SessionError::WrongStage(self.phase));
        }
        if self.shares.contains_key(&sender) {
            return Err(SessionError::Duplicate(sender));
        }
        let mut r = Reader::new(&env.payload);
        if r.u8()? != 1 {
            return Err(SessionError::NotApproved);
        }
        let count = r.u16()? as usize;
        if !valid_actions(count) || count != self.actions {
            return Err(SessionError::Counts {
                actions: self.actions,
                alphas: count,
            });
        }
        let mut shares = Vec::with_capacity(count);
        for _ in 0..count {
            shares.push(SignatureShare::deserialize(r.bytes()?).map_err(frost)?);
        }
        r.finish()?;
        self.shares.insert(sender, shares);

        if self.shares.len() == self.cfg.signers.len() {
            self.phase.advance()?; // Verifying -> Approved
            self.phase.advance()?; // Approved -> SharesCollected
        }
        Ok(())
    }

    /// Aggregate every action. One signature per action, since the randomizer differs per action.
    pub fn aggregate(&mut self) -> Result<Vec<threshold::SpendAuthSignature>, SessionError> {
        if self.phase != SignPhase::SharesCollected {
            return Err(SessionError::WrongStage(self.phase));
        }
        let sighash = self.sighash.ok_or(SessionError::Malformed)?;

        let mut out = Vec::with_capacity(self.actions);
        for (action, alpha) in self.alphas.iter().enumerate() {
            let mut commitments = BTreeMap::new();
            for (slot, per_action) in &self.grid {
                let id = *self
                    .cfg
                    .identifiers
                    .get(slot)
                    .ok_or(SessionError::UnknownSender(*slot))?;
                commitments.insert(id, per_action[action].clone());
            }
            let mut shares = BTreeMap::new();
            for (slot, per_action) in &self.shares {
                let id = *self
                    .cfg
                    .identifiers
                    .get(slot)
                    .ok_or(SessionError::UnknownSender(*slot))?;
                shares.insert(id, per_action[action]);
            }
            out.push(threshold::aggregate_with_alpha(
                &self.pubkeys,
                &commitments,
                &sighash,
                alpha,
                &shares,
            )?);
        }
        // SharesCollected -> Aggregated. Broadcast is the wallet tier's step, not this module's.
        self.phase.advance()?;
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Signer
// ---------------------------------------------------------------------------

/// What a signer must verify before approving. Owned so the caller can hold it across the approval
/// prompt without borrowing the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedMaterial {
    pub sighash: [u8; 32],
    pub alphas: Vec<[u8; 32]>,
}

/// One device's side of a session. Owns its nonces; never sees another signer's.
pub struct SignerSession {
    session_id: [u8; SESSION_ID_LEN],
    my_slot: u8,
    identifiers: BTreeMap<u8, Identifier>,
    phase: SignPhase,
    actions: usize,
    nonces: Vec<SigningNonces>,
    nonce_states: Vec<NonceState>,
    commitments: Vec<SigningCommitments>,
    last_seq: Option<u32>,
    my_seq: u32,
    decoded: Option<DecodedPayload>,
}

impl SignerSession {
    /// `identifiers` is the locked roster's slot → FROST identifier map, which §3.2 requires every
    /// device to hold. Without it a signer cannot build the signing package the coordinator defined.
    pub fn new(
        session_id: [u8; SESSION_ID_LEN],
        my_slot: u8,
        identifiers: BTreeMap<u8, Identifier>,
    ) -> Self {
        Self {
            session_id,
            my_slot,
            identifiers,
            phase: SignPhase::Created,
            actions: 0,
            nonces: Vec::new(),
            nonce_states: Vec::new(),
            commitments: Vec::new(),
            last_seq: None,
            my_seq: 0,
            decoded: None,
        }
    }

    pub fn phase(&self) -> SignPhase {
        self.phase
    }

    pub fn nonce_states(&self) -> &[NonceState] {
        &self.nonce_states
    }

    fn next_seq(&mut self) -> u32 {
        self.my_seq = self.my_seq.wrapping_add(1);
        self.my_seq
    }

    fn accept(&mut self, env: &Envelope, expected: MsgType) -> Result<(), SessionError> {
        if env.session_id != self.session_id {
            return Err(SessionError::WrongSession);
        }
        if env.recipient != self.my_slot {
            return Err(SessionError::WrongRecipient(env.recipient));
        }
        if env.sender != COORDINATOR_SLOT {
            return Err(SessionError::UnknownSender(env.sender));
        }
        if env.msg_type != expected {
            return Err(SessionError::WrongPhase {
                expected,
                got: env.msg_type,
            });
        }
        if let Some(last) = self.last_seq {
            if env.sequence <= last {
                return Err(SessionError::SequenceRewind {
                    sender: env.sender,
                    last,
                    got: env.sequence,
                });
            }
        }
        self.last_seq = Some(env.sequence);
        Ok(())
    }

    /// `SIGN_INIT` in, `SIGN_COMMITMENTS` out. Generates one nonce set per action.
    pub fn on_init<R>(
        &mut self,
        env: &Envelope,
        key_package: &KeyPackage,
        rng: &mut R,
    ) -> Result<Envelope, SessionError>
    where
        R: CryptoRng + RngCore,
    {
        self.accept(env, MsgType::SignInit)?;
        if self.phase != SignPhase::Created {
            return Err(SessionError::WrongStage(self.phase));
        }
        let mut r = Reader::new(&env.payload);
        let actions = r.u16()? as usize;
        let participants = r.u16()?;
        let threshold = r.u16()?;
        let n = r.u8()? as usize;
        let signers = r.take(n)?.to_vec();
        r.finish()?;

        if !valid_actions(actions) || signers.len() > crate::threshold::MAX_PARTICIPANTS as usize {
            return Err(SessionError::Counts {
                actions: MAX_ACTIONS,
                alphas: actions,
            });
        }
        // The selection must be exactly threshold-sized, and every slot in it must exist. A
        // coordinator that under-selects would otherwise produce a session that cannot aggregate,
        // long after the user approved.
        if signers.len() != threshold as usize || threshold > participants {
            return Err(SessionError::Incomplete {
                have: signers.len(),
                need: threshold as usize,
            });
        }
        if self.my_slot as u16 >= participants || !signers.contains(&self.my_slot) {
            return Err(SessionError::UnknownSender(self.my_slot));
        }
        if !self.identifiers.contains_key(&self.my_slot) {
            return Err(SessionError::UnknownSender(self.my_slot));
        }
        self.actions = actions;
        self.phase.advance()?; // Created -> InitSent

        let mut payload = Vec::new();
        put_u16(&mut payload, actions as u16);
        for _ in 0..actions {
            let c = commit(key_package, rng);
            put_bytes(&mut payload, &c.commitments.serialize().map_err(frost)?)?;
            self.commitments.push(c.commitments);
            self.nonces.push(c.nonces);
            // The commitment goes on the wire in the same message, so the nonce is Committed as
            // soon as it exists.
            self.nonce_states.push(NonceState::Committed);
        }
        self.phase.advance()?; // InitSent -> CommitmentsCollected

        let seq = self.next_seq();
        Ok(Envelope::new(
            MsgType::SignCommitments,
            1,
            self.my_slot,
            COORDINATOR_SLOT,
            seq,
            self.session_id,
            payload,
        )?)
    }

    /// `SIGN_PAYLOAD` in. Returns the material the caller must verify before approving.
    pub fn on_payload(&mut self, env: &Envelope) -> Result<VerifiedMaterial, SessionError> {
        self.accept(env, MsgType::SignPayload)?;
        if self.phase != SignPhase::CommitmentsCollected {
            return Err(SessionError::WrongStage(self.phase));
        }
        let decoded = decode_payload(&env.payload)?;
        if decoded.alphas.len() != self.actions || decoded.per_action.len() != self.actions {
            return Err(SessionError::Counts {
                actions: self.actions,
                alphas: decoded.alphas.len(),
            });
        }
        // Every action's map must contain this signer's commitment, or the coordinator is building a
        // package this device never contributed to.
        for grid in &decoded.per_action {
            if !grid.contains_key(&self.my_slot) {
                return Err(SessionError::UnknownSender(self.my_slot));
            }
        }
        let material = VerifiedMaterial {
            sighash: decoded.sighash,
            alphas: decoded.alphas.clone(),
        };
        self.decoded = Some(decoded);
        self.phase.advance()?; // CommitmentsCollected -> PcztReady
        self.phase.advance()?; // PcztReady -> Verifying
        Ok(material)
    }

    /// Record that the human approved, after the caller verified the material and the PCZT. Only
    /// from here can a share exist.
    pub fn approve(&mut self) -> Result<(), SessionError> {
        if self.phase != SignPhase::Verifying {
            return Err(SessionError::WrongStage(self.phase));
        }
        self.phase.advance()?; // Verifying -> Approved
        Ok(())
    }

    /// Produce `SIGN_SHARE`. Consumes every nonce: a second call cannot reuse them.
    pub fn share(&mut self, key_package: &KeyPackage) -> Result<Envelope, SessionError> {
        if self.phase != SignPhase::Approved {
            return Err(SessionError::NotApproved);
        }
        let decoded = self.decoded.as_ref().ok_or(SessionError::Malformed)?;
        let actions = self.actions;

        let mut payload = Vec::new();
        payload.push(1u8); // approved
        put_u16(&mut payload, actions as u16);

        for action in 0..actions {
            // Slot -> FROST identifier, from the locked roster.
            let grid: BTreeMap<Identifier, SigningCommitments> = decoded.per_action[action]
                .iter()
                .map(|(slot, c)| {
                    self.identifiers
                        .get(slot)
                        .map(|id| (*id, c.clone()))
                        .ok_or(SessionError::UnknownSender(*slot))
                })
                .collect::<Result<_, _>>()?;

            // Burn the nonce *before* signing. Once it is handed to `sign_with_alpha` it may have
            // contributed to a share, so a failed call must not leave it reusable — the `?` on the
            // call would otherwise return early with the nonce still Committed.
            self.nonce_states[action] = self.nonce_states[action].consume()?;

            let share = threshold::sign_with_alpha(
                &self.nonces[action],
                key_package,
                &grid,
                &decoded.sighash,
                &decoded.alphas[action],
            )?;
            put_bytes(&mut payload, &share.serialize())?;
        }

        self.phase.advance()?; // Approved -> SharesCollected
        let seq = self.next_seq();
        Ok(Envelope::new(
            MsgType::SignShare,
            3,
            self.my_slot,
            COORDINATOR_SLOT,
            seq,
            self.session_id,
            payload,
        )?)
    }
}
