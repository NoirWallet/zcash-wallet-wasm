//! Session state machines.
//!
//! Two of them, as the design doc separates them (§13.1 DKG, §13.2 signing), plus the per-spend
//! nonce lifecycle that the signing machine is not allowed to skip.
//!
//! The point of encoding these as types rather than as documentation is that the dangerous
//! transitions are the ones that are easy to leave out of an implementation: an aggregation that
//! happens before every spend has a share, a signing session that is resumed after its nonces may
//! have been dropped, or a nonce that is put back to `Committed` after being used.

/// DKG lifecycle: `CREATED → PAIRING → ROSTER_LOCKED → DKG_R1 → R1_VERIFIED → DKG_R2 →
/// FINALIZED → KEYSET_VERIFIED → ACTIVATED`, any failure to `ABORTED`.
///
/// **Round 1 may be generated during `PAIRING`.** The chain above is the order *after* the roster is
/// locked, not a precondition on when this device's own round-1 contribution is created: FROST round 1
/// depends only on `(N, t, my identifier)` and references no other participant, so creating it early
/// cannot weaken anything. What the ceremony actually needs is that no participant *consumes* another's
/// round-1 broadcast, and nobody sends a round-2 secret, before the roster is locked and the round-1
/// set has been checked for consistency (§5.5/§5.6).
///
/// That difference is what makes the merged QR flow of §6.2/§6.3 possible: a pairing message can carry
/// `Pair + Round 1` because generating it early is fine, while the identity binding still comes from
/// SAS and the roster hash once the roster locks.
///
/// So "has round 1 been generated" is deliberately **not** a phase: it is local state that no other
/// device can observe. Use [`DkgPhase::may_start_round1`] before generating and
/// [`DkgPhase::may_accept_peer_round1`] before consuming a peer's broadcast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DkgPhase {
    Created,
    Pairing,
    RosterLocked,
    DkgR1,
    R1Verified,
    DkgR2,
    Finalized,
    KeysetVerified,
    Activated,
    Aborted,
}

impl DkgPhase {
    /// The only forward transitions. `Aborted` is terminal and reachable from anywhere.
    pub fn next(self) -> Option<Self> {
        use DkgPhase::*;
        Some(match self {
            Created => Pairing,
            Pairing => RosterLocked,
            RosterLocked => DkgR1,
            DkgR1 => R1Verified,
            R1Verified => DkgR2,
            DkgR2 => Finalized,
            Finalized => KeysetVerified,
            KeysetVerified => Activated,
            Activated | Aborted => return None,
        })
    }

    /// Whether a vault in this phase may hold funds. Only `Activated` may.
    pub fn is_usable(self) -> bool {
        matches!(self, DkgPhase::Activated)
    }

    /// Whether this device may generate its own round-1 contribution now.
    ///
    /// True from `Pairing` onwards: round 1 needs only `(N, t, my slot)`. Generating it early does not
    /// advance the phase — the phase still moves to `RosterLocked` only when the roster is actually
    /// locked.
    pub fn may_start_round1(self) -> bool {
        matches!(self, DkgPhase::Pairing | DkgPhase::RosterLocked | DkgPhase::DkgR1)
    }

    /// Whether a *peer's* round-1 broadcast may be consumed now.
    ///
    /// This is the invariant the merged flow must not break: consuming peers' broadcasts before the
    /// roster is locked would let a participant act on a set it has not agreed to.
    pub fn may_accept_peer_round1(self) -> bool {
        matches!(self, DkgPhase::RosterLocked | DkgPhase::DkgR1 | DkgPhase::R1Verified)
    }

    /// Whether a round-2 secret may be produced or forwarded now.
    ///
    /// Requires the roster to be locked *and* the round-1 set to be verified, so it is reachable only
    /// after [`Self::may_accept_peer_round1`] has been honoured.
    pub fn may_send_round2(self) -> bool {
        matches!(self, DkgPhase::R1Verified | DkgPhase::DkgR2)
    }
}

