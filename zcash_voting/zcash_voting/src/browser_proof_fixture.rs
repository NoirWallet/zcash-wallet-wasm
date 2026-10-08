//! Synthetic owned-job recipe, compiled only with `test-fixtures`.
use crate as fixture_sdk;
#[path = "../tests/support/owned_proof_fixture.rs"]
mod owned_proof_fixture;

pub async fn prove_owned_fixture<E: crate::proof_executor::ProofExecutor>(executor: E) -> bool {
    use owned_proof_fixture::{prepare_delegation, prove_vote_after_database_drop, HEIGHT};
    use vote_commitment_tree::MemoryTreeServer;

    let context = prepare_delegation(&executor).await;
    let mut tree = MemoryTreeServer::empty();
    tree.append(context.van_commitment).unwrap();
    tree.checkpoint(HEIGHT as u32).unwrap();
    let path = tree.path(0, HEIGHT as u32).unwrap();
    assert!(path.verify(context.van_commitment, tree.root()));
    let van_witness = crate::vote::VanWitness {
        auth_path: path
            .auth_path()
            .iter()
            .map(|hash| hash.to_bytes().to_vec())
            .collect(),
        position: 0,
        anchor_height: HEIGHT as u32,
    };
    prove_vote_after_database_drop(context, &executor, van_witness).await;
    true
}
