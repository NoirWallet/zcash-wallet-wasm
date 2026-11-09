//! Minimal wasm exports: proof that the protocol crates run under `wasm32`, not just compile.
//!
//! Every crate in this workspace builds for `wasm32-unknown-unknown`, but building is not running:
//! a wasm32 target has a **32-bit `usize`**, and it resolves symbols and panics differently. The
//! tests in the other crates all execute natively on 64-bit, so they cannot see those differences.
//!
//! This crate exposes a handful of `extern "C"` functions with no wasm-bindgen dependency, so a
//! plain `WebAssembly.instantiate` in Node can call into real protocol code and check the results.
//! See `run.mjs` — it is wired into `make wasm-smoke` and is expected to be run before any frontend
//! work starts.
//!
//! Keep the surface tiny and keep the return type `u32`: the point is to exercise the crates, not to
//! be an API.

use frost_vault_core::protocol::{Envelope, MsgType, COORDINATOR_SLOT};

/// Width of `usize` in bits. The single most useful value here: on wasm32 it must be 32, and every
/// `as usize` in the protocol code is then operating on a narrower type than the native tests saw.
#[no_mangle]
pub extern "C" fn fv_usize_bits() -> u32 {
    (core::mem::size_of::<usize>() * 8) as u32
}

/// Encode and decode an envelope; returns 0 on agreement.
#[no_mangle]
pub extern "C" fn fv_envelope_roundtrip() -> u32 {
    let payload: Vec<u8> = (0..64u8).collect();
    let Ok(env) = Envelope::new(
        MsgType::SignPayload,
        3,
        COORDINATOR_SLOT,
        1,
        7,
        [0x42u8; 16],
        payload.clone(),
    ) else {
        return 1;
    };
    let bytes = env.encode();
    match Envelope::decode(&bytes) {
        Ok(back) if back == env && back.payload == payload => 0,
        _ => 2,
    }
}

/// A truncated envelope must be refused, not panic.
#[no_mangle]
pub extern "C" fn fv_truncated_envelope_refused() -> u32 {
    let Ok(env) = Envelope::new(
        MsgType::SignCommitments,
        1,
        0,
        COORDINATOR_SLOT,
        1,
        [0x11u8; 16],
        vec![7u8; 40],
    ) else {
        return 1;
    };
    let mut bytes = env.encode();
    bytes.truncate(bytes.len() - 3);
    match Envelope::decode(&bytes) {
        Ok(_) => 2,
        Err(_) => 0,
    }
}

/// Envelope size, so the metric baseline can be checked from inside wasm too.
#[no_mangle]
pub extern "C" fn fv_envelope_len() -> u32 {
    Envelope::new(
        MsgType::SignInit,
        1,
        COORDINATOR_SLOT,
        0,
        1,
        [0u8; 16],
        vec![0u8; 16],
    )
    .map(|e| e.encode().len() as u32)
    .unwrap_or(0)
}

/// Number of canonical outputs a deposit of `amount` zatoshi needs.
#[no_mangle]
pub extern "C" fn fv_plan_outputs(amount: u64) -> u32 {
    match vault_deposit::plan_deposit(amount) {
        Ok(plan) => plan.outputs.len() as u32,
        Err(_) => 0,
    }
}

/// Total value that would actually move.
#[no_mangle]
pub extern "C" fn fv_plan_total(amount: u64) -> u64 {
    vault_deposit::plan_deposit(amount)
        .map(|p| p.total)
        .unwrap_or(0)
}

/// Residual left behind.
#[no_mangle]
pub extern "C" fn fv_plan_residual(amount: u64) -> u64 {
    vault_deposit::plan_deposit(amount)
        .map(|p| p.residual)
        .unwrap_or(0)
}

/// Whether every output is canonical.
#[no_mangle]
pub extern "C" fn fv_plan_all_canonical(amount: u64) -> u32 {
    match vault_deposit::plan_deposit(amount) {
        Ok(plan) if plan.outputs.iter().all(|v| vault_deposit::is_canonical(*v)) => 1,
        _ => 0,
    }
}

/// Canary gating: 1 when a bulk deposit is refused before the canary is proven.
#[no_mangle]
pub extern "C" fn fv_canary_blocks_bulk() -> u32 {
    let mut canary = vault_deposit::Canary::new();
    if canary.begin(vault_deposit::CANARY_AMOUNT_ZATS).is_err() {
        return 0;
    }
    let bulk_refused = !canary.allows(vault_deposit::COIN);
    let canary_allowed = canary.allows(vault_deposit::CANARY_AMOUNT_ZATS);
    let still_refused_after_arrival = {
        canary.arrival_confirmed().is_ok() && !canary.allows(vault_deposit::COIN)
    };
    let allowed_after_spend = {
        canary.spend_confirmed().is_ok() && canary.allows(vault_deposit::COIN)
    };
    u32::from(
        bulk_refused && canary_allowed && still_refused_after_arrival && allowed_after_spend,
    )
}
