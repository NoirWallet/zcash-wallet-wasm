//! Test-only setup shared by native contract and opt-in browser proof wrappers.
use super::fixture_sdk;
use fixture_sdk::storage::VotingDb;
use fixture_sdk::{
    backend::{orchard, zcash_client_backend, zcash_keys, zcash_primitives},
    delegate::DelegationAccountKeys,
    store_tree_state_and_witnesses, Network, NoteInfo, VotingRoundParams, WitnessData,
};
use incrementalmerkletree::{
    frontier::{CommitmentTree, Frontier},
    Hashable, Level,
};
use orchard::{
    note::{ExtractedNoteCommitment, RandomSeed, Rho},
    tree::MerkleHashOrchard,
};
use prost::Message;
use zcash_client_backend::proto::service::TreeState;
pub(super) const HEIGHT: u64 = 100;
const OWNER: &str = "vote-proof-fixture";

pub(super) fn fixture() -> (NoteInfo, DelegationAccountKeys, TreeState, WitnessData) {
    let ufvk = zcash_keys::keys::UnifiedSpendingKey::from_seed(
        &Network::Regtest,
        &[17; 32],
        zip32::AccountId::ZERO,
    )
    .unwrap()
    .to_unified_full_viewing_key();
    let fvk = ufvk.orchard().unwrap();
    let rho = Rho::from_bytes(&[1; 32]).unwrap();
    let seed = (0..=255)
        .find_map(|b| RandomSeed::from_bytes([b; 32], &rho).into_option())
        .unwrap();
    let note = orchard::Note::from_parts(
        fvk.address_at(0u32, zip32::Scope::External),
        orchard::value::NoteValue::from_raw(fixture_sdk::governance::BALLOT_DIVISOR * 2),
        rho,
        seed,
        orchard::NoteVersion::V3,
    )
    .unwrap();
    let cmx: ExtractedNoteCommitment = note.commitment().into();
    let mut frontier = Frontier::<MerkleHashOrchard, 32>::empty();
    frontier.append(MerkleHashOrchard::from_cmx(&cmx));
    let mut tree_bytes = Vec::new();
    zcash_primitives::merkle_tree::write_commitment_tree(
        &CommitmentTree::from_frontier(&frontier),
        &mut tree_bytes,
    )
    .unwrap();
    let state = TreeState {
        network: "regtest".into(),
        height: HEIGHT,
        ironwood_tree: hex::encode(tree_bytes),
        ..TreeState::default()
    };
    let info =
        NoteInfo::from_orchard_note(&note, 0, zip32::Scope::External, &ufvk, &Network::Regtest)
            .unwrap();
    let keys = DelegationAccountKeys {
        account_index: 0,
        orchard_fvk_bytes: fvk.to_bytes(),
        seed_fingerprint: [0; 32],
    };
    let witness = WitnessData {
        position: 0,
        note_commitment: info.commitment.clone(),
        root: frontier.root().to_bytes().to_vec(),
        auth_path: (0..32)
            .map(|level| {
                MerkleHashOrchard::empty_root(Level::from(level))
                    .to_bytes()
                    .to_vec()
            })
            .collect(),
    };
    (info, keys, state, witness)
}

use fixture_sdk::{
    backend::pasta_curves::{group::ff::PrimeField, pallas::Base},
    proof_executor::ProofExecutor,
    vote::{DraftVote, VanWitness, VoteSigner},
    VotingHotkey,
};

pub(super) struct ProofContext {
    pub db: VotingDb,
    pub hotkey: VotingHotkey,
    pub id: String,
    pub van_commitment: Base,
}

pub(super) async fn prepare_delegation<E: ProofExecutor>(executor: &E) -> ProofContext {
    use voting_circuits::delegation::{ImtProvider, SpacedLeafImtProvider};
    let (note, account, tree, witness) = fixture();
    let id = "02".repeat(32);
    let imt = SpacedLeafImtProvider::new();
    let db = VotingDb::open_browser(OWNER, Network::Regtest, None).unwrap();
    let params = VotingRoundParams {
        vote_round_id: id.clone(),
        snapshot_height: HEIGHT,
        ea_pk: vec![7; 32],
        nc_root: witness.root.clone(),
        nullifier_imt_root: imt.root().to_repr().to_vec(),
    };
    db.init_round(Network::Regtest, &params, None).unwrap();
    let hotkey = db.ensure_voting_hotkey(&id).unwrap();
    let keys = fixture_sdk::delegate::DelegationKeys::with_voting_hotkey(
        account.orchard_fvk_bytes.to_vec(),
        &hotkey,
        account.seed_fingerprint,
        account.account_index,
        OWNER.into(),
    )
    .unwrap();
    let notes = vec![note];
    db.ensure_bundles(&id, &notes).unwrap();
    db.ensure_padded_secrets(&id, 0, &notes).unwrap();
    store_tree_state_and_witnesses(&db, &id, 0, &tree.encode_to_vec(), &notes, &[witness]).unwrap();
    db.precompute_delegation_pir_async(&id, 0, &notes, Network::Regtest, |nullifiers| async {
        Ok(nullifiers
            .into_iter()
            .map(|nf| {
                let proof = imt.non_membership_proof(nf).unwrap();
                fixture_sdk::pir::ImtProofData {
                    root: proof.root,
                    nf_bounds: proof.nf_bounds,
                    leaf_pos: proof.leaf_pos,
                    path: proof.path,
                }
            })
            .collect())
    })
    .await
    .unwrap();
    db.build_governance_pczt(
        &id,
        0,
        &notes,
        &keys,
        fixture_sdk::delegate::branch_id_for_height(Network::Regtest, HEIGHT).unwrap(),
    )
    .unwrap();
    let delegation = db.delegation_proof_job(&id, 0, notes, keys).unwrap();
    let delegation = executor
        .prove(delegation, |job| {
            job(&fixture_sdk::types::NoopProgressReporter)
        })
        .await
        .unwrap();
    db.store_delegation_proof(&id, 0, &delegation).unwrap();
    db.store_van_position(&id, 0, 0).unwrap();

    let van_commitment = Base::from_repr(delegation.van_comm.try_into().unwrap())
        .into_option()
        .unwrap();

    ProofContext {
        db,
        hotkey,
        id,
        van_commitment,
    }
}

pub(super) async fn prove_vote_after_database_drop<E: ProofExecutor>(
    context: ProofContext,
    executor: &E,
    van_witness: VanWitness,
) {
    let ProofContext { db, hotkey, id, .. } = context;
    let draft = DraftVote {
        proposal_id: 1,
        choice: 0,
        num_options: 2,
        single_share: false,
        vc_tree_position: 0,
    };
    db.set_ballot_intent(&id, 1, fixture_sdk::session::Decision::Choice(0), 2)
        .unwrap();
    let job = db
        .vote_proof_job(VoteSigner::hotkey(&hotkey), &id, 0, &draft, &van_witness)
        .unwrap();
    let bytes = db.serialize_bytes().unwrap();
    drop(db);

    let prepared = executor
        .prove(job, |job| {
            job.prove(&fixture_sdk::types::NoopProgressReporter)
        })
        .await
        .unwrap();
    let restored = VotingDb::open_browser(OWNER, Network::Regtest, Some(&bytes)).unwrap();
    fixture_sdk::vote::persist_prepared_commit(&restored, prepared).unwrap();
    assert_eq!(
        restored.vote_phase(&id, 0, 1).unwrap(),
        fixture_sdk::phases::VotePhase::Committed
    );
}
