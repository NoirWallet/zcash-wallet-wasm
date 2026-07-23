use crate::testing::{MemBlockCache, TestMemDbFactory};

#[test]
fn put_received_transparent_utxo() {
    zcash_client_backend::data_api::testing::transparent::put_received_transparent_utxo(
        TestMemDbFactory::new(),
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
