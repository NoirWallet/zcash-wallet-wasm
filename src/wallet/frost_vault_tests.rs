//! FROST vault account import.
//!
//! A vault's FVK is `ak ‖ nk ‖ rivk` assembled from a DKG, not derived from a seed. The hardware
//! wallet import path requires a ZIP-32 derivation and would therefore have to invent one; this
//! module's test pins that the vault account is recorded with **no** derivation, and that none of
//! the wallet's other guarantees (shielded-only address, viewing key binding) are lost in exchange.

use zcash_client_backend::data_api::{Account as _, AccountBirthday, AccountPurpose, AccountSource, WalletRead, WalletWrite};
use zcash_client_memory::MemoryWalletDb;
use zcash_protocol::consensus::BlockHeight;

type Db = MemoryWalletDb<zcash_protocol::consensus::Network>;

fn test_db() -> Db {
    MemoryWalletDb::new(zcash_protocol::consensus::Network::TestNetwork, 100)
}

/// The protocol-level network, which is what keys and addresses are encoded against.
const NETWORK: zcash_protocol::consensus::Network = zcash_protocol::consensus::Network::TestNetwork;

fn birthday() -> AccountBirthday {
    AccountBirthday::from_parts(
        zcash_client_backend::data_api::chain::ChainState::empty(BlockHeight::from_u32(3_000_000), zcash_primitives::block::BlockHash([0; 32])),
        None,
    )
}

/// A deterministic `qsk` for the keyset derivation.
fn qsk() -> [u8; 32] {
    let mut k = [0u8; 32];
    k[0] = 0x77;
    k[31] = 0x01;
    k
}

fn vault_keyset() -> vault_keyset::VaultKeyset {
    let mut rng = rand_core::OsRng;
    let dkg = frost_vault_core::threshold::run_dkg(2, 2, &mut rng).expect("dkg");
    let ak = dkg.group_verifying_key().expect("ak");
    vault_keyset::VaultKeyset::derive(&ak, &qsk(), NETWORK, 0).expect("keyset")
}

#[test]
fn a_vault_account_is_imported_without_a_derivation() {
    let keyset = vault_keyset();
    let mut db = test_db();
    let ufvk = zcash_keys::keys::UnifiedFullViewingKey::decode(&NETWORK, &keyset.ufvk).expect("vault ufvk decodes");

    let id = db
        .import_account_ufvk(
            "frost-vault",
            &ufvk,
            &birthday(),
            AccountPurpose::Spending { derivation: None },
            Some("frost-dkg"),
        )
        .expect("import without derivation")
        .id();

    let account = db.get_account(id).expect("get").expect("present");

    // The account really has no derivation: this is the property that distinguishes it from a
    // hardware or seed account, and inventing one would mislead any signer that reads it.
    match account.source() {
        AccountSource::Imported {
            purpose: AccountPurpose::Spending { derivation },
            key_source,
        } => {
            assert!(derivation.is_none(), "a vault account must have no derivation");
            assert_eq!(key_source.as_deref(), Some("frost-dkg"));
        }
        other => panic!("unexpected account source: {other:?}"),
    }

    // The viewing key is exactly the one the keyset produced, so the address a payer sees is the
    // vault's address and not something the wallet re-derived.
    let stored = account.ufvk().expect("ufvk");
    let orchard = stored.orchard().expect("orchard component");
    assert_eq!(&orchard.to_bytes(), &keyset.fvk);
}

#[test]
fn a_vault_account_address_is_shielded_only() {
    // P8: a transparent receiver would let someone send funds the threshold can never spend.
    let keyset = vault_keyset();
    let mut db = test_db();
    let ufvk = zcash_keys::keys::UnifiedFullViewingKey::decode(&NETWORK, &keyset.ufvk).unwrap();
    let id = db
        .import_account_ufvk("frost-vault", &ufvk, &birthday(), AccountPurpose::Spending { derivation: None }, None)
        .unwrap()
        .id();
    let account = db.get_account(id).unwrap().unwrap();
    let stored = account.ufvk().unwrap();

    // The UFVK carries no transparent or Sapling component, so no address derived from it can.
    assert!(stored.transparent().is_none(), "no transparent key");
    assert!(stored.sapling().is_none(), "no sapling key");
    assert!(stored.orchard().is_some());
}

#[test]
fn a_view_only_import_is_still_possible_for_the_same_keyset() {
    // Sanity check that the vault keyset is not tied to one purpose: the same UFVK can be imported
    // read-only, which is what a watching device would do.
    let keyset = vault_keyset();
    let mut db = test_db();
    let ufvk = zcash_keys::keys::UnifiedFullViewingKey::decode(&NETWORK, &keyset.ufvk).unwrap();
    let id = db
        .import_account_ufvk("vault-watch", &ufvk, &birthday(), AccountPurpose::ViewOnly, None)
        .unwrap()
        .id();
    let account = db.get_account(id).unwrap().unwrap();
    assert!(matches!(
        account.source(),
        AccountSource::Imported {
            purpose: AccountPurpose::ViewOnly,
            ..
        }
    ));
}

#[test]
fn the_wasm_import_rejects_a_malformed_ufvk_before_touching_the_network() {
    // The wrapper decodes first, so a bad string must fail here rather than at the lightwalletd call
    // (this test runs offline).
    let wallet = crate::Wallet::new(
        test_db(),
        tonic_web_wasm_client::Client::new("https://example.invalid".into()),
        crate::Network::TestNetwork,
    )
    .expect("wallet");

    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    let error = runtime
        .block_on(wallet.create_account_ufvk_external("bad", "uview1not-a-key", Some(3_000_000), None))
        .expect_err("must be refused");
    assert!(matches!(error, crate::error::Error::KeyDecoding(_)), "expected a decoding error, got {error:?}");
}
