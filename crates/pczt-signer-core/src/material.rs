//! Per-spend signature material, and the check that makes a lying coordinator detectable.
//!
//! A signer is handed `alpha` for each action, together with the PCZT. `alpha` is the randomizer
//! the transaction was built with, so the action's `rk` must equal `ak + alpha·G`. Recomputing
//! that from the group verifying key is the whole verification: it turns "the coordinator told me
//! this is the transaction" into "this `rk` is my group key, randomized by this `alpha`".
//!
//! Note what this does *not* need: the PCZT's own `alpha`. Comparing the coordinator's `alpha`
//! against `rk` already binds the two, so a coordinator that ships an `alpha` the PCZT was not
//! built with fails the check. That matters because the PCZT's `alpha` is not part of its public
//! accessors — the check is complete without it.
//!
//! This replaces the design doc's §8.6, which proposed deriving the randomizer from the round-1
//! commitments. Orchard fixes `alpha` when the transaction is built — before any commitment
//! exists — so there is nothing to derive; what a signer can and must do is check the relation.

use frost_vault_core::threshold::randomized_verifying_key;
use pczt::Pczt;

/// One Ironwood action's randomized verification key, as carried by the PCZT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionRk {
    /// Index of the action inside the Ironwood bundle.
    pub action_index: u32,
    /// The action's randomized verification key (`rk`).
    pub rk: [u8; 32],
}

/// One action's signing material: the PCZT's `rk` plus the coordinator's `alpha`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpendMaterial {
    pub action_index: u32,
    pub alpha: [u8; 32],
    pub rk: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialError {
    /// `rk` is not `ak + alpha·G`. Either the coordinator lied about `alpha`, or the PCZT was
    /// swapped after the material was prepared.
    RkMismatch { action_index: u32 },
    /// The number of alphas does not match the number of actions.
    AlphaCount { actions: usize, alphas: usize },
    /// An action carries no `rk`, so nothing can be verified for it.
    MissingRk { action_index: u32 },
}

impl core::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RkMismatch { action_index } => write!(
                f,
                "action {action_index}: rk is not the group key randomized by the supplied alpha"
            ),
            Self::AlphaCount { actions, alphas } => write!(
                f,
                "{alphas} alpha value(s) supplied for {actions} ironwood action(s)"
            ),
            Self::MissingRk { action_index } => write!(f, "action {action_index}: no rk present"),
        }
    }
}

impl std::error::Error for MaterialError {}

impl SpendMaterial {
    /// Check `rk == ak + alpha·G`, using the FROST group verifying key as `ak`.
    ///
    /// The value is computed, never trusted: `rk` from the PCZT is compared against the key derived
    /// from the group key and the supplied `alpha`.
    pub fn verify_rk(&self, group_verifying_key: &[u8; 32]) -> Result<(), MaterialError> {
        let expected =
            randomized_verifying_key(group_verifying_key, &self.alpha).map_err(|_| {
                MaterialError::RkMismatch {
                    action_index: self.action_index,
                }
            })?;
        if expected != self.rk {
            return Err(MaterialError::RkMismatch {
                action_index: self.action_index,
            });
        }
        Ok(())
    }
}

/// Read every Ironwood action's `rk` from a PCZT.
///
/// The caller must have run [`crate::gate::check`] first; this function reads what the policy
/// already admitted and does not re-apply it.
pub fn ironwood_action_rks(pczt: &Pczt) -> Result<Vec<ActionRk>, MaterialError> {
    let actions = pczt.ironwood().actions();
    let mut out = Vec::with_capacity(actions.len());
    for (index, action) in actions.iter().enumerate() {
        let action_index = index as u32;
        let spend = action.spend();
        // `rk` is a non-optional field of a finalizable spend; if it is absent the PCZT is not a
        // signing request this build understands.
        let rk = *spend.rk();
        if rk == [0u8; 32] {
            return Err(MaterialError::MissingRk { action_index });
        }
        out.push(ActionRk { action_index, rk });
    }
    Ok(out)
}

