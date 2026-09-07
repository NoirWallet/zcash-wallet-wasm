use transparent::{
    bundle::{OutPoint, TxOut},
    keys::TransparentKeyScope,
};
use zcash_client_backend::{
    data_api::{
        Account as _, WalletRead as _, WalletWrite as _, testing::TestBuilder,
        wallet::ConfirmationsPolicy,
    },
    wallet::WalletTransparentOutput,
};
use zcash_keys::keys::UnifiedAddressRequest;
use zcash_primitives::{block::BlockHash, transaction::TxId};
use zcash_protocol::value::Zatoshis;

use crate::testing::{MemBlockCache, TestMemDbFactory};

#[test]
fn put_received_transparent_utxo() {
    zcash_client_backend::data_api::testing::transparent::put_received_transparent_utxo(
        TestMemDbFactory::new(),
    );
}

#[test]
fn transparent_balance_tracks_pending_confirmations() {
    let mut st = TestBuilder::new()
        .with_data_store_factory(TestMemDbFactory::new())
        .with_account_from_sapling_activation(BlockHash([0; 32]))
        .build();

    let account = st.test_account().unwrap();
    let account_id = account.id();
    let birthday = account.birthday().height();
    let taddr = st
        .wallet()
        .get_last_generated_address_matching(account_id, UnifiedAddressRequest::AllAvailableKeys)
        .unwrap()
        .unwrap()
        .transparent()
        .copied()
        .unwrap();

    let mined_height = birthday + 100;
    st.wallet_mut().update_chain_tip(mined_height + 1).unwrap();

    let value = Zatoshis::const_from_u64(163_400);
    let utxo = WalletTransparentOutput::from_parts(
        OutPoint::new([1; 32], 0),
        TxOut::new(value, taddr.script().into()),
        Some(mined_height),
        Some(account_id),
        Some(TransparentKeyScope::EXTERNAL),
        None,
    )
    .unwrap();
    st.wallet_mut()
        .put_received_transparent_utxo(&utxo)
        .unwrap();

    let balance_at_two_confirmations = st
        .wallet()
        .get_wallet_summary(ConfirmationsPolicy::default())
        .unwrap()
        .unwrap()
        .account_balances()
        .get(&account_id)
        .copied()
        .unwrap();
    assert_eq!(
        balance_at_two_confirmations
            .unshielded_balance()
            .spendable_value(),
        Zatoshis::ZERO
    );
    assert_eq!(
        balance_at_two_confirmations
            .unshielded_balance()
            .value_pending_spendability(),
        value
    );
    assert_eq!(
        balance_at_two_confirmations.unshielded_balance().total(),
        value
    );

    st.wallet_mut().update_chain_tip(mined_height + 9).unwrap();

    let balance_at_ten_confirmations = st
        .wallet()
        .get_wallet_summary(ConfirmationsPolicy::default())
        .unwrap()
        .unwrap()
        .account_balances()
        .get(&account_id)
        .copied()
        .unwrap();
    assert_eq!(
        balance_at_ten_confirmations
            .unshielded_balance()
            .spendable_value(),
        value
    );
    assert_eq!(
        balance_at_ten_confirmations
            .unshielded_balance()
            .value_pending_spendability(),
        Zatoshis::ZERO
    );

    let wallet_created_value = Zatoshis::const_from_u64(200_000);
    let wallet_created_txid = TxId::from_bytes([2; 32]);
    let wallet_created_outpoint = OutPoint::new([2; 32], 0);
    let wallet_created_height = mined_height + 8;
    let wallet_created_utxo = WalletTransparentOutput::from_parts(
        wallet_created_outpoint,
        TxOut::new(wallet_created_value, taddr.script().into()),
        Some(wallet_created_height),
        Some(account_id),
        Some(TransparentKeyScope::EXTERNAL),
        None,
    )
    .unwrap();
    st.wallet_mut()
        .put_received_transparent_utxo(&wallet_created_utxo)
        .unwrap();
    st.wallet_mut()
        .transparent_received_output_spends
        .insert(OutPoint::new([3; 32], 0), wallet_created_txid);

    let balance_at_two_trusted_confirmations = st
        .wallet()
        .get_wallet_summary(ConfirmationsPolicy::default())
        .unwrap()
        .unwrap()
        .account_balances()
        .get(&account_id)
        .copied()
        .unwrap();
    assert_eq!(
        balance_at_two_trusted_confirmations
            .unshielded_balance()
            .spendable_value(),
        value
    );
    assert_eq!(
        balance_at_two_trusted_confirmations
            .unshielded_balance()
            .value_pending_spendability(),
        wallet_created_value
    );

    st.wallet_mut()
        .update_chain_tip(wallet_created_height + 2)
        .unwrap();

    let balance_at_three_trusted_confirmations = st
        .wallet()
        .get_wallet_summary(ConfirmationsPolicy::default())
        .unwrap()
        .unwrap()
        .account_balances()
        .get(&account_id)
        .copied()
        .unwrap();
    assert_eq!(
        balance_at_three_trusted_confirmations
            .unshielded_balance()
            .spendable_value(),
        (value + wallet_created_value).unwrap()
    );
    assert_eq!(
        balance_at_three_trusted_confirmations
            .unshielded_balance()
            .value_pending_spendability(),
        Zatoshis::ZERO
    );
}

