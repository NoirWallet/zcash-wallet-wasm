use super::*;
use transparent::bundle::{OutPoint, TxOut};
use transparent::keys::TransparentKeyScope;
use zcash_client_backend::data_api::chain::ChainState;
use zcash_client_backend::wallet::WalletTransparentOutput;
use zcash_keys::keys::UnifiedAddressRequest;
use zcash_primitives::block::BlockHash;

type TestWallet = Wallet<MemoryWalletDb<Network>, tonic_web_wasm_client::Client>;

fn funded_db() -> (MemoryWalletDb<Network>, crate::bindgen::wallet::AccountId) {
    let network = Network::TestNetwork;
    let mut db = MemoryWalletDb::new(network, PRUNING_DEPTH);
    let height = BlockHeight::from_u32(3_000_000);
    let birthday = AccountBirthday::from_parts(ChainState::empty(height, BlockHash([0; 32])), None);
    let ufvk = UnifiedSpendingKey::from_seed(&network, &[31; 32], zip32::AccountId::ZERO)
        .unwrap()
        .to_unified_full_viewing_key();
    let account_id = db
        .import_account_ufvk(
            "confirmations fixture",
            &ufvk,
            &birthday,
            AccountPurpose::Spending {
                derivation: Some(Zip32Derivation::new(
                    SeedFingerprint::from_seed(&[31; 32]).unwrap(),
                    zip32::AccountId::ZERO,
                )),
            },
            None,
        )
        .unwrap()
        .id();
    let address = *db
        .get_last_generated_address_matching(account_id, UnifiedAddressRequest::AllAvailableKeys)
        .unwrap()
        .unwrap()
        .transparent()
        .unwrap();
    let mined = height + 100;
    db.update_chain_tip(mined + 1).unwrap();
    let output = WalletTransparentOutput::from_parts(
        OutPoint::new([41; 32], 0),
        TxOut::new(Zatoshis::const_from_u64(200_000), address.script().into()),
        Some(mined),
        Some(account_id),
        Some(TransparentKeyScope::EXTERNAL),
        None,
    )
    .unwrap();
    db.put_received_transparent_utxo(&output).unwrap();
    // Transparent proposals still require retained anchors under both policies.
    for height in [3_000_080, 3_000_101].map(BlockHeight::from_u32) {
        db.with_sapling_tree_mut(|tree| tree.checkpoint(height))
            .unwrap();
        db.with_orchard_tree_mut(|tree| tree.checkpoint(height))
            .unwrap();
    }
    (db, account_id)
}

fn client() -> tonic_web_wasm_client::Client {
    tonic_web_wasm_client::Client::new("https://example.invalid".into())
}

fn custom_policy() -> ConfirmationsPolicy {
    ConfirmationsPolicy::new(1.try_into().unwrap(), 2.try_into().unwrap(), false).unwrap()
}

#[tokio::test]
async fn custom_policy_controls_estimates_and_unsigned_transparent_transfers() {
    let (db, account_id) = funded_db();
    let mut encoded = Vec::new();
    db.encode(&mut encoded).unwrap();
    let defaulted = TestWallet::new(db, client(), Network::TestNetwork).unwrap();
    let custom = TestWallet::new_with_confirmations_policy(
        MemoryWalletDb::decode_new(&encoded[..], Network::TestNetwork, PRUNING_DEPTH).unwrap(),
        client(),
        Network::TestNetwork,
        custom_policy(),
    )
    .unwrap();
    let recipient = zcash_client_backend::address::Address::Transparent(
        transparent::address::TransparentAddress::PublicKeyHash([42; 20]),
    )
    .to_zcash_address(&Network::TestNetwork);
    for (wallet, spendable) in [(&defaulted, false), (&custom, true)] {
        let fee = wallet
            .estimate_pczt_transparent_transfer_fee(
                account_id,
                recipient.clone(),
                100_000,
                None,
                5_000,
            )
            .await;
        let estimate = wallet
            .estimate_transfer(
                account_id,
                recipient.clone(),
                100_000,
                TransferFundingSource::Transparent,
                None,
                Some(5_000),
                None,
            )
            .await;
        let pczt = wallet
            .pczt_create_transparent_dynamic_fee(
                account_id,
                recipient.clone(),
                100_000,
                None,
                5_000,
            )
            .await;
        if spendable {
            assert_eq!(fee.unwrap(), 10_000);
            assert_eq!(estimate.unwrap().fee_zats, "10000");
            let pczt = pczt.unwrap();
            assert_eq!(pczt.transparent().inputs().len(), 1);
            assert_eq!(*pczt.transparent().inputs()[0].prevout_txid(), [41; 32]);
        } else {
            for error in [fee.unwrap_err(), estimate.unwrap_err(), pczt.err().unwrap()] {
                assert!(error.to_string().contains("InsufficientFunds"), "{error}");
            }
        }
    }
}

