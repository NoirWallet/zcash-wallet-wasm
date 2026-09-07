//! Synthetic signatures and placeholder proof bytes for orchestration tests only.
use zcash_protocol::consensus::{NetworkUpgrade, Parameters};
use zcash_voting::{
    backend::{orchard, pczt, zcash_keys},
    delegate,
    delegation_submission::CanonicalDelegation,
    storage::VotingDb,
};

use zcash_voting::{Network, NoteInfo, VotingRoundParams};

pub fn height() -> u64 {
    u64::from(u32::from(
        Network::Testnet
            .activation_height(NetworkUpgrade::Nu6_3)
            .unwrap(),
    ))
}

pub fn prepare(db: &VotingDb, params: &VotingRoundParams, seed: u8) -> CanonicalDelegation {
    let usk = zcash_keys::keys::UnifiedSpendingKey::from_seed(
        &Network::Testnet,
        &[seed; 32],
        zip32::AccountId::ZERO,
    )
    .unwrap();
    let fingerprint = zip32::fingerprint::SeedFingerprint::from_seed(&[seed; 32]).unwrap();
    let ufvk = usk.to_unified_full_viewing_key();
    let fvk = ufvk.orchard().unwrap();
    let rho = orchard::note::Rho::from_bytes(&[1; 32]).unwrap();
    let rseed = (0..=255)
        .find_map(|b| orchard::note::RandomSeed::from_bytes([b; 32], &rho).into_option())
        .unwrap();
    let note = orchard::Note::from_parts(
        fvk.address_at(0u32, zip32::Scope::External),
        orchard::value::NoteValue::from_raw(100_000_000),
        rho,
        rseed,
        orchard::NoteVersion::V3,
    )
    .unwrap();
    let note =
        NoteInfo::from_orchard_note(&note, 0, zip32::Scope::External, &ufvk, &Network::Testnet)
            .unwrap();
    let id = &params.vote_round_id;
    db.ensure_bundles(id, std::slice::from_ref(&note)).unwrap();
    let hotkey = db.ensure_voting_hotkey(id).unwrap();
    let keys = delegate::DelegationKeys::with_voting_hotkey(
        fvk.to_bytes().to_vec(),
        &hotkey,
        fingerprint.to_bytes(),
        0,
        "fixture".into(),
    )
    .unwrap();
    let setup = db
        .build_governance_pczt(
            id,
            0,
            &[note],
            &keys,
            delegate::branch_id_for_height(Network::Testnet, params.snapshot_height).unwrap(),
        )
        .unwrap();
    let mut signer =
        pczt::roles::signer::Signer::new(pczt::Pczt::parse(&setup.pczt_bytes).unwrap()).unwrap();
    signer
        .sign_ironwood(
            setup.action_index,
            &orchard::keys::SpendAuthorizingKey::from(usk.orchard()),
        )
        .unwrap();
    let signature =
        delegate::spend_auth_signature(&signer.finish().serialize().unwrap(), setup.action_index)
            .unwrap();
    db.store_delegation_proof(
        id,
        0,
        &zcash_voting::DelegationProofResult {
            proof: vec![1],
            public_inputs: vec![],
            nf_signed: setup.nf_signed,
            cmx_new: setup.cmx_new,
            gov_nullifiers: setup.gov_nullifiers,
            van_comm: setup.van,
            rk: setup.rk,
        },
    )
    .unwrap();
    let submission = delegate::submission(
        db,
        id,
        0,
        delegate::DelegationSigner::signature(signature, setup.pczt_sighash.try_into().unwrap()),
    )
    .unwrap();
    CanonicalDelegation::from_submission(&submission).unwrap()
}
