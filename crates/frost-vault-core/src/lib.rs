//! FROST threshold signing protocol core for the Noir FROST Secure Vault.
//!
//! This crate is deliberately free of any Zcash dependency: it is the protocol tier that both the
//! Chrome Extension (wasm) and the mobile signer (native) link against. Everything that needs the
//! wallet stack — PCZT verification, `ak → FVK → address`, pool migration — belongs to a higher
//! tier, because the signer must not carry a transaction builder or a broadcast path.
//!
//! Scope decisions this crate encodes (see `local-docs/frost-vault-review-notes.md`):
//!
//! * Ironwood is the only supported value pool; Orchard appears solely as a migration source. The
//!   two share key material and the spend-auth primitive, so nothing here is pool-specific beyond
//!   the pool tag carried in messages.
//! * `transparent` is rejected: FROST is Schnorr, so a threshold vault can never produce a
//!   secp256k1 ECDSA signature over a transparent input.
//! * Existing wallets are not imported. There is therefore no `split()` path: a vault's spend
//!   authority is only ever produced by a real multi-party DKG.
//!
//! What lives here:
//!
//! * [`threshold`] — the DKG and threshold-signing wrappers over `frost-core` / `frost-rerandomized`
//!   for 2-of-2, 2-of-3 and 3-of-3, including the explicit-randomizer (Orchard `alpha`) path.
//! * [`protocol`] — the wire envelope. Deliberately separate from the Keystone PCZT UR types: the
//!   Keystone envelope binds no session, group, sender or round, so it cannot carry FROST rounds.
//! * [`state`] — the DKG and signing session state machines, plus per-spend nonce lifecycle.

pub mod custody;
pub mod protocol;
pub mod session;
pub mod state;
pub mod threshold;

pub use custody::{Recovery, SessionGuard, SessionProgress, ShareBackup, ShareIdentity};
pub use session::{CoordinatorSession, SessionConfig, SessionError, SignerSession};
pub use threshold::{DkgOutput, Suite};
