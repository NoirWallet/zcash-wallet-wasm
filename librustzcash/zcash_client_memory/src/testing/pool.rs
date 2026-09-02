use zcash_client_backend::data_api::testing::pool::ShieldedPoolTester;

use crate::testing::{MemBlockCache, TestMemDbFactory};

#[cfg(test)]
mod sapling;

#[cfg(test)]
#[cfg(feature = "orchard")]
mod orchard;

pub(crate) fn send_single_step_proposed_transfer<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::send_single_step_proposed_transfer::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

#[cfg(feature = "orchard")]
pub(crate) fn orchard_send_max_routes_to_ironwood() {
    use std::convert::Infallible;

    use zcash_client_backend::data_api::{
        Account as _, MaxSpendMode,
        testing::{TestBuilder, pool::dsl::TestDsl},
        wallet::{ConfirmationsPolicy, propose_send_max_transfer},
    };
    use zcash_client_backend::fees::StandardFeeRule;
    use zcash_primitives::{block::BlockHash, transaction::TxVersion};
    use zcash_protocol::{
        PoolType, ShieldedPool, consensus::BlockHeight, local_consensus::LocalNetwork,
        value::Zatoshis,
    };

    use zcash_client_backend::data_api::testing::orchard::OrchardPoolTester;

    let activation = BlockHeight::from_u32(100_000);
    let network = LocalNetwork {
        nu6: Some(activation),
        nu6_1: Some(activation),
        nu6_2: Some(activation),
        nu6_3: Some(activation),
        ..TestBuilder::<(), ()>::DEFAULT_NETWORK
    };
    let mut st = TestDsl::from(
        TestBuilder::new()
            .with_network(network)
            .with_data_store_factory(TestMemDbFactory::new())
            .with_block_cache(MemBlockCache::new())
            .with_account_from_sapling_activation(BlockHash([0; 32])),
    )
    .build::<OrchardPoolTester>();

    st.add_a_single_note_checking_balance(Zatoshis::const_from_u64(60_000));
    let account = st.get_account();
    let to_extsk = OrchardPoolTester::sk(&[0xf5; 32]);
    let to = OrchardPoolTester::sk_default_address(&to_extsk).to_zcash_address(st.network());
    let network = *st.network();

    let proposal = propose_send_max_transfer::<_, _, _, Infallible>(
        st.wallet_mut(),
        &network,
        account.id(),
        &[ShieldedPool::Orchard],
        &StandardFeeRule::Zip317,
        to,
        None,
        MaxSpendMode::MaxSpendable,
        ConfirmationsPolicy::MIN,
    )
    .expect("Orchard migration send-max proposal should succeed")
    .with_proposed_version(Some(TxVersion::V6));

    assert_eq!(proposal.proposed_version(), Some(TxVersion::V6));
    assert_eq!(
        proposal.steps().head.payment_pools().get(&0),
        Some(&PoolType::IRONWOOD),
    );
    assert_eq!(
        proposal.steps().head.input_count_in_pool(PoolType::ORCHARD),
        1,
    );
    assert_eq!(
        proposal
            .steps()
            .head
            .input_count_in_pool(PoolType::IRONWOOD),
        0,
    );
}

