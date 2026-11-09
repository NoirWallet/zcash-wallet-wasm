//! Landing a signature share on a PCZT.
//!
//! The signer tier produces 64-byte RedPallas spend authorization signatures; something has to put
//! them into the transaction. That something is `pczt`'s Signer role, and the important detail is
//! that it **verifies each signature against the action's `rk` before storing it**. A wrong share, a
//! share for another action, or a share for another pool cannot be applied — so this module is a
//! thin wrapper rather than a second place where verification happens.
//!
//! `Signer::apply_orchard_spend_auth_signature` takes `(value_pool, action_index, signature)`, which
//! is exactly the triple [`crate::material`] and the session already carry, so nothing is re-derived
//! here.

use orchard::ValuePool;
use pczt::roles::signer::{Signer, SpendAuthSignature};
use pczt::Pczt;

use crate::material::SpendMaterial;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// The Signer role refused to start on this PCZT (malformed, or an anchor it cannot accept).
    SignerUnavailable,
    /// The signature did not verify against the action's `rk`, or the action does not exist.
    Rejected { action_index: u32 },
    /// Two signatures were supplied for the same action.
    Duplicate { action_index: u32 },
    /// Material and signatures are not the same length.
    CountMismatch { material: usize, signatures: usize },
}

impl core::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SignerUnavailable => f.write_str("cannot start the signer role on this PCZT"),
            Self::Rejected { action_index } => write!(
                f,
                "action {action_index}: signature does not verify against rk, or the action is absent"
            ),
            Self::Duplicate { action_index } => {
                write!(f, "action {action_index}: more than one signature supplied")
            }
            Self::CountMismatch {
                material,
                signatures,
            } => write!(
                f,
                "{signatures} signature(s) for {material} action(s): refusing to guess the pairing"
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

/// Apply one spend authorization signature to one Ironwood action.
pub fn apply_ironwood_signature(
    pczt: Pczt,
    action_index: u32,
    signature: &[u8; 64],
) -> Result<Pczt, ApplyError> {
    apply_all(pczt, &[(action_index, *signature)])
}

/// Apply several at once. All of them land or none do: the PCZT is consumed and only returned on
/// success, so a partially-signed transaction cannot leak out of a failed call.
pub fn apply_all(pczt: Pczt, signatures: &[(u32, [u8; 64])]) -> Result<Pczt, ApplyError> {
    // Reject a malformed request before starting the role, so a duplicate never leaves a partially
    // applied PCZT behind.
    let mut seen = std::collections::BTreeSet::new();
    for (action_index, _) in signatures {
        if !seen.insert(*action_index) {
            return Err(ApplyError::Duplicate {
                action_index: *action_index,
            });
        }
    }

    let mut signer = Signer::new(pczt).map_err(|_| ApplyError::SignerUnavailable)?;
    for (action_index, signature) in signatures {
        let contribution =
            SpendAuthSignature::from_parts(ValuePool::Ironwood, *action_index as usize, *signature);
        signer
            .apply_orchard_spend_auth_signature(&contribution)
            .map_err(|_| ApplyError::Rejected {
                action_index: *action_index,
            })?;
    }
    Ok(signer.finish())
}

/// Apply every aggregated share, pairing it with the material it was produced for.
pub fn apply_material(
    pczt: Pczt,
    material: &[SpendMaterial],
    signatures: &[[u8; 64]],
) -> Result<Pczt, ApplyError> {
    if material.len() != signatures.len() {
        return Err(ApplyError::CountMismatch {
            material: material.len(),
            signatures: signatures.len(),
        });
    }
    let paired: Vec<(u32, [u8; 64])> = material
        .iter()
        .zip(signatures.iter())
        .map(|(m, s)| (m.action_index, *s))
        .collect();
    apply_all(pczt, &paired)
}