/// Pair coordinator-supplied `alpha` values with the PCZT's action `rk`s.
///
/// Ordering is positional and must match the Ironwood bundle exactly: an off-by-one here would
/// authorize a different action than the one the user was shown, so a count mismatch is an error
/// rather than a zip.
pub fn bind_alphas(
    rks: &[ActionRk],
    alphas: &[[u8; 32]],
) -> Result<Vec<SpendMaterial>, MaterialError> {
    if rks.len() != alphas.len() {
        return Err(MaterialError::AlphaCount {
            actions: rks.len(),
            alphas: alphas.len(),
        });
    }
    Ok(rks
        .iter()
        .zip(alphas.iter())
        .map(|(rk, alpha)| SpendMaterial {
            action_index: rk.action_index,
            alpha: *alpha,
            rk: rk.rk,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use frost_vault_core::threshold::run_dkg;
    use rand08::rngs::OsRng;

    fn alpha(tag: u8) -> [u8; 32] {
        let mut a = [0u8; 32];
        a[0] = tag;
        a[31] = 0x01;
        a
    }

    #[test]
    fn accepts_matching_rk() {
        let mut rng = OsRng;
        let dkg = run_dkg(2, 2, &mut rng).unwrap();
        let ak = dkg.group_verifying_key().unwrap();
        let alpha = alpha(0x11);
        let rk = randomized_verifying_key(&ak, &alpha).unwrap();

        let material = SpendMaterial {
            action_index: 0,
            alpha,
            rk,
        };
        assert_eq!(material.verify_rk(&ak), Ok(()));
    }

    #[test]
    fn rejects_swapped_alpha() {
        let mut rng = OsRng;
        let dkg = run_dkg(2, 2, &mut rng).unwrap();
        let ak = dkg.group_verifying_key().unwrap();
        let rk = randomized_verifying_key(&ak, &alpha(0x11)).unwrap();

        let material = SpendMaterial {
            action_index: 3,
            alpha: alpha(0x22), // coordinator swapped the alpha
            rk,
        };
        assert_eq!(
            material.verify_rk(&ak),
            Err(MaterialError::RkMismatch { action_index: 3 })
        );
    }

    #[test]
    fn rejects_rk_from_another_group() {
        // The classic substitution: a valid rk, but for a different vault's group key.
        let mut rng = OsRng;
        let mine = run_dkg(2, 2, &mut rng).unwrap();
        let theirs = run_dkg(2, 2, &mut rng).unwrap();
        let their_ak = theirs.group_verifying_key().unwrap();
        let alpha = alpha(0x33);
        let their_rk = randomized_verifying_key(&their_ak, &alpha).unwrap();

        let material = SpendMaterial {
            action_index: 0,
            alpha,
            rk: their_rk,
        };
        assert_eq!(
            material.verify_rk(&mine.group_verifying_key().unwrap()),
            Err(MaterialError::RkMismatch { action_index: 0 })
        );
    }

    #[test]
    fn bind_refuses_count_mismatch() {
        let rks = vec![
            ActionRk {
                action_index: 0,
                rk: [1u8; 32],
            },
            ActionRk {
                action_index: 1,
                rk: [2u8; 32],
            },
        ];
        assert_eq!(
            bind_alphas(&rks, &[alpha(1)]),
            Err(MaterialError::AlphaCount {
                actions: 2,
                alphas: 1
            })
        );
    }

    #[test]
    fn bind_preserves_action_order() {
        let rks = vec![
            ActionRk {
                action_index: 0,
                rk: [1u8; 32],
            },
            ActionRk {
                action_index: 1,
                rk: [2u8; 32],
            },
        ];
        let bound = bind_alphas(&rks, &[alpha(7), alpha(8)]).unwrap();
        assert_eq!(bound[0].action_index, 0);
        assert_eq!(bound[0].rk, [1u8; 32]);
        assert_eq!(bound[0].alpha, alpha(7));
        assert_eq!(bound[1].action_index, 1);
        assert_eq!(bound[1].alpha, alpha(8));
    }
}
