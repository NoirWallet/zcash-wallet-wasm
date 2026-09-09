//! Offline diagnostic for transparent spend discovery, using public transaction fixtures.
//!
//! Pass the JSON produced by the companion chain-query script as the first argument.
//! A BIP-39 seed, encoded as hex, is read from stdin; it is never printed or persisted.
//! This replays transaction enhancement, not the entire compact-block scanner.
//! Balances below refer only to the fixture's address, not the account total:
//! change addresses require their own history queries to determine that total.

use std::{fs, io};

use secrecy::Zeroize;
use serde_json::{Value, json};
use zcash_client_backend::{
    data_api::{
        Account as _, AccountBirthday, AccountPurpose, TransactionDataRequest, WalletRead,
        WalletWrite, chain::ChainState, wallet::ConfirmationsPolicy,
    },
    decrypt_transaction,
    encoding::AddressCodec,
};
use zcash_client_memory::MemoryWalletDb;
use zcash_keys::keys::{UnifiedAddressRequest, UnifiedSpendingKey};
use zcash_primitives::{block::BlockHash, transaction::Transaction};
use zcash_protocol::consensus::{BlockHeight, BranchId, MAIN_NETWORK};
use zip32::DiversifierIndex;

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "Invalid hex length");
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).expect("Invalid hex"))
        .collect()
}

