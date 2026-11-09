//! `KEYSET_SETUP` for the FROST Secure Vault: group `ak` → FVK → address → UFVK.
//!
//! Kept apart from the wallet tier because every device needs it, including a phone that has no
//! database, no network client and no transaction builder.

pub mod keyset;

pub use keyset::{KeysetError, QuantumRecoveryMaterial, VaultKeyset};
