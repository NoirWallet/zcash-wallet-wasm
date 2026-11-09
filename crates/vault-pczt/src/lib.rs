//! Coordinator-side PCZT construction for the FROST Secure Vault.
//!
//! The coordinator builds and proves the transaction; the signer only verifies and signs. That split
//! is what makes the signer tier small enough to audit: nothing here is linked on the signer side.
//!
//! Three decisions are encoded here, from the review notes:
//!
//! 1. **Proofs are stripped before the request goes out.** A one-action Ironwood proof is 4992
//!    bytes (`Proof::expected_proof_size(n) = 2720 + 2272n`), larger than the rest of the PCZT put
//!    together. The signer verifies displayed amounts against note commitments, which needs no
//!    proof, so the transport copy drops them.
//! 2. **The coordinator keeps its own proven copy.** Signatures are applied to that copy, never to
//!    the stripped transport copy, so the transaction that gets broadcast is the one that was
//!    proven.
//! 3. **Stripping must not change the sighash.** The signature is over the shielded digest; if
//!    removing a proof moved that digest, the signer would be authorizing a different transaction
//!    than the one that gets built. [`Prepared::prepare`] asserts it instead of assuming it.

use ff::PrimeField;
use orchard::circuit::{OrchardCircuitVersion, ProvingKey};
use orchard::keys::FullViewingKey;
use pczt::roles::prover::Prover;
use pczt::roles::redactor::Redactor;
use pczt::roles::signer::Signer;
use pczt::roles::verifier::Verifier;
use pczt::Pczt;

#[derive(Debug)]
pub enum CreateError {
    /// The FVK bytes did not parse.
    InvalidViewingKey,
    /// Proving failed (the transaction was not internally consistent).
    Proving,
    /// The Verifier/Signer role could not read the transaction.
    Unreadable,
}

impl core::fmt::Display for CreateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidViewingKey => f.write_str("full viewing key bytes did not parse"),
            Self::Proving => f.write_str("proving failed"),
            Self::Unreadable => f.write_str("the PCZT could not be read by the verifier role"),
        }
    }
}

impl std::error::Error for CreateError {}

/// Everything the coordinator needs to run a signing session, plus the proven copy it keeps.
pub struct Prepared {
    /// Proofless PCZT for transport: bytes, ready to be framed into QR or BLE.
    pub transport: Vec<u8>,
    /// Proven PCZT. Signatures are applied to this one.
    pub kept: Pczt,
    /// The shielded sighash, identical in both copies.
    pub sighash: [u8; 32],
    /// Per-action randomizers, as baked into the transaction.
    pub alphas: Vec<[u8; 32]>,
    /// Sizes, so the caller can log or gate on them.
    pub proven_len: usize,
    pub transport_len: usize,
}

/// The Ironwood proving key. Building it runs the circuit's keygen, so it is done once per process.
pub fn ironwood_proving_key() -> &'static ProvingKey {
    static PK: std::sync::OnceLock<ProvingKey> = std::sync::OnceLock::new();
    PK.get_or_init(|| ProvingKey::build(OrchardCircuitVersion::PostNu6_3))
}

/// Prove every Ironwood action of an unproven PCZT.
pub fn prove_ironwood<R: rand::Rng + rand::CryptoRng>(
    pczt: Pczt,
    rng: R,
) -> Result<Pczt, CreateError> {
    let prover = Prover::new(pczt);
    if !prover.requires_ironwood_proof() {
        return Ok(prover.finish());
    }
    prover
        .create_ironwood_proof(rng, ironwood_proving_key())
        .map(|prover| prover.finish())
        .map_err(|_| CreateError::Proving)
}

/// Drop every zk proof from the transport copy.
pub fn strip_proofs(pczt: Pczt) -> Pczt {
    Redactor::new(pczt)
        .redact_ironwood_with(|mut bundle| bundle.clear_zkproof())
        .redact_orchard_with(|mut bundle| bundle.clear_zkproof())
        .finish()
}

/// Read each Ironwood action's randomizer. This is the coordinator's half of the §8.6 correction:
/// `alpha` is not derived from commitments, it is read from the transaction that was already built.
pub fn extract_alphas(pczt: &Pczt) -> Result<Vec<[u8; 32]>, CreateError> {
    let verifier = Verifier::new(pczt.clone());
    let mut alphas = Vec::new();
    verifier
        .with_ironwood::<(), _>(|bundle| {
            for action in bundle.actions() {
                let alpha = action
                    .spend()
                    .alpha()
                    .ok_or(pczt::roles::verifier::OrchardError::Custom(()))?;
                // The scalar type belongs to orchard's `pasta_curves`, which is not re-exported;
                // `PrimeField` is in scope so `to_repr` resolves without naming it.
                alphas.push(alpha.to_repr());
            }
            Ok(())
        })
        .map_err(|_| CreateError::Unreadable)?;
    Ok(alphas)
}

/// The shielded sighash of a PCZT, recomputed by the Signer role.
pub fn shielded_sighash(pczt: Pczt) -> Result<[u8; 32], CreateError> {
    Ok(Signer::new(pczt)
        .map_err(|_| CreateError::Unreadable)?
        .shielded_sighash())
}

/// Turn a proven PCZT into what the coordinator sends and what it keeps.
///
/// Fails if stripping changes the sighash — that would mean the signer authorizes one transaction
/// and the network sees another.
pub fn prepare(proven: Pczt) -> Result<Prepared, CreateError> {
    let sighash = shielded_sighash(proven.clone())?;
    let alphas = extract_alphas(&proven)?;
    let proven_len = proven
        .clone()
        .serialize()
        .map_err(|_| CreateError::Proving)?
        .len();

    let stripped = strip_proofs(proven.clone());
    let transport = stripped
        .clone()
        .serialize()
        .map_err(|_| CreateError::Proving)?;

    let stripped_sighash = shielded_sighash(stripped)?;
    if stripped_sighash != sighash {
        return Err(CreateError::Unreadable);
    }

    Ok(Prepared {
        transport_len: transport.len(),
        transport,
        kept: proven,
        sighash,
        alphas,
        proven_len,
    })
}

/// Parse a `ak ‖ nk ‖ rivk` blob into an Orchard FVK.
pub fn fvk_from_bytes(fvk: &[u8; 96]) -> Result<FullViewingKey, CreateError> {
    FullViewingKey::from_bytes(fvk).ok_or(CreateError::InvalidViewingKey)
}

/// How many QR frames a payload needs at a given frame size, rounded up.
pub fn frames(len: usize, frame_size: usize) -> usize {
    if frame_size == 0 {
        return 0;
    }
    len.div_ceil(frame_size)
}