fn main() {
    let fixture: Value = serde_json::from_str(
        &fs::read_to_string(
            std::env::args()
                .nth(1)
                .expect("Expected public fixture path"),
        )
        .expect("Unable to read public fixture"),
    )
    .expect("Invalid public fixture");
    let address = fixture["address"].as_str().unwrap();
    let tip = BlockHeight::from_u32(fixture["tip"].as_u64().unwrap().try_into().unwrap());
    let birthday_height = BlockHeight::from_u32(3_471_108);

    let mut seed_hex = String::new();
    io::stdin()
        .read_line(&mut seed_hex)
        .expect("Expected seed on stdin");
    let mut seed = decode_hex(seed_hex.trim());
    seed_hex.zeroize();
    let ufvk = UnifiedSpendingKey::from_seed(&MAIN_NETWORK, &seed, zip32::AccountId::ZERO)
        .expect("Unable to derive account zero")
        .to_unified_full_viewing_key();
    seed.zeroize();

    let birthday = AccountBirthday::from_parts(
        ChainState::empty(birthday_height - 1, BlockHash([0; 32])),
        None,
    );
    let mut db = MemoryWalletDb::new(MAIN_NETWORK, 100);
    let account = db
        .import_account_ufvk(
            "diagnostic",
            &ufvk,
            &birthday,
            AccountPurpose::ViewOnly,
            None,
        )
        .unwrap()
        .id();
    db.get_address_for_index(
        account,
        DiversifierIndex::new(),
        UnifiedAddressRequest::ALLOW_ALL,
    )
    .unwrap();
    db.get_next_available_address(account, UnifiedAddressRequest::AllAvailableKeys)
        .unwrap();
    db.update_chain_tip(tip).unwrap();
    let receivers = db.get_transparent_receivers(account, true, true).unwrap();
    let taddr = *receivers
        .keys()
        .find(|addr| addr.encode(&MAIN_NETWORK) == address)
        .expect("The input seed does not derive the fixture's address");
    let ufvks = db.get_unified_full_viewing_keys().unwrap();
    let balance = |db: &MemoryWalletDb<_>| {
        db.get_transparent_balances(account, (tip + 1).into(), ConfirmationsPolicy::default())
            .unwrap()
            .get(&taddr)
            .map_or(0, |(_, amount)| u64::from(amount.total()))
    };
    let mut transactions = Vec::new();
    for item in fixture["transactions"].as_array().unwrap() {
        let height = BlockHeight::from_u32(item["height"].as_u64().unwrap().try_into().unwrap());
        assert!(height >= birthday_height);
        let raw = decode_hex(item["raw"].as_str().unwrap());
        let tx = Transaction::read(raw.as_slice(), BranchId::for_height(&MAIN_NETWORK, height))
            .expect("Unable to parse public transaction");
        transactions.push((height, tx));
    }

    // On a fresh import the current UTXO response is empty. Historical shielded
    // transaction enhancement nevertheless inserts their transparent outputs.
    assert!(fixture["utxos"].as_array().unwrap().is_empty());
    for (height, tx) in &transactions {
        let decrypted = decrypt_transaction(&MAIN_NETWORK, Some(*height), Some(tip), tx, &ufvks);
        let discoverable = decrypted.has_decrypted_outputs();
        let inputs: Vec<_> = tx
            .transparent_bundle()
            .iter()
            .flat_map(|b| &b.vin)
            .map(|input| format!("{}:{}", input.prevout().txid(), input.prevout().n()))
            .collect();
        let outputs: Vec<_> = tx
            .transparent_bundle()
            .iter()
            .flat_map(|b| &b.vout)
            .enumerate()
            .map(|(index, output)| {
                json!({
                    "index": index,
                    "valueZat": u64::from(output.value()),
                    "address": output.recipient_address().map(|a| a.encode(&MAIN_NETWORK)),
                })
            })
            .collect();
        if discoverable {
            db.store_decrypted_tx(decrypted).unwrap();
        }
        println!(
            "{}",
            json!({
                "height": u32::from(*height), "txid": tx.txid().to_string(),
                "shieldedEnhancement": discoverable,
                "hasShieldedBundle": tx.sapling_bundle().is_some() || tx.orchard_bundle().is_some() || tx.ironwood_bundle().is_some(),
                "inputs": inputs, "outputs": outputs,
                "fixtureAddressBalanceZat": balance(&db),
            })
        );
    }
    let requests = db.transaction_data_requests().unwrap();
    let address_requests = requests
        .into_iter()
        .filter_map(|request| match request {
            TransactionDataRequest::TransactionsInvolvingAddress(request)
                if request.address() == taddr =>
            {
                Some(request)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let stale_balance = balance(&db);
    println!(
        "{}",
        json!({
            "stage": "after_shielded_enhancement", "address": address,
            "birthday": u32::from(birthday_height), "tip": u32::from(tip),
            "chainAddressBalanceZat": 0, "fixtureAddressBalanceZat": stale_balance,
            "addressHistoryRequests": address_requests.len(),
        })
    );
    assert_eq!(
        stale_balance, 100_000,
        "Historical enhancement recreates the stale output"
    );
    assert_eq!(
        address_requests.len(),
        1,
        "The wallet must discover ordinary transparent spends"
    );

    // Fulfill only ranges requested by the production backend, using the public
    // address history as the network response. No balance or spent flag is edited.
    let mut found_reported_spend = false;
    for request in address_requests {
        let end = request.block_range_end().unwrap_or(tip + 1);
        for (height, tx) in &transactions {
            if *height >= request.block_range_start() && *height < end {
                let decrypted =
                    decrypt_transaction(&MAIN_NETWORK, Some(*height), Some(tip), tx, &ufvks);
                db.store_decrypted_tx(decrypted).unwrap();
                found_reported_spend |= tx.txid().to_string()
                    == "cb678f3ba952fef9c74ba055536d5cbbfa0bec42cca413e670c33b333e3be5d8";
            }
        }
        db.notify_address_checked(request, end - 1).unwrap();
    }
    println!(
        "{}",
        json!({
            "stage": "after_requested_address_history", "fixtureAddressBalanceZat": balance(&db),
            "foundReportedSpend": found_reported_spend,
        })
    );
    assert!(
        found_reported_spend,
        "The generated query must include the missing spending transaction"
    );
    assert_eq!(
        balance(&db),
        0,
        "Requested transaction history must clear the fixture address's spent balance, not all account change"
    );
}