#[cfg(all(feature = "orchard", feature = "transparent-inputs"))]
pub(crate) fn shielded_send_max_routes_to_transparent() {
    use std::convert::Infallible;

    use zcash_client_backend::data_api::{
        Account as _, MaxSpendMode,
        testing::{TestBuilder, pool::dsl::TestDsl},
        wallet::{ConfirmationsPolicy, propose_send_max_transfer},
    };
    use zcash_client_backend::fees::StandardFeeRule;
    use zcash_keys::address::Address;
    use zcash_primitives::block::BlockHash;
    use zcash_protocol::{
        PoolType, ShieldedPool, consensus::BlockHeight, local_consensus::LocalNetwork,
        value::Zatoshis,
    };

    use zcash_client_backend::data_api::testing::orchard::OrchardPoolTester;

    let activation = BlockHeight::from_u32(100_000);
    let network = LocalNetwork {
        nu6: Some(activation),
        nu6_1: Some(activation),
        nu6_2: Some(activation),
        nu6_3: Some(activation),
        ..TestBuilder::<(), ()>::DEFAULT_NETWORK
    };
    let mut st = TestDsl::from(
        TestBuilder::new()
            .with_network(network)
            .with_data_store_factory(TestMemDbFactory::new())
            .with_block_cache(MemBlockCache::new())
            .with_account_from_sapling_activation(BlockHash([0; 32])),
    )
    .build::<OrchardPoolTester>();

    st.add_a_single_note_checking_balance(Zatoshis::const_from_u64(60_000));
    let account = st.get_account();
    let (to, _) = account.usk().default_transparent_address();
    let to = Address::Transparent(to).to_zcash_address(st.network());
    let network = *st.network();

    let proposal = propose_send_max_transfer::<_, _, _, Infallible>(
        st.wallet_mut(),
        &network,
        account.id(),
        &[
            ShieldedPool::Sapling,
            ShieldedPool::Orchard,
            ShieldedPool::Ironwood,
        ],
        &StandardFeeRule::Zip317,
        to,
        None,
        MaxSpendMode::MaxSpendable,
        ConfirmationsPolicy::MIN,
    )
    .expect("shielded send-max proposal to a transparent recipient should succeed");

    assert_eq!(
        proposal.steps().head.payment_pools().get(&0),
        Some(&PoolType::TRANSPARENT),
    );
    assert_eq!(
        proposal
            .steps()
            .head
            .transaction_request()
            .payments()
            .get(&0)
            .and_then(|payment| payment.amount()),
        Some(Zatoshis::const_from_u64(45_000)),
    );
    assert_eq!(
        proposal.steps().head.balance().fee_required(),
        Zatoshis::const_from_u64(15_000),
    );
}

#[cfg(all(feature = "orchard", feature = "transparent-inputs"))]
pub(crate) fn send_max_rejects_fee_only_transfer() {
    use std::convert::Infallible;

    use zcash_client_backend::data_api::{
        Account as _, MaxSpendMode,
        testing::{TestBuilder, pool::dsl::TestDsl},
        wallet::{ConfirmationsPolicy, propose_send_max_transfer},
    };
    use zcash_client_backend::fees::StandardFeeRule;
    use zcash_keys::address::Address;
    use zcash_primitives::{block::BlockHash, transaction::fees::zip317::MINIMUM_FEE};
    use zcash_protocol::{ShieldedPool, consensus::BlockHeight, local_consensus::LocalNetwork};

    use zcash_client_backend::data_api::testing::orchard::OrchardPoolTester;

    let activation = BlockHeight::from_u32(100_000);
    let network = LocalNetwork {
        nu6: Some(activation),
        nu6_1: Some(activation),
        nu6_2: Some(activation),
        nu6_3: Some(activation),
        ..TestBuilder::<(), ()>::DEFAULT_NETWORK
    };
    let mut st = TestDsl::from(
        TestBuilder::new()
            .with_network(network)
            .with_data_store_factory(TestMemDbFactory::new())
            .with_block_cache(MemBlockCache::new())
            .with_account_from_sapling_activation(BlockHash([0; 32])),
    )
    .build::<OrchardPoolTester>();

    st.add_a_single_note_checking_balance(MINIMUM_FEE);
    let account = st.get_account();
    let (to, _) = account.usk().default_transparent_address();
    let to = Address::Transparent(to).to_zcash_address(st.network());
    let network = *st.network();

    let result = propose_send_max_transfer::<_, _, _, Infallible>(
        st.wallet_mut(),
        &network,
        account.id(),
        &[
            ShieldedPool::Sapling,
            ShieldedPool::Orchard,
            ShieldedPool::Ironwood,
        ],
        &StandardFeeRule::Zip317,
        to,
        None,
        MaxSpendMode::MaxSpendable,
        ConfirmationsPolicy::MIN,
    );

    assert!(matches!(
        result,
        Err(zcash_client_backend::data_api::error::Error::InsufficientFunds {
            available,
            required,
        }) if available == MINIMUM_FEE && required > available
    ));
}

