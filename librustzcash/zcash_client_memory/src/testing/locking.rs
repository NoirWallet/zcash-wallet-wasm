use crate::{MemBlockCache, testing::TestMemDbFactory};
use zcash_client_backend::data_api::testing::{pool::locking, sapling::SaplingPoolTester};

#[test]
fn spend_fails_on_locked_notes() {
    locking::spend_fails_on_locked_notes::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn explicit_note_locking() {
    locking::explicit_note_locking::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn note_locking_height_boundary() {
    locking::note_locking_height_boundary::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn clear_locked_outputs() {
    locking::clear_locked_outputs::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn proposal_level_note_locking() {
    locking::proposal_level_note_locking::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn locked_proposal_proto_roundtrip() {
    locking::locked_proposal_proto_roundtrip::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn lock_expiry_restores_spendability() {
    locking::lock_expiry_restores_spendability::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn lock_conflict_and_batch_atomicity() {
    locking::lock_conflict_and_batch_atomicity::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn unlock_proposal_inputs_releases_locks() {
    locking::unlock_proposal_inputs_releases_locks::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn spend_policy_locked_input_policy_reaches_selection() {
    locking::spend_policy_locked_input_policy_reaches_selection::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn single_note_selection_honors_lock_tier_preference() {
    locking::single_note_selection_honors_lock_tier_preference::<SaplingPoolTester>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn transparent_note_locking() {
    locking::transparent_note_locking(TestMemDbFactory::new());
}
