//! Independent verification and signature material for the vault signer tier.
//!
//! This is the tier a phone (or a second device) needs and nothing more: it parses a PCZT that a
//! coordinator produced, decides whether it is a transaction this vault is even allowed to sign,
//! recomputes the sighash itself, checks that every spend's `rk` really is the group key
//! re-randomized by the `alpha` it was handed, and — only after a human approves — produces a
//! signature share. It carries no transaction builder, no prover, no network client and no
//! broadcast path, so compromising it cannot silently redirect funds.
//!
//! Two rules come straight from the product decisions rather than from the protocol:
//!
//! * **Ironwood only.** A freshly created vault can never hold an Orchard note (since NU6.3 the
//!   Orchard pool forbids cross-address transfers), so an Orchard-pool action in a vault PCZT is
//!   either a bug or an attack. It is refused by name — see [`gate::Refusal::OrchardActions`].
//! * **No transparent, no Sapling.** FROST is Schnorr: a threshold vault cannot produce the
//!   secp256k1 ECDSA signature a transparent input needs, and a transparent output would break the
//!   value accounting the display gate depends on. TEX addresses are refused for the same reason
//!   the wallet library refuses them (`PaysTexFromShielded`): they are transparent-source-only.

pub mod apply;
pub mod gate;
pub mod material;
pub mod sighash;

pub use apply::{apply_all, apply_ironwood_signature, apply_material, ApplyError};
pub use gate::{check, PcztShape, Refusal};
pub use material::{bind_alphas, ironwood_action_rks, ActionRk, MaterialError, SpendMaterial};
pub use sighash::{shielded_sighash, verify_shielded_sighash, SighashError};