#[cfg(feature = "orchard")]
pub(crate) fn dust_note_is_reported_as_uneconomic() {
    use zcash_client_backend::data_api::{
        Account as _, WalletRead,
        testing::{TestBuilder, pool::dsl::TestDsl},
        wallet::ConfirmationsPolicy,
    };
    use zcash_primitives::{block::BlockHash, transaction::fees::zip317::MARGINAL_FEE};
    use zcash_protocol::value::Zatoshis;

    use zcash_client_backend::data_api::testing::orchard::OrchardPoolTester;

    let mut st = TestDsl::from(
        TestBuilder::new()
            .with_data_store_factory(TestMemDbFactory::new())
            .with_block_cache(MemBlockCache::new())
            .with_account_from_sapling_activation(BlockHash([0; 32])),
    )
    .build::<OrchardPoolTester>();

    st.add_a_single_note_checking_balance(MARGINAL_FEE);
    let account_id = st.get_account().id();
    let summary = st
        .wallet()
        .get_wallet_summary(ConfirmationsPolicy::MIN)
        .expect("summary query succeeds")
        .expect("summary exists");
    let balance = summary
        .account_balances()
        .get(&account_id)
        .expect("account balance exists");

    assert_eq!(balance.spendable_value(), Zatoshis::ZERO);
    assert_eq!(balance.value_pending_spendability(), Zatoshis::ZERO);
    assert_eq!(balance.uneconomic_value(), MARGINAL_FEE);

    let diagnostics = st
        .wallet()
        .spendability_diagnostics(account_id, ConfirmationsPolicy::MIN)
        .expect("diagnostic query succeeds");
    assert_eq!(diagnostics.incomplete_note_count(), 0);
    assert_eq!(diagnostics.incomplete_value(), Zatoshis::ZERO);
}

#[cfg(feature = "orchard")]
pub(crate) fn incomplete_note_metadata_is_not_reported_as_spendable() {
    use zcash_client_backend::data_api::{
        Account as _, InputSource, WalletRead,
        testing::{TestBuilder, pool::dsl::TestDsl},
        wallet::{ConfirmationsPolicy, TargetHeight},
    };
    use zcash_primitives::block::BlockHash;
    use zcash_protocol::{ShieldedPool, local_consensus::LocalNetwork, value::Zatoshis};

    use zcash_client_backend::data_api::testing::orchard::OrchardPoolTester;

    let mut st = TestDsl::from(
        TestBuilder::new()
            .with_data_store_factory(TestMemDbFactory::new())
            .with_block_cache(MemBlockCache::new())
            .with_account_from_sapling_activation(BlockHash([0; 32])),
    )
    .build::<OrchardPoolTester>();

    let note_value = Zatoshis::const_from_u64(60_000);
    st.add_a_single_note_checking_balance(note_value);
    let account_id = st.get_account().id();
    let target_height = TargetHeight::from(
        st.wallet()
            .chain_height()
            .expect("chain height query succeeds")
            .expect("test chain has a tip")
            + 1,
    );

    let note = st
        .wallet_mut()
        .received_notes
        .0
        .first_mut()
        .expect("test note exists");
    note.nf = None;
    note.commitment_tree_position = None;

    let summary = st
        .wallet()
        .get_wallet_summary(ConfirmationsPolicy::MIN)
        .expect("summary query succeeds")
        .expect("summary exists");
    let balance = summary
        .account_balances()
        .get(&account_id)
        .expect("account balance exists");
    assert_eq!(balance.spendable_value(), Zatoshis::ZERO);
    assert_eq!(balance.value_pending_spendability(), note_value);

    let selected = st
        .wallet()
        .select_spendable_notes(
            account_id,
            zcash_client_backend::data_api::TargetValue::AllFunds(
                zcash_client_backend::data_api::MaxSpendMode::MaxSpendable,
            ),
            &[ShieldedPool::Orchard],
            target_height,
            ConfirmationsPolicy::MIN,
            &[],
        )
        .expect("note selection succeeds");
    assert!(selected.orchard().is_empty());

    let diagnostics = st
        .wallet()
        .spendability_diagnostics(account_id, ConfirmationsPolicy::MIN)
        .expect("diagnostic query succeeds");
    assert_eq!(diagnostics.incomplete_note_count(), 1);
    assert_eq!(diagnostics.incomplete_value(), note_value);

    let mut encoded = Vec::new();
    st.wallet()
        .encode(&mut encoded)
        .expect("wallet serialization succeeds");
    let restored =
        crate::MemoryWalletDb::<LocalNetwork>::decode_new(encoded.as_slice(), *st.network(), 100)
            .expect("wallet deserialization succeeds");
    assert_eq!(
        restored
            .spendability_diagnostics(account_id, ConfirmationsPolicy::MIN)
            .expect("restored diagnostic query succeeds"),
        diagnostics,
    );
}

