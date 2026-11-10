//! FROST(Pallas, BLAKE2b-512) threshold signing for Zcash spend authorization.
//!
//! This is glue, not cryptography: DKG is `frost_core::keys::dkg::{part1,part2,part3}`, signing is
//! `frost_rerandomized`, and the group key is the RedPallas `ak` that Orchard/Ironwood spend auth
//! verifies against. No bespoke construction appears here.
//!
//! Two signing paths exist and they are not interchangeable:
//!
//! * [`sign_with_alpha`] takes an **explicit** randomizer. In Zcash this is the per-action `alpha`
//!   that the transaction builder already chose (it is what makes `rk = ak + alpha·G`), so the
//!   coordinator extracts it from the PCZT and ships it to signers. This is the path a vault uses.
//! * [`sign_with_randomizer_seed`] derives the randomizer from a seed + the commitments (the
//!   generic ZF flow). It is kept because it is the non-deprecated API and it demonstrates why the
//!   documented "derive the randomizer from commitments" flow does not apply to Orchard: Orchard's
//!   `alpha` is fixed at build time, before any commitment exists.

use std::collections::BTreeMap;

use frost_rerandomized::{RandomizedParams, Randomizer};
use rand_core::{CryptoRng, RngCore};
use reddsa::frost::redpallas::{frost as frost_core, PallasBlake2b512};

/// The ciphersuite: FROST over Pallas with BLAKE2b-512, i.e. RedPallas spend authorization.
pub type Suite = PallasBlake2b512;

pub type Identifier = frost_core::Identifier<Suite>;
pub type KeyPackage = frost_core::keys::KeyPackage<Suite>;
pub type PublicKeyPackage = frost_core::keys::PublicKeyPackage<Suite>;
pub type SigningNonces = frost_core::round1::SigningNonces<Suite>;
pub type SigningCommitments = frost_core::round1::SigningCommitments<Suite>;
pub type SignatureShare = frost_core::round2::SignatureShare<Suite>;

/// The three threshold modes the product supports. `(participants, threshold)`.
pub const SUPPORTED_MODES: [(u16, u16); 3] = [(2, 2), (3, 2), (3, 3)];

/// Largest participant count the protocol envelope can address. Roster indices are one byte, but
/// the product caps at 3, so anything above that is a caller bug rather than a wire concern.
pub const MAX_PARTICIPANTS: u16 = 3;

/// A 64-byte RedPallas spend authorization signature, i.e. what goes into `Orchard Action.spend_auth_sig`.
pub type SpendAuthSignature = [u8; 64];

#[derive(Debug)]
pub enum ThresholdError {
    UnsupportedMode { participants: u16, threshold: u16 },
    Frost(String),
    BadSignatureLength(usize),
}

impl std::fmt::Display for ThresholdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedMode { participants, threshold } => write!(
                f,
                "unsupported threshold mode {threshold}-of-{participants} (supported: 2-of-2, 2-of-3, 3-of-3)"
            ),
            Self::Frost(e) => write!(f, "frost: {e}"),
            Self::BadSignatureLength(n) => write!(f, "signature must be 64 bytes, got {n}"),
        }
    }
}

impl std::error::Error for ThresholdError {}

fn frost<E: std::fmt::Debug>(e: E) -> ThresholdError {
    ThresholdError::Frost(format!("{e:?}"))
}

/// Result of a completed DKG: every participant's key package plus the shared public package.
///
/// In real use none of these are ever co-located — each device keeps only its own `KeyPackage`.
/// The map exists so tests (and a coordinator-side simulation) can drive a full ceremony.
pub struct DkgOutput {
    pub participants: u16,
    pub threshold: u16,
    pub identifiers: Vec<Identifier>,
    pub key_packages: BTreeMap<Identifier, KeyPackage>,
    pub pubkeys: PublicKeyPackage,
}

