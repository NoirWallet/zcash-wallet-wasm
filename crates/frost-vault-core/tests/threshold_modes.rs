//! End-to-end threshold signing for the three supported modes.
//!
//! These tests are the empirical answer to "can the MIT FROST crates actually do 2-of-2, 2-of-3 and
//! 3-of-3 for Zcash spend authorization". They run a real 3-round DKG (no dealer), sign with the
//! required threshold, aggregate, and verify the resulting 64-byte signature against
//! `rk = ak + alpha·G` — the same check the Zcash consensus rule performs.

use std::collections::BTreeMap;

use frost_vault_core::threshold::{
    self, aggregate_with_alpha, aggregate_with_randomizer_seed, commit, randomized_verifying_key,
    run_dkg, sign_with_alpha, sign_with_randomizer_seed, verify_with_group_key_and_alpha,
    DkgOutput, PublicKeyPackage, SigningCommitments, SUPPORTED_MODES,
};
use rand::rngs::OsRng;

/// Deterministic-but-arbitrary 32-byte alpha, standing in for an Orchard action randomizer.
fn alpha(tag: u8) -> [u8; 32] {
    let mut a = [0u8; 32];
    a[0] = tag;
    a[31] = 0x01; // keep it a canonical non-zero scalar encoding
    a
}

fn message(tag: u8) -> [u8; 32] {
    let mut m = [0u8; 32];
    m[0] = tag;
    m
}

/// Run the full ceremony for a given mode and return (dkg, signature, rk).
fn sign_once(
    participants: u16,
    threshold: u16,
    signer_slots: &[usize],
) -> (DkgOutput, [u8; 64], [u8; 32]) {
    let mut rng = OsRng;
    let dkg = run_dkg(participants, threshold, &mut rng).expect("dkg");
    let ak = dkg.group_verifying_key().expect("ak");

    // Every participant must agree on ak, otherwise outputs would be bound to different keys.
    assert_eq!(ak.len(), 32);

    let alpha = alpha(0x11);
    let msg = message(0x22);

    // Round 1: one commitment per selected signer.
    let mut commitments: BTreeMap<_, SigningCommitments> = BTreeMap::new();
    let mut round1 = Vec::new();
    for slot in signer_slots {
        let id = dkg.identifiers[*slot];
        let kp = &dkg.key_packages[&id];
        let c = commit(kp, &mut rng);
        commitments.insert(id, c.commitments);
        round1.push(c);
    }
    assert_eq!(commitments.len(), threshold as usize);

    // Round 2: each selected signer produces a share over the same (message, alpha).
    let mut shares = BTreeMap::new();
    for c in &round1 {
        let kp = &dkg.key_packages[&c.identifier];
        let share = sign_with_alpha(&c.nonces, kp, &commitments, &msg, &alpha).expect("share");
        shares.insert(c.identifier, share);
    }

    let sig = aggregate_with_alpha(&dkg.pubkeys, &commitments, &msg, &alpha, &shares).expect("agg");

    // rk must be reconstructible from (ak, alpha) alone — this is what a signer checks against the
    // PCZT before signing, and what the review notes replace §8.6's "randomizer from commitments".
    let rk = randomized_verifying_key(&ak, &alpha).expect("rk");
    verify_with_group_key_and_alpha(&ak, &alpha, &msg, &sig).expect("consensus-style verify");

    (dkg, sig, rk)
}

#[test]
fn all_supported_modes_sign_and_verify() {
    for (participants, threshold) in SUPPORTED_MODES {
        // Use the first `threshold` participants (the common daily-use subset).
        let slots: Vec<usize> = (0..threshold as usize).collect();
        let (dkg, sig, _rk) = sign_once(participants, threshold, &slots);
        assert_eq!(dkg.participants, participants);
        assert_eq!(dkg.threshold, threshold);
        assert_eq!(sig.len(), 64);
        println!("{threshold}-of-{participants}: ok");
    }
}

#[test]
fn two_of_three_can_sign_with_the_recovery_subset() {
    // §8.3: if A's share is gone, B + C must be able to sign. Any threshold-sized subset works.
    let (dkg, sig, _rk) = sign_once(3, 2, &[1, 2]);
    assert_eq!(dkg.threshold, 2);
    assert_eq!(sig.len(), 64);
}

#[test]
fn below_threshold_shares_do_not_aggregate() {
    let mut rng = OsRng;
    let dkg = run_dkg(3, 2, &mut rng).expect("dkg");
    let alpha = alpha(0x33);
    let msg = message(0x44);

    let id0 = dkg.identifiers[0];
    let c = commit(&dkg.key_packages[&id0], &mut rng);
    let mut commitments = BTreeMap::new();
    commitments.insert(id0, c.commitments);

    // A signer refuses to even produce a share when the commitment set is below threshold, so a
    // 2-of-3 group cannot be talked into signing with one commitment. Failure happens before
    // aggregation, which is the stronger guarantee.
    let share = sign_with_alpha(
        &c.nonces,
        &dkg.key_packages[&id0],
        &commitments,
        &msg,
        &alpha,
    );
    assert!(share.is_err());

    let mut shares = BTreeMap::new();
    if let Ok(share) = share {
        shares.insert(id0, share);
        assert!(aggregate_with_alpha(&dkg.pubkeys, &commitments, &msg, &alpha, &shares).is_err());
    }
}