#[test]
fn recoverable_ephemeral_output_requests_spend_discovery() {
    use zcash_client_backend::data_api::TransactionDataRequest;

    let mut st = TestBuilder::new()
        .with_data_store_factory(TestMemDbFactory::new())
        .with_account_from_sapling_activation(BlockHash([0; 32]))
        .build();

    let account_id = st.test_account().unwrap().id();
    let ephemeral_address = st
        .wallet_mut()
        .reserve_next_n_ephemeral_addresses(account_id, 1)
        .unwrap()[0]
        .0;
    let mined_height = st.test_account().unwrap().birthday().height() + 100;
    let chain_tip = mined_height + 20;
    st.wallet_mut().update_chain_tip(chain_tip).unwrap();

    let outpoint = OutPoint::new([9; 32], 0);
    let output = WalletTransparentOutput::from_parts(
        outpoint.clone(),
        TxOut::new(
            Zatoshis::const_from_u64(20_000),
            ephemeral_address.script().into(),
        ),
        Some(mined_height),
        Some(account_id),
        Some(TransparentKeyScope::EPHEMERAL),
        None,
    )
    .unwrap();
    st.wallet_mut()
        .put_received_transparent_utxo(&output)
        .unwrap();

    let request = st
        .wallet()
        .transaction_data_requests()
        .unwrap()
        .into_iter()
        .find_map(|request| match request {
            TransactionDataRequest::TransactionsInvolvingAddress(request) => Some(request),
            _ => None,
        })
        .expect("ephemeral output should request transparent spend discovery");
    assert_eq!(request.address(), ephemeral_address);
    assert_eq!(request.block_range_start(), mined_height);
    assert_eq!(request.block_range_end(), Some(chain_tip + 1));

    st.wallet_mut()
        .notify_address_checked(request, chain_tip)
        .unwrap();
    assert!(
        !st.wallet()
            .transaction_data_requests()
            .unwrap()
            .iter()
            .any(|request| matches!(
                request,
                TransactionDataRequest::TransactionsInvolvingAddress(_)
            ))
    );

    st.wallet_mut().update_chain_tip(chain_tip + 1).unwrap();
    let next_request = st
        .wallet()
        .transaction_data_requests()
        .unwrap()
        .into_iter()
        .find_map(|request| match request {
            TransactionDataRequest::TransactionsInvolvingAddress(request) => Some(request),
            _ => None,
        })
        .expect("a new block should advance transparent spend discovery");
    assert_eq!(next_request.block_range_start(), chain_tip + 1);

    st.wallet_mut()
        .mark_transparent_output_spent(&TxId::from_bytes([10; 32]), &outpoint)
        .unwrap();
    assert!(
        !st.wallet()
            .transaction_data_requests()
            .unwrap()
            .iter()
            .any(|request| matches!(
                request,
                TransactionDataRequest::TransactionsInvolvingAddress(_)
            ))
    );
}

#[test]
fn transparent_balance_across_shielding() {
    zcash_client_backend::data_api::testing::transparent::transparent_balance_across_shielding(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn reserve_next_n_internal_addresses_gap_limit() {
    zcash_client_backend::data_api::testing::transparent::reserve_next_n_internal_addresses_gap_limit(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
        |error, account_id, expected_index| {
            matches!(
                error,
                crate::Error::ReachedGapLimit(actual, index)
                    if actual == &account_id && index == &expected_index
            )
        },
    );
}

#[test]
fn propose_t2t_with_transparent_change() {
    zcash_client_backend::data_api::testing::transparent::propose_t2t_with_transparent_change(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn propose_t2tex_with_transparent_funding() {
    zcash_client_backend::data_api::testing::transparent::propose_t2tex_with_transparent_funding(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn propose_t2shielded_requires_transparent_regather() {
    zcash_client_backend::data_api::testing::transparent::propose_t2shielded_requires_transparent_regather(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn value_bounded_transparent_gather() {
    zcash_client_backend::data_api::testing::transparent::value_bounded_transparent_gather(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}

#[test]
fn transparent_send_max_uses_only_transparent_inputs() {
    zcash_client_backend::data_api::testing::transparent::transparent_send_max_uses_only_transparent_inputs(
        TestMemDbFactory::new(),
        MemBlockCache::new(),
    );
}
