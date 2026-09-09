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
use zcash_protocol::{consensus::BlockHeight, value::Zatoshis};

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

fn transparent_test_transaction(
    prevout: OutPoint,
    outputs: Vec<TxOut>,
    expiry_height: BlockHeight,
) -> zcash_primitives::transaction::Transaction {
    use transparent::{
        address::Script,
        bundle::{Authorized, Bundle, TxIn},
    };
    use zcash_primitives::transaction::{TransactionData, TxVersion};
    use zcash_protocol::consensus::BranchId;

    TransactionData::from_parts(
        TxVersion::V4,
        BranchId::Sapling,
        0,
        expiry_height,
        #[cfg(all(zcash_unstable = "nu7", feature = "zip-233"))]
        Zatoshis::ZERO,
        Some(Bundle {
            vin: vec![TxIn::from_parts(prevout, Script::default(), u32::MAX)],
            vout: outputs,
            authorization: Authorized,
        }),
        None,
        None,
        None,
    )
    .freeze()
    .unwrap()
}

fn transparent_spend_discovery_clears_balance(scope: TransparentKeyScope) {
    use transparent::address::TransparentAddress;
    use zcash_client_backend::data_api::{DecryptedTransaction, TransactionDataRequest};

    let mut st = TestBuilder::new()
        .with_data_store_factory(TestMemDbFactory::new())
        .with_account_from_sapling_activation(BlockHash([0; 32]))
        .build();
    let account_id = st.test_account().unwrap().id();
    let mined_height = st.test_account().unwrap().birthday().height() + 100;
    let chain_tip = mined_height + 20;
    st.wallet_mut().update_chain_tip(chain_tip).unwrap();
    let address = if scope == TransparentKeyScope::INTERNAL {
        st.wallet_mut()
            .reserve_next_n_internal_addresses(account_id, 1)
            .unwrap()[0]
            .0
    } else {
        *st.wallet()
            .get_last_generated_address_matching(
                account_id,
                UnifiedAddressRequest::AllAvailableKeys,
            )
            .unwrap()
            .unwrap()
            .transparent()
            .unwrap()
    };

    // Historical enhancement can insert an already-spent output even when the
    // current network UTXO response is empty. Its expiry is still in the future:
    // mining must make ordinary-address spend discovery immediately eligible.
    let received = transparent_test_transaction(
        OutPoint::new([15; 32], 0),
        vec![TxOut::new(
            Zatoshis::const_from_u64(100_000),
            address.script().into(),
        )],
        chain_tip + 100,
    );
    st.wallet_mut()
        .store_decrypted_tx(DecryptedTransaction::new(
            Some(mined_height),
            &received,
            vec![],
            #[cfg(feature = "orchard")]
            vec![],
            #[cfg(feature = "orchard")]
            vec![],
        ))
        .unwrap();
    assert_eq!(
        st.wallet()
            .get_transparent_balances(
                account_id,
                (chain_tip + 1).into(),
                ConfirmationsPolicy::default(),
            )
            .unwrap()
            .get(&address)
            .unwrap()
            .1
            .total(),
        Zatoshis::const_from_u64(100_000)
    );

    // Existing snapshots with a stale balance must also recover on their next sync.
    let mut snapshot = Vec::new();
    st.wallet().encode(&mut snapshot).unwrap();
    let reopened =
        crate::MemoryWalletDb::decode_new(snapshot.as_slice(), *st.wallet().params(), 100).unwrap();
    *st.wallet_mut() = reopened;

    let requests = st.wallet().transaction_data_requests().unwrap();
    let request = requests
        .into_iter()
        .find_map(|request| match request {
            TransactionDataRequest::TransactionsInvolvingAddress(request)
                if request.address() == address =>
            {
                Some(request)
            }
            _ => None,
        })
        .expect("ordinary transparent output must request its spending transactions");
    assert_eq!(request.block_range_start(), mined_height);
    assert_eq!(request.block_range_end(), Some(chain_tip + 1));

    let spent = transparent_test_transaction(
        OutPoint::new(received.txid().into(), 0),
        vec![TxOut::new(
            Zatoshis::const_from_u64(90_000),
            TransparentAddress::PublicKeyHash([3; 20]).script().into(),
        )],
        chain_tip + 100,
    );
    st.wallet_mut()
        .store_decrypted_tx(DecryptedTransaction::new(
            Some(mined_height + 4),
            &spent,
            vec![],
            #[cfg(feature = "orchard")]
            vec![],
            #[cfg(feature = "orchard")]
            vec![],
        ))
        .unwrap();
    st.wallet_mut()
        .notify_address_checked(request, chain_tip)
        .unwrap();
    assert!(
        st.wallet()
            .get_transparent_balances(
                account_id,
                (chain_tip + 1).into(),
                ConfirmationsPolicy::default(),
            )
            .unwrap()
            .is_empty()
    );
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
fn external_transparent_spend_discovery_clears_balance() {
    transparent_spend_discovery_clears_balance(TransparentKeyScope::EXTERNAL);
}

#[test]
fn internal_transparent_spend_discovery_clears_balance() {
    transparent_spend_discovery_clears_balance(TransparentKeyScope::INTERNAL);
}

#[test]
fn transparent_spend_discovery_coalesces_ranges_and_persists_progress() {
    use crate::MemoryWalletDb;
    use zcash_client_backend::data_api::TransactionDataRequest;

    let mut st = TestBuilder::new()
        .with_data_store_factory(TestMemDbFactory::new())
        .with_account_from_sapling_activation(BlockHash([0; 32]))
        .build();
    let account_id = st.test_account().unwrap().id();
    let mined_height = st.test_account().unwrap().birthday().height() + 100;
    let chain_tip = mined_height + 20;
    st.wallet_mut().update_chain_tip(chain_tip).unwrap();
    let address = *st
        .wallet()
        .get_last_generated_address_matching(account_id, UnifiedAddressRequest::AllAvailableKeys)
        .unwrap()
        .unwrap()
        .transparent()
        .unwrap();
    for (hash, height) in [(21, mined_height), (22, mined_height + 5)] {
        let output = WalletTransparentOutput::from_parts(
            OutPoint::new([hash; 32], 0),
            TxOut::new(Zatoshis::const_from_u64(100_000), address.script().into()),
            Some(height),
            Some(account_id),
            Some(TransparentKeyScope::EXTERNAL),
            None,
        )
        .unwrap();
        st.wallet_mut()
            .put_received_transparent_utxo(&output)
            .unwrap();
    }
    let address_requests = |db: &MemoryWalletDb<_>| {
        db.transaction_data_requests()
            .unwrap()
            .into_iter()
            .filter_map(|request| match request {
                TransactionDataRequest::TransactionsInvolvingAddress(request) => Some(request),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let requests = address_requests(st.wallet());
    assert_eq!(
        requests.len(),
        1,
        "query a shared address once, from its earliest unchecked output"
    );
    assert_eq!(requests[0].block_range_start(), mined_height);
    assert_eq!(requests[0].block_range_end(), Some(chain_tip + 1));
    st.wallet_mut()
        .notify_address_checked(requests[0].clone(), chain_tip)
        .unwrap();

    let mut bytes = Vec::new();
    st.wallet().encode(&mut bytes).unwrap();
    let mut reopened =
        MemoryWalletDb::decode_new(bytes.as_slice(), *st.wallet().params(), 100).unwrap();
    assert!(
        address_requests(&reopened).is_empty(),
        "successful checks must survive reload"
    );
    reopened.update_chain_tip(chain_tip + 1).unwrap();
    let requests = address_requests(&reopened);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].block_range_start(), chain_tip + 1);
    assert_eq!(requests[0].block_range_end(), Some(chain_tip + 2));
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