/// Signing lifecycle: `CREATED → INIT_SENT → COMMITMENTS_COLLECTED → PCZT_READY → VERIFYING →
/// APPROVED → SHARES_COLLECTED → AGGREGATED → BROADCASTED`, with `REJECTED` / `ABORTED` /
/// `EXPIRED` as terminal failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SignPhase {
    Created,
    InitSent,
    CommitmentsCollected,
    PcztReady,
    Verifying,
    Approved,
    SharesCollected,
    Aggregated,
    Broadcasted,
    Rejected,
    Aborted,
    Expired,
}

impl SignPhase {
    pub fn next(self) -> Option<Self> {
        use SignPhase::*;
        Some(match self {
            Created => InitSent,
            InitSent => CommitmentsCollected,
            CommitmentsCollected => PcztReady,
            PcztReady => Verifying,
            Verifying => Approved,
            Approved => SharesCollected,
            SharesCollected => Aggregated,
            Aggregated => Broadcasted,
            Broadcasted | Rejected | Aborted | Expired => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        self.next().is_none()
    }

    /// Whether signature shares may still be produced in this phase.
    ///
    /// `Approved` is the only window: the signer has had to independently verify the PCZT and a
    /// human has had to approve it before a share exists. This is what prevents a coordinator from
    /// obtaining shares for a transaction nobody looked at.
    pub fn may_sign(self) -> bool {
        matches!(self, SignPhase::Approved)
    }
}

/// Per-spend nonce lifecycle. `CREATED → COMMITTED → CONSUMED → DESTROYED`.
///
/// There is no path back. `CONSUMED` means a signature share was derived from this nonce; reusing
/// it with a different challenge (a different per-action `alpha` over the same sighash) leaks the
/// signing share outright.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NonceState {
    Created,
    Committed,
    Consumed,
    Destroyed,
}

impl NonceState {
    pub fn next(self) -> Option<Self> {
        use NonceState::*;
        Some(match self {
            Created => Committed,
            Committed => Consumed,
            Consumed => Destroyed,
            Destroyed => return None,
        })
    }

    /// Whether a nonce in this state may still be used to produce a share.
    pub fn may_sign(self) -> bool {
        matches!(self, NonceState::Committed)
    }

    /// Destroy a nonce without using it. Legal from any live state; a destroyed nonce never signs.
    pub fn destroy(self) -> Self {
        NonceState::Destroyed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionError {
    /// The step is not the next one; callers must advance in order.
    NotNext { from_phase: &'static str },
    /// The machine is already in a terminal state.
    Terminal,
    /// The nonce has been used (or destroyed) and must not be used again.
    NonceUnusable(NonceState),
}

impl core::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotNext { from_phase } => write!(f, "illegal transition from {from_phase}"),
            Self::Terminal => write!(f, "state machine is in a terminal state"),
            Self::NonceUnusable(s) => write!(f, "nonce in state {s:?} cannot sign"),
        }
    }
}

impl std::error::Error for TransitionError {}

impl DkgPhase {
    pub fn advance(&mut self) -> Result<Self, TransitionError> {
        let next = self.next().ok_or(TransitionError::Terminal)?;
        *self = next;
        Ok(next)
    }

    pub fn abort(&mut self) {
        *self = DkgPhase::Aborted;
    }
}

impl SignPhase {
    pub fn advance(&mut self) -> Result<Self, TransitionError> {
        let next = self.next().ok_or(TransitionError::Terminal)?;
        *self = next;
        Ok(next)
    }

    pub fn abort(&mut self) {
        *self = SignPhase::Aborted;
    }
}

impl NonceState {
    pub fn advance(&mut self) -> Result<Self, TransitionError> {
        if !self.may_sign() && *self != NonceState::Created {
            return Err(TransitionError::NonceUnusable(*self));
        }
        let next = self.next().ok_or(TransitionError::Terminal)?;
        *self = next;
        Ok(next)
    }