impl DkgOutput {
    /// The group verifying key, i.e. `ak`: the 32-byte Orchard/Ironwood spend validating key.
    ///
    /// `ak` is the only value that crosses the boundary into the wallet stack (there it is spliced
    /// into an FVK as `ak ‖ nk ‖ rivk`, 96 bytes).
    pub fn group_verifying_key(&self) -> Result<[u8; 32], ThresholdError> {
        let bytes = self.pubkeys.verifying_key().serialize().map_err(frost)?;
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| ThresholdError::Frost("ak is not 32 bytes".into()))
    }

    /// Identifiers of the first `threshold` participants, i.e. a legal signing subset.
    pub fn default_signers(&self) -> Vec<Identifier> {
        self.identifiers
            .iter()
            .take(self.threshold as usize)
            .copied()
            .collect()
    }
}

/// Run a complete 3-round DKG among `participants` devices with threshold `threshold`.
///
/// Every participant really does run `part1`/`part2`/`part3` and only ever sees the round-1
/// broadcasts and the round-2 packages addressed to it. There is deliberately no dealer path: a
/// trusted dealer (or splitting an existing `ask`) would contradict the vault's core promise that
/// the full spend authority never exists in one place.
pub fn run_dkg<R>(
    participants: u16,
    threshold: u16,
    rng: &mut R,
) -> Result<DkgOutput, ThresholdError>
where
    R: CryptoRng + RngCore,
{
    if !SUPPORTED_MODES.contains(&(participants, threshold)) {
        return Err(ThresholdError::UnsupportedMode {
            participants,
            threshold,
        });
    }

    let identifiers: Vec<Identifier> = (1..=participants)
        .map(|i| Identifier::try_from(i).expect("1..=3 is a valid identifier"))
        .collect();

    // Round 1 — every participant broadcasts a public package.
    let mut round1_secret = BTreeMap::new();
    let mut round1_packages = BTreeMap::new();
    for id in &identifiers {
        let (secret, package) =
            frost_core::keys::dkg::part1(*id, participants, threshold, &mut *rng).map_err(frost)?;
        round1_secret.insert(*id, secret);
        round1_packages.insert(*id, package);
    }

    // Round 2 — every participant sends one encrypted package to each peer. `inbox` models the
    // per-recipient channel: only packages addressed to `id` land there.
    let mut round2_secret = BTreeMap::new();
    let mut inbox: BTreeMap<
        Identifier,
        BTreeMap<Identifier, frost_core::keys::dkg::round2::Package<Suite>>,
    > = BTreeMap::new();
    for id in &identifiers {
        let peers: BTreeMap<_, _> = round1_packages
            .iter()
            .filter(|(peer, _)| *peer != id)
            .map(|(peer, pkg)| (*peer, pkg.clone()))
            .collect();
        let secret = round1_secret.remove(id).expect("round 1 secret present");
        let (round2_secret_package, outgoing) =
            frost_core::keys::dkg::part2(secret, &peers).map_err(frost)?;
        round2_secret.insert(*id, round2_secret_package);
        for (recipient, package) in outgoing {
            inbox.entry(recipient).or_default().insert(*id, package);
        }
    }

    // Round 3 — each participant finalizes with what it received.
    let mut key_packages = BTreeMap::new();
    let mut pubkeys: Option<PublicKeyPackage> = None;
    for id in &identifiers {
        let peers: BTreeMap<_, _> = round1_packages
            .iter()
            .filter(|(peer, _)| *peer != id)
            .map(|(peer, pkg)| (*peer, pkg.clone()))
            .collect();
        let received = inbox.remove(id).expect("round 2 inbox present");
        let secret = round2_secret.remove(id).expect("round 2 secret present");
        let (key_package, public_key_package) =
            frost_core::keys::dkg::part3(&secret, &peers, &received).map_err(frost)?;

        // Every participant must land on the same group key, or the sweep/outputs would be bound
        // to different verification keys. This is the check §5.9 describes as mandatory.
        match &pubkeys {
            None => pubkeys = Some(public_key_package),
            Some(expected) => {
                if expected.verifying_key() != public_key_package.verifying_key() {
                    return Err(ThresholdError::Frost(
                        "participants disagree on the group verifying key".into(),
                    ));
                }
            }
        }
        key_packages.insert(*id, key_package);
    }

    Ok(DkgOutput {
        participants,
        threshold,
        identifiers,
        key_packages,
        pubkeys: pubkeys.expect("at least one participant"),
    })
}