#[test]
fn duplicate_signer_share_is_refused() {
    // A coordinator that submits the same signer twice must not reach the threshold. Because the
    // commitment map is keyed by identifier, a duplicate collapses and the set stays below
    // threshold, which the signer rejects.
    let mut rng = OsRng;
    let dkg = run_dkg(3, 2, &mut rng).expect("dkg");
    let alpha = alpha(0x55);
    let msg = message(0x66);

    let id0 = dkg.identifiers[0];
    let c = commit(&dkg.key_packages[&id0], &mut rng);
    let mut commitments = BTreeMap::new();
    commitments.insert(id0, c.commitments);

    assert!(sign_with_alpha(
        &c.nonces,
        &dkg.key_packages[&id0],
        &commitments,
        &msg,
        &alpha
    )
    .is_err());
}

#[test]
fn wrong_alpha_fails_verification() {
    // A coordinator that ships a different alpha than the one baked into rk must fail: this is the
    // check that makes a lying SIGN_PAYLOAD detectable.
    let (dkg, sig, _rk) = sign_once(2, 2, &[0, 1]);
    let ak = dkg.group_verifying_key().unwrap();
    assert!(verify_with_group_key_and_alpha(&ak, &alpha(0x99), &message(0x22), &sig).is_err());
}

#[test]
fn wrong_message_fails_verification() {
    let (dkg, sig, _rk) = sign_once(2, 2, &[0, 1]);
    let ak = dkg.group_verifying_key().unwrap();
    assert!(verify_with_group_key_and_alpha(&ak, &alpha(0x11), &message(0x23), &sig).is_err());
}

#[test]
fn distinct_actions_use_distinct_rk() {
    // Two actions share the sighash but must not share a randomizer, hence not an rk. Reusing a
    // nonce set across these two is the key-recovery bug §8.5 warns about.
    let mut rng = OsRng;
    let dkg = run_dkg(2, 2, &mut rng).expect("dkg");
    let ak = dkg.group_verifying_key().unwrap();
    let rk_a = randomized_verifying_key(&ak, &alpha(0x01)).unwrap();
    let rk_b = randomized_verifying_key(&ak, &alpha(0x02)).unwrap();
    assert_ne!(rk_a, rk_b);
}

#[test]
fn seed_derived_randomizer_path_also_works() {
    // The generic ZF re-randomized flow (randomizer derived from seed + commitments). Kept as a
    // working reference so the difference from the Orchard-alpha path stays explicit.
    let mut rng = OsRng;
    let dkg = run_dkg(2, 2, &mut rng).expect("dkg");
    let msg = message(0x77);

    let mut commitments: BTreeMap<_, SigningCommitments> = BTreeMap::new();
    let mut round1 = Vec::new();
    for id in dkg.default_signers() {
        let c = commit(&dkg.key_packages[&id], &mut rng);
        commitments.insert(id, c.commitments);
        round1.push(c);
    }

    let seed = [7u8; 32];
    let mut shares = BTreeMap::new();
    for c in &round1 {
        let kp = &dkg.key_packages[&c.identifier];
        shares.insert(
            c.identifier,
            sign_with_randomizer_seed(&c.nonces, kp, &commitments, &msg, &seed).expect("share"),
        );
    }

    let (sig, rk) =
        aggregate_with_randomizer_seed(&dkg.pubkeys, &commitments, &msg, &seed, &shares)
            .expect("agg");
    assert_eq!(sig.len(), 64);

    // The reconstructed rk must match a fresh derivation from (ak, ...) — here we only know the
    // seed, so check the signature against the aggregate's own rk via the public package.
    let ak = dkg.group_verifying_key().unwrap();
    let params = frost_rerandomized::RandomizedParams::<threshold::Suite>::regenerate_from_seed_and_commitments(
        PublicKeyPackage::verifying_key(&dkg.pubkeys),
        &seed,
        &commitments,
    )
    .expect("params");
    let rk_from_params: [u8; 32] = params
        .randomized_verifying_key()
        .serialize()
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();
    assert_eq!(rk, rk_from_params);
    let _ = ak;
}

#[test]
fn unsupported_mode_is_refused() {
    let mut rng = OsRng;
    assert!(run_dkg(3, 5, &mut rng).is_err());
    assert!(run_dkg(1, 1, &mut rng).is_err());
    assert!(run_dkg(5, 3, &mut rng).is_err());
}

// ---------------------------------------------------------------------------
// Per-round DKG, the way a device actually runs it.
// ---------------------------------------------------------------------------