#[cfg(feature = "transparent-inputs")]
pub(crate) fn send_multi_step_proposed_transfer<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::send_multi_step_proposed_transfer::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
        |e, account_id, expected_bad_index| {
            matches!(
                e,
                crate::Error::ReachedGapLimit(acct, bad_index)
                if acct == &account_id && bad_index == &expected_bad_index)
        },
    )
}

#[cfg(feature = "transparent-inputs")]
pub(crate) fn proposal_fails_if_not_all_ephemeral_outputs_consumed<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::proposal_fails_if_not_all_ephemeral_outputs_consumed::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

#[allow(deprecated)]
pub(crate) fn create_to_address_fails_on_incorrect_usk<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::create_to_address_fails_on_incorrect_usk::<T, _>(
        TestMemDbFactory::new(),
    )
}

#[allow(deprecated)]
pub(crate) fn proposal_fails_with_no_blocks<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::proposal_fails_with_no_blocks::<T, _>(
        TestMemDbFactory::new(),
    )
}

pub(crate) fn spend_fails_on_unverified_notes<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::spend_fails_on_unverified_notes::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn spend_fails_on_locked_notes<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::spend_fails_on_locked_notes::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn ovk_policy_prevents_recovery_from_chain<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::ovk_policy_prevents_recovery_from_chain::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn spend_succeeds_to_t_addr_zero_change<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::spend_succeeds_to_t_addr_zero_change::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn change_note_spends_succeed<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::change_note_spends_succeed::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

// TODO: Implement reset for memdb
pub(crate) fn external_address_change_spends_detected_in_restore_from_seed<
    T: ShieldedPoolTester,
>() {
    zcash_client_backend::data_api::testing::pool::external_address_change_spends_detected_in_restore_from_seed::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

#[allow(dead_code)]
pub(crate) fn zip317_spend<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::zip317_spend::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

#[cfg(feature = "transparent-inputs")]
pub(crate) fn shield_transparent<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::shield_transparent::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn birthday_in_anchor_shard<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::birthday_in_anchor_shard::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn checkpoint_gaps<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::checkpoint_gaps::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

#[cfg(feature = "orchard")]
pub(crate) fn pool_crossing_required<T: ShieldedPoolTester, TT: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::pool_crossing_required::<T, TT>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

#[cfg(feature = "orchard")]
pub(crate) fn fully_funded_fully_private<T: ShieldedPoolTester, TT: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::fully_funded_fully_private::<T, TT>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

#[cfg(all(feature = "orchard", feature = "transparent-inputs"))]
pub(crate) fn fully_funded_send_to_t<T: ShieldedPoolTester, TT: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::fully_funded_send_to_t::<T, TT>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

#[cfg(feature = "orchard")]
pub(crate) fn multi_pool_checkpoint<T: ShieldedPoolTester, TT: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::multi_pool_checkpoint::<T, TT>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

#[cfg(feature = "orchard")]
pub(crate) fn multi_pool_checkpoints_with_pruning<T: ShieldedPoolTester, TT: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::multi_pool_checkpoints_with_pruning::<T, TT>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn valid_chain_states<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::valid_chain_states::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn invalid_chain_cache_disconnected<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::invalid_chain_cache_disconnected::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn data_db_truncation<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::data_db_truncation::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn scan_cached_blocks_allows_blocks_out_of_order<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::scan_cached_blocks_allows_blocks_out_of_order::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn scan_cached_blocks_finds_received_notes<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::scan_cached_blocks_finds_received_notes::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn scan_cached_blocks_finds_change_notes<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::scan_cached_blocks_finds_change_notes::<T, _>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}

pub(crate) fn scan_cached_blocks_detects_spends_out_of_order<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::scan_cached_blocks_detects_spends_out_of_order::<
        T,
        _,
    >(TestMemDbFactory::new(), MemBlockCache::new())
}

pub(crate) fn receive_two_notes_with_same_value<T: ShieldedPoolTester>() {
    zcash_client_backend::data_api::testing::pool::receive_two_notes_with_same_value::<T>(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    )
}
