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
    prepare_bundles(db, params, seed, 1).remove(0)
}

pub fn prepare_bundles(
    db: &VotingDb,
    params: &VotingRoundParams,
    seed: u8,
    count: u8,
) -> Vec<CanonicalDelegation> {
    let usk = zcash_keys::keys::UnifiedSpendingKey::from_seed(
        &Network::Testnet,
        &[seed; 32],
        zip32::AccountId::ZERO,
    )
    .unwrap();
    let fingerprint = zip32::fingerprint::SeedFingerprint::from_seed(&[seed; 32]).unwrap();
    let ufvk = usk.to_unified_full_viewing_key();
    let fvk = ufvk.orchard().unwrap();
    let notes = (0..count)
        .map(|index| {
            let rho = orchard::note::Rho::from_bytes(&[index + 1; 32]).unwrap();
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
            NoteInfo::from_orchard_note(
                &note,
                u64::from(index),
                zip32::Scope::External,
                &ufvk,
                &Network::Testnet,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let id = &params.vote_round_id;
    db.ensure_bundles_with_policy(id, &notes, zcash_voting::BundlePolicy::new(1).unwrap())
        .unwrap();
    let hotkey = db.ensure_voting_hotkey(id).unwrap();
    let keys = delegate::DelegationKeys::with_voting_hotkey(
        fvk.to_bytes().to_vec(),
        &hotkey,
        fingerprint.to_bytes(),
        0,
        "fixture".into(),
    )
    .unwrap();
    notes
        .into_iter()
        .enumerate()
        .map(|(index, note)| {
            let index = index as u32;
            let setup = db
                .build_governance_pczt(
                    id,
                    index,
                    &[note],
                    &keys,
                    delegate::branch_id_for_height(Network::Testnet, params.snapshot_height)
                        .unwrap(),
                )
                .unwrap();
            let mut signer =
                pczt::roles::signer::Signer::new(pczt::Pczt::parse(&setup.pczt_bytes).unwrap())
                    .unwrap();
            signer
                .sign_ironwood(
                    setup.action_index,
                    &orchard::keys::SpendAuthorizingKey::from(usk.orchard()),
                )
                .unwrap();
            let signature = delegate::spend_auth_signature(
                &signer.finish().serialize().unwrap(),
                setup.action_index,
            )
            .unwrap();
            db.store_delegation_proof(
                id,
                index,
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
                index,
                delegate::DelegationSigner::signature(
                    signature,
                    setup.pczt_sighash.try_into().unwrap(),
                ),
            )
            .unwrap();
            CanonicalDelegation::from_submission(&submission).unwrap()
        })
        .collect()
}

pub fn event(round: &str, position: &str) -> zcash_voting::confirmation::TxEvent {
    use zcash_voting::confirmation::{TxEvent, TxEventAttribute};
    TxEvent {
        event_type: "delegate_vote".into(),
        attributes: vec![
            TxEventAttribute {
                key: "vote_round_id".into(),
                value: round.into(),
            },
            TxEventAttribute {
                key: "leaf_index".into(),
                value: position.into(),
            },
        ],
    }
}

pub fn corrupt(bytes: &[u8], sql: &str) -> Vec<u8> {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.deserialize_read_exact(
        rusqlite::MAIN_DB,
        std::io::Cursor::new(bytes),
        bytes.len(),
        false,
    )
    .unwrap();
    conn.execute_batch(sql).unwrap();
    conn.serialize(rusqlite::MAIN_DB).unwrap().to_vec()
}

pub fn confirmed_corruptions() -> Vec<&'static str> {
    vec![
        "UPDATE proofs SET proof=x'02' WHERE bundle_index=1",
        "DELETE FROM proofs WHERE bundle_index=1",
        "UPDATE bundles SET pczt_sighash=zeroblob(32) WHERE bundle_index=1",
        "UPDATE delegation_attempts SET canonical_json=x'00' WHERE bundle_index=1",
        "UPDATE delegation_attempts SET tx_hash='wrong' WHERE bundle_index=1",
        "UPDATE delegation_attempts SET outcome='rejected' WHERE bundle_index=1",
        "UPDATE delegation_attempts SET outcome='prepared' WHERE bundle_index=1",
        "UPDATE delegation_attempts SET outcome='ambiguous' WHERE bundle_index=1",
        "UPDATE delegation_attempts SET outcome='dispatching' WHERE bundle_index=1",
        "DELETE FROM delegation_attempts WHERE bundle_index=1",
        "PRAGMA foreign_keys=OFF; DELETE FROM bundles WHERE bundle_index=1",
        "UPDATE bundles SET delegation_tx_hash=NULL WHERE bundle_index=1",
        "UPDATE bundles SET delegation_tx_hash='wrong' WHERE bundle_index=1",
        "UPDATE bundles SET van_leaf_position=43 WHERE bundle_index=1",
        "UPDATE bundles SET van_leaf_position=NULL WHERE bundle_index=1",
        "UPDATE delegation_attempts SET confirmation_tx_hash='wrong' WHERE bundle_index=1",
        "UPDATE delegation_attempts SET confirmation_van_position=43 WHERE bundle_index=1",
        "UPDATE delegation_attempts SET confirmation_tx_hash=NULL,confirmation_van_position=NULL WHERE bundle_index=1",
    ]
}