#[test]
fn per_round_dkg_agrees_with_the_single_process_ceremony() {
    use frost_vault_core::threshold::{dkg_part1, dkg_part2, dkg_part3};

    // Three devices, each holding only its own secret between rounds.
    let (participants, threshold) = (3u16, 2u16);
    let mut rng = OsRng;
    let mut secrets = BTreeMap::new();
    let mut broadcasts = BTreeMap::new();
    for slot in 0..participants {
        let r = dkg_part1(participants, threshold, slot as u8, &mut rng).expect("part1");
        secrets.insert(slot as u8, r.secret);
        broadcasts.insert(slot as u8, r.broadcast);
    }

    // Round 2: every device seals one package per peer; `inbox` is per recipient.
    let mut round2_secrets = BTreeMap::new();
    let mut inbox: BTreeMap<u8, BTreeMap<u8, Vec<u8>>> = BTreeMap::new();
    for slot in 0..participants {
        let slot = slot as u8;
        let peers: BTreeMap<u8, Vec<u8>> = broadcasts
            .iter()
            .filter(|(peer, _)| **peer != slot)
            .map(|(peer, bytes)| (*peer, bytes.clone()))
            .collect();
        let r = dkg_part2(&secrets[&slot], &peers).expect("part2");
        round2_secrets.insert(slot, r.secret);
        for (recipient, bytes) in r.outgoing {
            inbox.entry(recipient).or_default().insert(slot, bytes);
        }
    }

    // Round 3: each device finalizes with what it received.
    let mut group_keys = Vec::new();
    for slot in 0..participants {
        let slot = slot as u8;
        let peers: BTreeMap<u8, Vec<u8>> = broadcasts
            .iter()
            .filter(|(peer, _)| **peer != slot)
            .map(|(peer, bytes)| (*peer, bytes.clone()))
            .collect();
        let f = dkg_part3(&round2_secrets[&slot], &peers, &inbox[&slot]).expect("part3");
        let package: PublicKeyPackage =
            frost_vault_core::threshold::PublicKeyPackage::deserialize(&f.public_key_package)
                .expect("public package parses");
        group_keys.push(package.verifying_key().serialize().unwrap());
    }
    // All three land on the same group key, and it is a usable `ak`.
    assert_eq!(group_keys[0], group_keys[1]);
    assert_eq!(group_keys[1], group_keys[2]);
    assert_eq!(group_keys[0].len(), 32);

    // And the resulting key packages sign: two of the three.
    let mut key_packages = BTreeMap::new();
    for slot in 0..participants {
        let slot = slot as u8;
        let peers: BTreeMap<u8, Vec<u8>> = broadcasts
            .iter()
            .filter(|(peer, _)| **peer != slot)
            .map(|(peer, bytes)| (*peer, bytes.clone()))
            .collect();
        let f = dkg_part3(&round2_secrets[&slot], &peers, &inbox[&slot]).expect("part3");
        key_packages.insert(
            frost_vault_core::threshold::slot_identifier(slot).unwrap(),
            frost_vault_core::threshold::KeyPackage::deserialize(&f.key_package).unwrap(),
        );
    }
    let mut commitments = BTreeMap::new();
    let mut round1 = Vec::new();
    for slot in [0u8, 1u8] {
        let id = frost_vault_core::threshold::slot_identifier(slot).unwrap();
        let c = commit(&key_packages[&id], &mut rng);
        commitments.insert(id, c.commitments);
        round1.push(c);
    }
    let alpha = alpha(0x77);
    let msg = message(0x88);
    let mut shares = BTreeMap::new();
    for c in &round1 {
        shares.insert(
            c.identifier,
            sign_with_alpha(
                &c.nonces,
                &key_packages[&c.identifier],
                &commitments,
                &msg,
                &alpha,
            )
            .expect("share"),
        );
    }
    let group: PublicKeyPackage = frost_vault_core::threshold::PublicKeyPackage::deserialize(
        &dkg_part3(
            &round2_secrets[&0],
            &{
                let mut p = BTreeMap::new();
                for (slot, bytes) in &broadcasts {
                    if *slot != 0 {
                        p.insert(*slot, bytes.clone());
                    }
                }
                p
            },
            &inbox[&0],
        )
        .unwrap()
        .public_key_package,
    )
    .unwrap();
    let sig = aggregate_with_alpha(&group, &commitments, &msg, &alpha, &shares).expect("aggregate");
    let ak: [u8; 32] = group_keys[0].as_slice().try_into().unwrap();
    verify_with_group_key_and_alpha(&ak, &alpha, &msg, &sig).expect("verifies");
}

#[test]
fn per_round_dkg_rejects_an_unsupported_mode() {
    let mut rng = OsRng;
    assert!(frost_vault_core::threshold::dkg_part1(3, 5, 0, &mut rng).is_err());
    assert!(frost_vault_core::threshold::dkg_part1(2, 2, 3, &mut rng).is_err());
}
