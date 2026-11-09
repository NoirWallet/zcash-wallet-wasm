//! What this signer agrees to look at at all.
//!
//! The gate runs **before** anything is decoded for display, so a PCZT that this build cannot
//! fully verify never reaches a screen and never reaches a signature. It is deliberately a pure
//! function over a small shape struct: the shape can be produced from a real `pczt::Pczt`
//! ([`PcztShape::of`]) or constructed directly in a test, and the policy is auditable without
//! reading any Zcash code.

use pczt::Pczt;
use zcash_protocol::PoolType;

/// Consensus branch id of NU6.3 (Ironwood). First branch with V6 transactions.
pub const BRANCH_NU6_3: u32 = 0x37a5_165b;
/// Consensus branch id of NU7 (ZIP 259).
pub const BRANCH_NU7: u32 = 0x7719_0ad9;

/// Transaction version carrying an Ironwood bundle.
pub const TX_VERSION_V6: u32 = 6;

/// Everything the gate needs to know about a PCZT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PcztShape {
    pub tx_version: u32,
    pub consensus_branch_id: u32,
    pub transparent_inputs: usize,
    pub transparent_outputs: usize,
    pub sapling_spends: usize,
    pub sapling_outputs: usize,
    pub orchard_actions: usize,
    pub ironwood_actions: usize,
}

impl PcztShape {
    /// Read the shape of a parsed PCZT.
    pub fn of(pczt: &Pczt) -> Self {
        Self {
            tx_version: *pczt.global().tx_version(),
            consensus_branch_id: *pczt.global().consensus_branch_id(),
            transparent_inputs: if pczt.has_data_in_pool(PoolType::TRANSPARENT) {
                pczt.transparent().inputs().len()
            } else {
                0
            },
            transparent_outputs: pczt.transparent().outputs().len(),
            sapling_spends: pczt.sapling().spends().len(),
            sapling_outputs: pczt.sapling().outputs().len(),
            orchard_actions: pczt.orchard().actions().len(),
            ironwood_actions: pczt.ironwood().actions().len(),
        }
    }
}

/// Why a PCZT is not signable by this vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not a V6 (Ironwood-capable) transaction.
    TxVersion(u32),
    /// Branch not in the verified whitelist.
    Branch(u32),
    /// Nothing to sign.
    NoSpends,
    /// Orchard-pool actions are impossible for a fresh vault and always refused.
    OrchardActions(usize),
    /// Any Sapling part.
    Sapling { spends: usize, outputs: usize },
    /// Transparent inputs: FROST cannot produce an ECDSA signature.
    TransparentInputs(usize),
    /// Transparent outputs, including TEX addresses, are refused (see module docs).
    TransparentOutputs(usize),
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TxVersion(v) => write!(f, "transaction version {v} is not V6"),
            Self::Branch(b) => write!(f, "consensus branch 0x{b:08x} is not supported"),
            Self::NoSpends => f.write_str("PCZT has no Ironwood spend to authorize"),
            Self::OrchardActions(n) => write!(
                f,
                "{n} orchard-pool action(s) refused: a fresh vault holds no orchard notes and the \
                 orchard pool forbids cross-address transfers"
            ),
            Self::Sapling { spends, outputs } => {
                write!(
                    f,
                    "sapling part refused ({spends} spends, {outputs} outputs)"
                )
            }
            Self::TransparentInputs(n) => write!(
                f,
                "{n} transparent input(s) refused: a threshold vault cannot produce a secp256k1 \
                 ECDSA signature"
            ),
            Self::TransparentOutputs(n) => write!(
                f,
                "{n} transparent output(s) refused: shielded-only, and TEX addresses are \
                 transparent-source-only"
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// The policy. Everything is fail-closed: an unrecognised version, branch or pool part refuses.
pub fn check(shape: &PcztShape) -> Result<(), Refusal> {
    if shape.tx_version != TX_VERSION_V6 {
        return Err(Refusal::TxVersion(shape.tx_version));
    }
    if shape.consensus_branch_id != BRANCH_NU6_3 && shape.consensus_branch_id != BRANCH_NU7 {
        return Err(Refusal::Branch(shape.consensus_branch_id));
    }
    if shape.transparent_inputs > 0 {
        return Err(Refusal::TransparentInputs(shape.transparent_inputs));
    }
    if shape.transparent_outputs > 0 {
        return Err(Refusal::TransparentOutputs(shape.transparent_outputs));
    }
    if shape.sapling_spends > 0 || shape.sapling_outputs > 0 {
        return Err(Refusal::Sapling {
            spends: shape.sapling_spends,
            outputs: shape.sapling_outputs,
        });
    }
    if shape.orchard_actions > 0 {
        return Err(Refusal::OrchardActions(shape.orchard_actions));
    }
    if shape.ironwood_actions == 0 {
        return Err(Refusal::NoSpends);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ironwood_only() -> PcztShape {
        PcztShape {
            tx_version: TX_VERSION_V6,
            consensus_branch_id: BRANCH_NU6_3,
            ironwood_actions: 2,
            ..Default::default()
        }
    }

    #[test]
    fn accepts_a_plain_ironwood_transaction() {
        assert_eq!(check(&ironwood_only()), Ok(()));
    }

    #[test]
    fn accepts_nu7() {
        let mut shape = ironwood_only();
        shape.consensus_branch_id = BRANCH_NU7;
        assert_eq!(check(&shape), Ok(()));
    }

    #[test]
    fn refuses_v5() {
        let mut shape = ironwood_only();
        shape.tx_version = 5;
        assert_eq!(check(&shape), Err(Refusal::TxVersion(5)));
    }

    #[test]
    fn refuses_unknown_branch() {
        let mut shape = ironwood_only();
        shape.consensus_branch_id = 0xdead_beef;
        assert_eq!(check(&shape), Err(Refusal::Branch(0xdead_beef)));
    }

    #[test]
    fn refuses_orchard_actions_by_name() {
        let mut shape = ironwood_only();
        shape.orchard_actions = 1;
        assert_eq!(check(&shape), Err(Refusal::OrchardActions(1)));
    }

    #[test]
    fn refuses_transparent_inputs() {
        let mut shape = ironwood_only();
        shape.transparent_inputs = 1;
        assert_eq!(check(&shape), Err(Refusal::TransparentInputs(1)));
    }

    #[test]
    fn refuses_transparent_outputs_which_covers_tex() {
        // A TEX address is a transparent output; the wallet library itself refuses it when the
        // funds come from shielded (`PaysTexFromShielded`), so this is the same decision.
        let mut shape = ironwood_only();
        shape.transparent_outputs = 1;
        assert_eq!(check(&shape), Err(Refusal::TransparentOutputs(1)));
    }

    #[test]
    fn refuses_sapling() {
        let mut shape = ironwood_only();
        shape.sapling_outputs = 2;
        assert_eq!(
            check(&shape),
            Err(Refusal::Sapling {
                spends: 0,
                outputs: 2
            })
        );
    }

    #[test]
    fn refuses_a_pczt_with_nothing_to_sign() {
        let mut shape = ironwood_only();
        shape.ironwood_actions = 0;
        assert_eq!(check(&shape), Err(Refusal::NoSpends));
    }
}