#[tokio::test]
async fn custom_policy_makes_two_confirmation_external_funds_spendable_in_clones() {
    let (db, account_id) = funded_db();
    let wallet = TestWallet::new_with_confirmations_policy(
        db,
        client(),
        Network::TestNetwork,
        custom_policy(),
    )
    .unwrap();
    for wallet in [&wallet, &wallet.clone()] {
        let summary = wallet.get_wallet_summary().await.unwrap().unwrap();
        let balance = summary.account_balances()[&account_id].unshielded_balance();
        assert_eq!(balance.spendable_value(), Zatoshis::const_from_u64(200_000));
        assert_eq!(balance.value_pending_spendability(), Zatoshis::ZERO);
        let db = wallet.db.read().await;
        let policy = wallet.transfer_confirmations_policy(&*db, None).unwrap();
        assert_eq!(u32::from(policy.trusted()), 1);
        assert_eq!(u32::from(policy.untrusted()), 2);
        assert!(!policy.allow_zero_conf_shielding());
    }
}

#[tokio::test]
async fn confirmations_policy_is_runtime_configuration_not_wallet_database_state() {
    let (db, account_id) = funded_db();
    let wallet = TestWallet::new_with_confirmations_policy(
        db,
        client(),
        Network::TestNetwork,
        custom_policy(),
    )
    .unwrap();
    let bytes = wallet.db_to_bytes().await.unwrap();
    let decoded =
        || MemoryWalletDb::decode_new(&bytes[..], Network::TestNetwork, PRUNING_DEPTH).unwrap();
    let defaulted = TestWallet::new(decoded(), client(), Network::TestNetwork).unwrap();
    let restored = TestWallet::new_with_confirmations_policy(
        decoded(),
        client(),
        Network::TestNetwork,
        custom_policy(),
    )
    .unwrap();
    for (wallet, spendable) in [(&defaulted, 0), (&restored, 200_000)] {
        let summary = wallet.get_wallet_summary().await.unwrap().unwrap();
        assert_eq!(
            summary.account_balances()[&account_id]
                .unshielded_balance()
                .spendable_value(),
            Zatoshis::from_u64(spendable).unwrap()
        );
        assert_eq!(wallet.db_to_bytes().await.unwrap(), bytes);
    }
}

#[tokio::test]
async fn default_policy_keeps_two_confirmation_external_funds_pending() {
    let (db, account_id) = funded_db();
    let wallet = TestWallet::new(db, client(), Network::TestNetwork).unwrap();
    let summary = wallet.get_wallet_summary().await.unwrap().unwrap();
    let balance = summary.account_balances()[&account_id].unshielded_balance();
    assert_eq!(balance.spendable_value(), Zatoshis::ZERO);
    assert_eq!(
        balance.value_pending_spendability(),
        Zatoshis::const_from_u64(200_000)
    );
}

#[tokio::test]
#[ignore = "generates a public synthetic wallet for the browser policy tests"]
async fn write_confirmations_policy_browser_fixture() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("CONFIRMATIONS_FIXTURE_DIR").expect("set CONFIRMATIONS_FIXTURE_DIR"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let (db, _) = funded_db();
    let wallet = TestWallet::new(db, client(), Network::TestNetwork).unwrap();
    std::fs::write(
        directory.join("confirmations-wallet.bin"),
        wallet.db_to_bytes().await.unwrap(),
    )
    .unwrap();
}