// ---------------------------------------------------------------------------
// DKG, one round at a time.
//
// `run_dkg` drives every participant in one process, which is only useful for tests. A device in the
// field runs exactly one participant and moves serialized state across the wire between rounds, so
// each round needs its own entry point and its own byte encoding.
//
// Slots are 0-based roster indices; the FROST identifier for a slot is `slot + 1`, matching
// `run_dkg`. Keeping identifiers out of the API means a frontend never has to shuttle them around.
// ---------------------------------------------------------------------------

/// FROST identifier for a roster slot. Both the DKG rounds and the session use this mapping.
pub fn slot_identifier(slot: u8) -> Result<Identifier, ThresholdError> {
    if (slot as u16) >= MAX_PARTICIPANTS {
        return Err(ThresholdError::UnsupportedMode {
            participants: slot as u16 + 1,
            threshold: 1,
        });
    }
    Identifier::try_from(slot as u16 + 1).map_err(|_| ThresholdError::UnsupportedMode {
        participants: slot as u16 + 1,
        threshold: 1,
    })
}

/// Round 1 output: a secret to keep and a broadcast to send everyone.
pub struct DkgRound1 {
    pub secret: Vec<u8>,
    pub broadcast: Vec<u8>,
}

/// Round 2 output: a secret to keep and one package per peer, keyed by that peer's slot.
pub struct DkgRound2 {
    pub secret: Vec<u8>,
    pub outgoing: BTreeMap<u8, Vec<u8>>,
}

/// Round 3 output: this device's key package and the group's public package.
pub struct DkgFinal {
    pub key_package: Vec<u8>,
    pub public_key_package: Vec<u8>,
}

/// Round 1 of the DKG for one device.
pub fn dkg_part1<R>(
    participants: u16,
    threshold: u16,
    slot: u8,
    rng: &mut R,
) -> Result<DkgRound1, ThresholdError>
where
    R: CryptoRng + RngCore,
{
    if !SUPPORTED_MODES.contains(&(participants, threshold)) {
        return Err(ThresholdError::UnsupportedMode {
            participants,
            threshold,
        });
    }
    let identifier = slot_identifier(slot)?;
    let (secret, package) =
        frost_core::keys::dkg::part1(identifier, participants, threshold, &mut *rng)
            .map_err(frost)?;
    Ok(DkgRound1 {
        secret: secret.serialize().map_err(frost)?,
        broadcast: package.serialize().map_err(frost)?,
    })
}

/// Round 2: produce one sealed package per peer from the round-1 broadcasts.
pub fn dkg_part2(
    secret: &[u8],
    peer_broadcasts: &BTreeMap<u8, Vec<u8>>,
) -> Result<DkgRound2, ThresholdError> {
    let secret = frost_core::keys::dkg::round1::SecretPackage::<Suite>::deserialize(secret)
        .map_err(frost)?;
    let mut peers = BTreeMap::new();
    for (slot, bytes) in peer_broadcasts {
        let package =
            frost_core::keys::dkg::round1::Package::<Suite>::deserialize(bytes).map_err(frost)?;
        peers.insert(slot_identifier(*slot)?, package);
    }
    let (secret, outgoing) = frost_core::keys::dkg::part2(secret, &peers).map_err(frost)?;
    let mut by_slot = BTreeMap::new();
    for (identifier, package) in outgoing {
        by_slot.insert(slot_of(identifier)?, package.serialize().map_err(frost)?);
    }
    Ok(DkgRound2 {
        secret: secret.serialize().map_err(frost)?,
        outgoing: by_slot,
    })
}

