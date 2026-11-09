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
        assert_eq!(
            nonce.consume(),
            Err(TransitionError::NonceUnusable(NonceState::Consumed))
        );
        assert_eq!(
            nonce.advance(),
            Err(TransitionError::NonceUnusable(NonceState::Consumed))
        );
    }

    #[test]
    fn destroyed_nonce_never_signs() {
        let mut nonce = NonceState::Committed;
        nonce = nonce.destroy();
        assert!(!nonce.may_sign());
        assert_eq!(
            nonce.consume(),
            Err(TransitionError::NonceUnusable(NonceState::Destroyed))
        );
    }
}