    /// Consume a committed nonce. This is the only way to reach `Consumed`, and it refuses any
    /// state that is not `Committed`, which is what stops a second signature from one nonce.
    pub fn consume(&mut self) -> Result<Self, TransitionError> {
        if !self.may_sign() {
            return Err(TransitionError::NonceUnusable(*self));
        }
        *self = NonceState::Consumed;
        Ok(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dkg_runs_to_activation() {
        let mut phase = DkgPhase::Created;
        while phase.next().is_some() {
            phase.advance().unwrap();
        }
        assert_eq!(phase, DkgPhase::Activated);
        assert!(phase.is_usable());
        assert_eq!(phase.advance(), Err(TransitionError::Terminal));
    }

    #[test]
    fn round_one_may_be_generated_before_the_roster_locks() {
        // The merged QR flow: a pairing message carries the device's round-1 broadcast, so round 1
        // exists while the ceremony is still pairing.
        let mut phase = DkgPhase::Created;
        assert!(!phase.may_start_round1(), "not before pairing");
        phase.advance().unwrap(); // Created -> Pairing
        assert_eq!(phase, DkgPhase::Pairing);
        assert!(phase.may_start_round1(), "round 1 may be generated during pairing");

        // But a peer's broadcast may not be consumed until the roster is locked.
        assert!(!phase.may_accept_peer_round1());
        assert!(!phase.may_send_round2(), "no round-2 secret before the roster locks");

        phase.advance().unwrap(); // Pairing -> RosterLocked
        assert!(phase.may_accept_peer_round1(), "peers' broadcasts may be consumed now");
        assert!(!phase.may_send_round2(), "round-1 set still has to be verified");

        phase.advance().unwrap(); // RosterLocked -> DkgR1
        phase.advance().unwrap(); // DkgR1 -> R1Verified
        assert!(phase.may_send_round2());
    }

    #[test]
    fn round_two_is_unreachable_while_pairing() {
        for phase in [DkgPhase::Created, DkgPhase::Pairing] {
            assert!(!phase.may_send_round2(), "{phase:?}");
            assert!(!phase.may_accept_peer_round1(), "{phase:?}");
        }
        // And once aborted, nothing is allowed.
        let mut aborted = DkgPhase::Pairing;
        aborted.abort();
        assert!(!aborted.may_start_round1());
        assert!(!aborted.may_accept_peer_round1());
        assert!(!aborted.may_send_round2());
    }

    #[test]
    fn aborted_dkg_is_never_usable() {
        let mut phase = DkgPhase::DkgR1;
        phase.abort();
        assert!(!phase.is_usable());
        assert_eq!(phase.advance(), Err(TransitionError::Terminal));
    }

    #[test]
    fn signing_only_signs_when_approved() {
        let mut phase = SignPhase::Created;
        while phase.next().is_some() {
            phase.advance().unwrap();
        }
        assert_eq!(phase, SignPhase::Broadcasted);
        assert!(phase.is_terminal());
        assert!(!phase.may_sign());
    }

    #[test]
    fn approved_is_the_signing_window() {
        let mut phase = SignPhase::Created;
        while phase != SignPhase::Approved {
            phase.advance().unwrap();
        }
        assert!(phase.may_sign());
        phase.advance().unwrap();
        assert!(!phase.may_sign());
    }

    #[test]
    fn nonce_cannot_sign_twice() {
        let mut nonce = NonceState::Created;
        nonce.advance().unwrap();
        assert!(nonce.may_sign());
        nonce.consume().unwrap();
        assert!(!nonce.may_sign());
        assert_eq!(nonce.consume(), Err(TransitionError::NonceUnusable(NonceState::Consumed)));
        assert_eq!(nonce.advance(), Err(TransitionError::NonceUnusable(NonceState::Consumed)));
    }

    #[test]
    fn destroyed_nonce_never_signs() {
        let mut nonce = NonceState::Committed;
        nonce = nonce.destroy();
        assert!(!nonce.may_sign());
        assert_eq!(nonce.consume(), Err(TransitionError::NonceUnusable(NonceState::Destroyed)));
    }
}