/// Round 3: finalize with the broadcasts and the packages addressed to this device.
pub fn dkg_part3(
    secret: &[u8],
    peer_broadcasts: &BTreeMap<u8, Vec<u8>>,
    incoming: &BTreeMap<u8, Vec<u8>>,
) -> Result<DkgFinal, ThresholdError> {
    let secret = frost_core::keys::dkg::round2::SecretPackage::<Suite>::deserialize(secret)
        .map_err(frost)?;
    let mut broadcasts = BTreeMap::new();
    for (slot, bytes) in peer_broadcasts {
        let package =
            frost_core::keys::dkg::round1::Package::<Suite>::deserialize(bytes).map_err(frost)?;
        broadcasts.insert(slot_identifier(*slot)?, package);
    }
    let mut received = BTreeMap::new();
    for (slot, bytes) in incoming {
        let package =
            frost_core::keys::dkg::round2::Package::<Suite>::deserialize(bytes).map_err(frost)?;
        received.insert(slot_identifier(*slot)?, package);
    }
    let (key_package, public_key_package) =
        frost_core::keys::dkg::part3(&secret, &broadcasts, &received).map_err(frost)?;
    Ok(DkgFinal {
        key_package: key_package.serialize().map_err(frost)?,
        public_key_package: public_key_package.serialize().map_err(frost)?,
    })
}

/// Inverse of [`slot_identifier`]: recover the roster slot from an identifier.
pub fn slot_of(identifier: Identifier) -> Result<u8, ThresholdError> {
    for slot in 0..MAX_PARTICIPANTS {
        let slot_u8 = slot as u8;
        if slot_identifier(slot_u8)? == identifier {
            return Ok(slot_u8);
        }
    }
    Err(ThresholdError::Frost(format!(
        "identifier {identifier:?} is not a roster slot"
    )))
}

/// One signer's round-1 output: its nonces (secret, local only) and commitments (broadcast).
pub struct Commitment {
    pub identifier: Identifier,
    pub nonces: SigningNonces,
    pub commitments: SigningCommitments,
}

/// Round 1 of signing: generate fresh nonces and commitments for **one** (pool, action).
///
/// A caller signing `m` actions must call this `m` times: reusing one nonce set across two
/// challenges leaks the signing share. With a shared sighash and a per-action randomizer, two
/// actions *are* two different challenges.
pub fn commit<R>(key_package: &KeyPackage, rng: &mut R) -> Commitment
where
    R: CryptoRng + RngCore,
{
    let (nonces, commitments) = frost_core::round1::commit(key_package.signing_share(), rng);
    Commitment {
        identifier: *key_package.identifier(),
        nonces,
        commitments,
    }
}

/// Round 2, explicit randomizer (the Orchard `alpha` path).
///
/// The signer recomputes `rk = ak + alpha·G` and checks it against the PCZT before calling this;
/// see `pczt-signer-core` for that gate. Here we only bind the share to (commitments, message,
/// alpha).
pub fn sign_with_alpha(
    nonces: &SigningNonces,
    key_package: &KeyPackage,
    commitments: &BTreeMap<Identifier, SigningCommitments>,
    message: &[u8],
    alpha: &[u8; 32],
) -> Result<SignatureShare, ThresholdError> {
    let signing_package = frost_core::SigningPackage::new(commitments.clone(), message);
    let randomizer = Randomizer::<Suite>::deserialize(alpha).map_err(frost)?;
    // `sign` is deprecated in favour of `sign_with_randomizer_seed`, but the replacement derives
    // alpha from the seed and the commitments, whereas Orchard needs the alpha the transaction was
    // already built with. Keeping the explicit-randomizer entry point is the whole point.
    #[allow(deprecated)]
    frost_rerandomized::sign(&signing_package, nonces, key_package, randomizer).map_err(frost)
}

/// Round 2, seed-derived randomizer (the generic ZF re-randomized FROST flow).
pub fn sign_with_randomizer_seed(
    nonces: &SigningNonces,
    key_package: &KeyPackage,
    commitments: &BTreeMap<Identifier, SigningCommitments>,
    message: &[u8],
    randomizer_seed: &[u8],
) -> Result<SignatureShare, ThresholdError> {
    let signing_package = frost_core::SigningPackage::new(commitments.clone(), message);
    frost_rerandomized::sign_with_randomizer_seed(
        &signing_package,
        nonces,
        key_package,
        randomizer_seed,
    )
    .map_err(frost)
}

