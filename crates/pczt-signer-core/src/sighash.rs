//! Sighash recomputation.
//!
//! The signer must never sign a digest it was handed. `SIGN_PAYLOAD` carries a `sighash` field for
//! the coordinator's convenience, and this is where it stops being trusted: the value is recomputed
//! from the PCZT by the same `pczt` Signer role that dispatches on transaction version and
//! consensus branch, and the claimed value is only allowed to agree with it.
//!
//! Note that the digest covers the randomized verification keys, so recomputation is only meaningful
//! *after* the PCZT carries its final `rk` values — which is the case for the payload a signer
//! receives.

use pczt::roles::signer::Signer;
use pczt::Pczt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SighashError {
    /// The PCZT could not be interpreted well enough to compute a sighash.
    Uncomputable,
    /// The recomputed digest differs from the one the coordinator supplied.
    Mismatch {
        claimed: [u8; 32],
        recomputed: [u8; 32],
    },
}

impl core::fmt::Display for SighashError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Uncomputable => f.write_str("cannot compute a shielded sighash from this PCZT"),
            Self::Mismatch { .. } => f.write_str(
                "recomputed sighash differs from the supplied digest: the coordinator's digest is \
                 not the transaction's",
            ),
        }
    }
}

impl std::error::Error for SighashError {}

/// Recompute the shielded sighash from the PCZT alone.
pub fn shielded_sighash(pczt: Pczt) -> Result<[u8; 32], SighashError> {
    let signer = Signer::new(pczt).map_err(|_| SighashError::Uncomputable)?;
    Ok(signer.shielded_sighash())
}

/// Recompute and compare against a coordinator-supplied digest.
pub fn verify_shielded_sighash(pczt: Pczt, claimed: &[u8; 32]) -> Result<[u8; 32], SighashError> {
    let recomputed = shielded_sighash(pczt)?;
    if &recomputed != claimed {
        return Err(SighashError::Mismatch {
            claimed: *claimed,
            recomputed,
        });
    }
    Ok(recomputed)
}
