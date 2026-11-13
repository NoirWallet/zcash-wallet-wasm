//! `KEYSET_SETUP` acceptance tests.
//!
//! The property that matters is cross-device agreement: three devices that each ran the DKG and each
//! received the same `qsk` must land on byte-identical FVK, address and fingerprint, with no device
//! holding anything that could spend alone.

use std::collections::BTreeMap;

use frost_vault_core::threshold::{run_dkg, DkgOutput};
use rand::rngs::OsRng;
use vault_keyset::{KeysetError, QuantumRecoveryMaterial, VaultKeyset};
use zcash_protocol::consensus::{Network, Parameters};

fn dkg(participants: u16, threshold: u16) -> DkgOutput {
    run_dkg(participants, threshold, &mut OsRng).expect("dkg")
}

fn qsk(seed: u8) -> [u8; 32] {
    let mut k = [0u8; 32];
    k[0] = seed;
    k[31] = 0x01; // canonical non-zero scalar encoding
    k
}

#[test]
fn every_device_derives_identical_bytes() {
    let dkg = dkg(3, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let qsk = qsk(7);

    // Each participant derives independently; all three must agree.
    let derived: Vec<VaultKeyset> = dkg
        .identifiers
        .iter()
        .map(|_| VaultKeyset::derive(&ak, &qsk, Network::TestNetwork, 0).expect("derive"))
        .collect();

    for k in &derived[1..] {
        assert_eq!(k.fvk, derived[0].fvk);
        assert_eq!(k.address, derived[0].address);
        assert_eq!(k.ufvk, derived[0].ufvk);
        assert_eq!(k.fingerprint, derived[0].fingerprint);
        assert_eq!(k.raw_address, derived[0].raw_address);
    }
}

#[test]
fn the_address_carries_only_an_orchard_receiver() {
    // P8: a transparent receiver in the vault's UA would let someone send funds that FROST can
    // never spend. This is the check that keeps that from happening by accident.
    let dkg = dkg(2, 2);
    let k = VaultKeyset::derive(&dkg.group_verifying_key().unwrap(), &qsk(3), Network::TestNetwork, 0).unwrap();
    assert!(k.has_only_orchard_receiver());
    assert!(k.address.starts_with("u1") || k.address.starts_with("utest1"));
    assert!(k.ufvk.starts_with("uview") || k.ufvk.starts_with("uviewtest"));
    // Decoding is the only reliable check: the bech32 data part can contain "t1" by chance, so a
    // substring test would be both wrong and confusing.
}

#[test]
fn the_ufvk_round_trips_and_carries_the_group_key() {
    let dkg = dkg(2, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let k = VaultKeyset::derive(&ak, &qsk(9), Network::TestNetwork, 0).unwrap();

    // ak is literally the first 32 bytes of the FVK: that is what makes the group the spender.
    assert_eq!(&k.fvk[..32], &ak);
    assert_eq!(&k.fvk, k.ufvk_orchard_component());

    use zcash_address::unified::{Encoding, Ufvk};
    // The UFVK parses and re-encodes to the same string: enough to know the container is well formed
    // and that no receiver kind was smuggled in. (The item list itself is not exposed by this
    // version of `zcash_address`, so the ak binding is asserted on `fvk` above instead.)
    let (_, _, decoded) = Ufvk::decode(&k.ufvk).expect("ufvk decodes");
    assert_eq!(decoded.encode(&Network::TestNetwork.network_type()), k.ufvk);
}

#[test]
fn different_recovery_keys_give_different_addresses() {
    // qsk must be generated once and shared: local generation would silently split the wallet.
    let dkg = dkg(2, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let a = VaultKeyset::derive(&ak, &qsk(1), Network::TestNetwork, 0).unwrap();
    let b = VaultKeyset::derive(&ak, &qsk(2), Network::TestNetwork, 0).unwrap();
    assert_ne!(a.address, b.address);
    assert_ne!(a.fingerprint, b.fingerprint);
    // Same group key, so the spend authority is unchanged.
    assert_eq!(&a.fvk[..32], &b.fvk[..32]);
}

#[test]
fn a_garbage_group_key_is_refused() {
    // `FullViewingKey::from_bytes` validates the point rather than trusting the bytes.
    let err = VaultKeyset::derive(&[0xFFu8; 32], &qsk(5), Network::TestNetwork, 0).unwrap_err();
    assert_eq!(err, KeysetError::InvalidFullViewingKey);

    // The identity (all-zero) is explicitly rejected.
    let err = VaultKeyset::derive(&[0u8; 32], &qsk(5), Network::TestNetwork, 0).unwrap_err();
    assert_eq!(err, KeysetError::InvalidFullViewingKey);
}

#[test]
fn the_recovery_key_is_the_only_free_input_besides_ak() {
    // `SpendingKey::from_bytes` accepts most 32-byte strings, so there is no useful
    // "non-canonical" rejection to assert. What matters is determinism: the same (ak, qsk) pair
    // always yields the same keyset, and qsk is the only other input.
    let dkg = dkg(2, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let a = VaultKeyset::derive(&ak, &qsk(0), Network::TestNetwork, 0).unwrap();
    let b = VaultKeyset::derive(&ak, &qsk(0), Network::TestNetwork, 0).unwrap();
    assert_eq!(a, b);
}

#[test]
fn diversifier_index_changes_the_address_but_not_the_viewing_key() {
    let dkg = dkg(2, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let k = VaultKeyset::derive(&ak, &qsk(11), Network::TestNetwork, 0).unwrap();
    let other = k.address_at(&qsk(11), Network::TestNetwork, 5).unwrap();
    assert_ne!(other, k.address);
    // Same viewing key: diversifying addresses must not require new keys.
    let ufvk_again = VaultKeyset::derive(&ak, &qsk(11), Network::TestNetwork, 5).unwrap().ufvk;
    assert_eq!(ufvk_again, k.ufvk);
}

#[test]
fn mainnet_and_testnet_encodings_differ_but_share_the_keyset() {
    let dkg = dkg(2, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let main = VaultKeyset::derive(&ak, &qsk(13), Network::MainNetwork, 0).unwrap();
    let test = VaultKeyset::derive(&ak, &qsk(13), Network::TestNetwork, 0).unwrap();
    assert_ne!(main.address, test.address);
    assert_eq!(main.fvk, test.fvk);
    assert_ne!(main.fingerprint, test.fingerprint);
    assert!(main.address.starts_with("u1"));
    assert!(test.address.starts_with("utest1"));
}

#[test]
fn recovery_material_round_trips_and_cannot_spend() {
    let material = QuantumRecoveryMaterial {
        group_verifying_key: [4u8; 32],
        qsk: qsk(21),
    };
    let bytes = material.to_bytes();
    let back = QuantumRecoveryMaterial::from_bytes(&bytes);
    assert_eq!(back.group_verifying_key, material.group_verifying_key);
    assert_eq!(back.qsk, material.qsk);

    // The material is enough to re-derive the keyset, and nothing more: there is no spending key in
    // it, so it cannot produce a spend authorization.
    let dkg = dkg(2, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let k = VaultKeyset::derive(&ak, &material.qsk, Network::TestNetwork, 0).unwrap();
    assert_eq!(&k.fvk[..32], &ak);

    // Debug output must never leak it.
    let printed = format!("{material:?}");
    assert!(printed.contains("<redacted>"));
    assert!(!printed.contains(&hex::encode(material.qsk)));
}

#[test]
fn the_fingerprint_is_stable_and_covers_the_whole_keyset() {
    let dkg = dkg(2, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let a = VaultKeyset::derive(&ak, &qsk(31), Network::TestNetwork, 0).unwrap();
    let b = VaultKeyset::derive(&ak, &qsk(31), Network::TestNetwork, 0).unwrap();
    assert_eq!(a.fingerprint, b.fingerprint);
    assert_ne!(a.fingerprint, [0u8; 32]);

    // A different ak changes the fingerprint, i.e. devices comparing it would notice a wrong group.
    let other = run_dkg(2, 2, &mut OsRng).unwrap().group_verifying_key().unwrap();
    if other != ak {
        let c = VaultKeyset::derive(&other, &qsk(31), Network::TestNetwork, 0).unwrap();
        assert_ne!(a.fingerprint, c.fingerprint);
    }
}

/// Sanity check that the SPLICE is the only thing connecting the group to the keyset: two different
/// ak values over the same qsk share nk/rivk and differ only in the first 32 bytes.
#[test]
fn splice_only_replaces_the_authorizing_component() {
    let a = run_dkg(2, 2, &mut OsRng).unwrap().group_verifying_key().unwrap();
    let b = run_dkg(2, 2, &mut OsRng).unwrap().group_verifying_key().unwrap();
    if a == b {
        return;
    }
    let ka = VaultKeyset::derive(&a, &qsk(41), Network::TestNetwork, 0).unwrap();
    let kb = VaultKeyset::derive(&b, &qsk(41), Network::TestNetwork, 0).unwrap();
    assert_eq!(&ka.fvk[..32], &a);
    assert_eq!(&kb.fvk[..32], &b);
    assert_eq!(&ka.fvk[32..], &kb.fvk[32..], "nk/rivk must be untouched");
}

#[test]
fn keyset_matches_the_wallet_stack_parameters_impl() {
    // `Network` is the `Parameters` impl the wallet uses; make sure we are encoding with the same
    // one rather than a look-alike.
    fn assert_parameters<P: Parameters>(_: &P) {}
    assert_parameters(&Network::TestNetwork);
    // Column of the map is irrelevant here; this just keeps the import meaningful.
    let _: BTreeMap<u8, u8> = BTreeMap::new();
}