/// Coordinator aggregation with an explicit randomizer. Returns the 64-byte spend auth signature.
pub fn aggregate_with_alpha(
    pubkeys: &PublicKeyPackage,
    commitments: &BTreeMap<Identifier, SigningCommitments>,
    message: &[u8],
    alpha: &[u8; 32],
    shares: &BTreeMap<Identifier, SignatureShare>,
) -> Result<SpendAuthSignature, ThresholdError> {
    let signing_package = frost_core::SigningPackage::new(commitments.clone(), message);
    let randomizer = Randomizer::<Suite>::deserialize(alpha).map_err(frost)?;
    let params = RandomizedParams::from_randomizer(pubkeys.verifying_key(), randomizer);
    let signature =
        frost_rerandomized::aggregate(&signing_package, shares, pubkeys, &params).map_err(frost)?;
    let bytes = signature.serialize().map_err(frost)?;
    let len = bytes.len();
    bytes
        .try_into()
        .map_err(|_| ThresholdError::BadSignatureLength(len))
}

/// Coordinator aggregation for the seed-derived path.
pub fn aggregate_with_randomizer_seed(
    pubkeys: &PublicKeyPackage,
    commitments: &BTreeMap<Identifier, SigningCommitments>,
    message: &[u8],
    randomizer_seed: &[u8],
    shares: &BTreeMap<Identifier, SignatureShare>,
) -> Result<(SpendAuthSignature, [u8; 32]), ThresholdError> {
    let signing_package = frost_core::SigningPackage::new(commitments.clone(), message);
    let params = RandomizedParams::regenerate_from_seed_and_commitments(
        pubkeys.verifying_key(),
        randomizer_seed,
        commitments,
    )
    .map_err(frost)?;
    let rk = params
        .randomized_verifying_key()
        .serialize()
        .map_err(frost)?;
    let signature =
        frost_rerandomized::aggregate(&signing_package, shares, pubkeys, &params).map_err(frost)?;
    let bytes = signature.serialize().map_err(frost)?;
    let len = bytes.len();
    let sig = bytes
        .try_into()
        .map_err(|_| ThresholdError::BadSignatureLength(len))?;
    let rk: [u8; 32] = rk
        .as_slice()
        .try_into()
        .map_err(|_| ThresholdError::Frost("rk is not 32 bytes".into()))?;
    Ok((sig, rk))
}

/// Reconstruct `rk = ak + alpha·G` and verify `signature` over `message`.
///
/// Going through `RandomizedParams` (rather than trusting a caller-supplied `rk`) is what lets a
/// signer check that the PCZT's `rk` really is the group key randomized by the `alpha` it was told
/// about — see the §8.6 correction in the review notes.
pub fn verify_with_group_key_and_alpha(
    group_verifying_key: &[u8; 32],
    alpha: &[u8; 32],
    message: &[u8],
    signature: &SpendAuthSignature,
) -> Result<(), ThresholdError> {
    let ak = frost_core::VerifyingKey::<Suite>::deserialize(group_verifying_key).map_err(frost)?;
    let randomizer = Randomizer::<Suite>::deserialize(alpha).map_err(frost)?;
    let params = RandomizedParams::from_randomizer(&ak, randomizer);
    let sig = frost_core::Signature::<Suite>::deserialize(signature).map_err(frost)?;
    params
        .randomized_verifying_key()
        .verify(message, &sig)
        .map_err(frost)
}

/// Randomized verification key for a given `ak` and `alpha`, i.e. the `rk` a PCZT must carry.
pub fn randomized_verifying_key(
    group_verifying_key: &[u8; 32],
    alpha: &[u8; 32],
) -> Result<[u8; 32], ThresholdError> {
    let ak = frost_core::VerifyingKey::<Suite>::deserialize(group_verifying_key).map_err(frost)?;
    let randomizer = Randomizer::<Suite>::deserialize(alpha).map_err(frost)?;
    let params = RandomizedParams::from_randomizer(&ak, randomizer);
    let rk = params
        .randomized_verifying_key()
        .serialize()
        .map_err(frost)?;
    rk.as_slice()
        .try_into()
        .map_err(|_| ThresholdError::Frost("rk is not 32 bytes".into()))
}
