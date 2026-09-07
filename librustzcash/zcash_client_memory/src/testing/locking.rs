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

#[test]
fn single_note_lock_preference_at_exact_coverage() {
    use zcash_client_backend::{
        data_api::{
            Account as _, InputSource, OutputLockStore, WalletRead,
            testing::pool::dsl::TestDsl,
            wallet::{
                ConfirmationsPolicy, TargetHeight,
                input_selection::{LockFilter, LockedInputPolicy, NonEmptyBTreeSet},
            },
        },
        wallet::LockOwner,
    };
    use zcash_protocol::{ShieldedPool, consensus::BlockHeight, value::Zatoshis};

    for lock_older in [true, false] {
        let mut st =
            TestDsl::with_sapling_birthday_account(TestMemDbFactory::new(), MemBlockCache::new())
                .build::<SaplingPoolTester>();
        let older = Zatoshis::const_from_u64(50_000);
        let newer = Zatoshis::const_from_u64(40_000);
        st.add_notes_checking_balance([[older], [newer]]);
        let account = st.test_account().unwrap().id();
        let owner = LockOwner::new([0xA2; 32]);
        let locked_value = if lock_older { older } else { newer };
        let unlocked_value = if lock_older { newer } else { older };
        let output = st.note_ref_by_value(locked_value);
        st.wallet_mut()
            .lock_outputs(&[output], owner, BlockHeight::from(u32::MAX))
            .unwrap();
        let target_height = TargetHeight::from(st.wallet().chain_height().unwrap().unwrap() + 1);

        for target in [30_000, 40_000] {
            for (policy, expected) in [
                (
                    LockedInputPolicy::PreferUnlocked(NonEmptyBTreeSet::singleton(owner)),
                    unlocked_value,
                ),
                (
                    LockedInputPolicy::PreferLocked(NonEmptyBTreeSet::singleton(owner)),
                    locked_value,
                ),
                (LockedInputPolicy::Exclude, unlocked_value),
            ] {
                let selected = st
                    .wallet()
                    .select_single_spendable_note(
                        account,
                        Zatoshis::const_from_u64(target),
                        &[ShieldedPool::Sapling],
                        target_height,
                        ConfirmationsPolicy::MIN,
                        &[],
                        LockFilter::Policy(&policy),
                    )
                    .unwrap();
                assert_eq!(
                    selected.total_value().unwrap(),
                    expected,
                    "lock_older={lock_older}, target={target}, policy={policy:?}"
                );
                assert_eq!(selected.sapling().len(), 1);
            }
        }
    }
}
