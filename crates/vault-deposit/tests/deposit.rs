//! Deposit policy: canonical denominations and the canary gate.

use vault_deposit::{
    is_canonical, plan_deposit, zip318_caps_agree, Canary, CanaryError, CanaryPhase, DepositError, CANARY_AMOUNT_ZATS, COIN, DENOM_CAP_ZATS, MAX_OUTPUTS,
    MAX_RESIDUAL_ZATS,
};

fn zec(n: u64) -> u64 {
    n.saturating_mul(COIN)
}

#[test]
fn the_caps_still_match_zip318() {
    assert!(zip318_caps_agree(), "ZIP 318 constants moved; revisit this crate");
    assert_eq!(DENOM_CAP_ZATS, zec(10_000));
    assert_eq!(MAX_RESIDUAL_ZATS, COIN / 100);
}

#[test]
fn a_round_amount_becomes_few_canonical_pieces() {
    // 1 ZEC is itself canonical, so a deposit of exactly 1 ZEC is one output with no residual.
    let plan = plan_deposit(zec(1)).expect("plan");
    assert_eq!(plan.outputs, vec![zec(1)]);
    assert_eq!(plan.residual, 0);
    assert_eq!(plan.total, zec(1));
    assert_eq!(plan.estimated_actions(), 1);
}

#[test]
fn odd_amounts_round_down_and_leave_the_residual_behind() {
    // 1.23456789 ZEC: greedy picks 1, 0.2, 0.02, 0.01 (1.23 total), leaving 0.00456789 behind.
    let amount = COIN + 23_456_789;
    let plan = plan_deposit(amount).expect("plan");
    assert_eq!(plan.outputs.iter().sum::<u64>(), plan.total);
    assert_eq!(plan.total + plan.residual, amount);
    assert!(plan.residual < MAX_RESIDUAL_ZATS, "residual must be dust");
    for (i, out) in plan.outputs.iter().enumerate() {
        assert!(is_canonical(*out), "output {i} = {out} is not canonical");
    }
    // Greedy from the largest: the first piece is the biggest canonical value that fits.
    assert_eq!(plan.outputs, vec![zec(1), COIN / 5, COIN / 50, COIN / 100]);
    assert_eq!(plan.total, COIN + 23_000_000);
    assert_eq!(plan.residual, 456_789);
}

#[test]
fn nothing_larger_than_the_cap_is_ever_emitted() {
    // 25,000 ZEC must not produce one 25,000 ZEC output: the cap is 10,000.
    let plan = plan_deposit(zec(25_000)).expect("plan");
    assert!(plan.outputs.iter().all(|v| *v <= DENOM_CAP_ZATS));
    assert_eq!(plan.outputs.len(), 3, "10k + 10k + 5k");
    assert_eq!(plan.outputs, vec![zec(10_000), zec(10_000), zec(5_000)]);
    assert_eq!(plan.residual, 0);
}

#[test]
fn below_the_smallest_denomination_nothing_happens() {
    assert_eq!(
        plan_deposit(MAX_RESIDUAL_ZATS - 1),
        Err(DepositError::BelowMinimum { minimum: MAX_RESIDUAL_ZATS })
    );
    assert_eq!(plan_deposit(0), Err(DepositError::BelowMinimum { minimum: MAX_RESIDUAL_ZATS }));
    // Exactly the minimum is sendable.
    let plan = plan_deposit(MAX_RESIDUAL_ZATS).expect("plan");
    assert_eq!(plan.outputs, vec![MAX_RESIDUAL_ZATS]);
}

#[test]
fn a_deposit_that_would_need_too_many_outputs_is_refused() {
    // Force the worst case: values just above the minimum, where greedy produces many pieces.
    // A plan that exceeds MAX_OUTPUTS must be reported, not silently truncated.
    let mut caused = false;
    for n in 1..200u64 {
        let amount = MAX_RESIDUAL_ZATS + n;
        match plan_deposit(amount) {
            Err(DepositError::TooManyOutputs { .. }) => {
                caused = true;
                break;
            }
            _ => {}
        }
    }
    assert!(!caused, "no amount this small needs more than {MAX_OUTPUTS} outputs");
    // And the invariant holds for a large spread of amounts.
    for k in 1..5000u64 {
        let amount = MAX_RESIDUAL_ZATS * (1 + k);
        let plan = plan_deposit(amount).expect("plan");
        assert!(plan.outputs.len() <= MAX_OUTPUTS, "amount {amount}");
    }
}

#[test]
fn every_amount_keeps_its_total_and_dust_below_the_threshold() {
    // Property sweep: total + residual == amount, residual is dust, outputs canonical and capped.
    for step in 1..=2_000u64 {
        let amount = MAX_RESIDUAL_ZATS.saturating_mul(step) / 3 + step;
        if amount < MAX_RESIDUAL_ZATS {
            continue;
        }
        let plan = plan_deposit(amount).expect("plan");
        assert_eq!(plan.total + plan.residual, amount);
        assert!(plan.residual < MAX_RESIDUAL_ZATS);
        assert!(!plan.outputs.is_empty());
        for out in &plan.outputs {
            assert!(is_canonical(*out));
            assert!(*out <= DENOM_CAP_ZATS);
        }
    }
}

// ---------------------------------------------------------------------------
// Canary
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_canary_allows_nothing() {
    let canary = Canary::new();
    assert_eq!(canary.phase(), CanaryPhase::NotStarted);
    assert!(!canary.allows(CANARY_AMOUNT_ZATS));
    assert!(!canary.allows(zec(1)));
}

#[test]
fn only_the_canary_amount_is_allowed_before_proof() {
    let mut canary = Canary::new();
    canary.begin(CANARY_AMOUNT_ZATS).expect("begin");
    assert!(canary.allows(CANARY_AMOUNT_ZATS));
    assert!(!canary.allows(zec(1)), "bulk deposit must be refused");
    assert!(!canary.allows(CANARY_AMOUNT_ZATS * 2));

    canary.arrival_confirmed().expect("arrival");
    assert_eq!(canary.phase(), CanaryPhase::AwaitingSpend);
    assert!(!canary.allows(zec(1)), "arrival alone is not proof");

    canary.spend_confirmed().expect("spend");
    assert_eq!(canary.phase(), CanaryPhase::Proven);
    assert!(canary.allows(zec(1)));
    assert!(canary.allows(zec(100)));
    // Even after proving, dust is still not worth sending.
    assert!(!canary.allows(MAX_RESIDUAL_ZATS - 1));
}

#[test]
fn the_canary_amount_must_be_small_and_canonical() {
    let mut canary = Canary::new();
    assert_eq!(
        canary.begin(CANARY_AMOUNT_ZATS * 2),
        Err(CanaryError::BadAmount {
            amount: CANARY_AMOUNT_ZATS * 2,
            max: CANARY_AMOUNT_ZATS
        })
    );
    // 0.007 ZEC is not in the canonical set.
    assert!(matches!(canary.begin(7_000_000), Err(CanaryError::BadAmount { .. })));
    assert_eq!(canary.phase(), CanaryPhase::NotStarted);
}

#[test]
fn the_steps_cannot_be_skipped() {
    let mut canary = Canary::new();
    assert_eq!(
        canary.spend_confirmed(),
        Err(CanaryError::WrongPhase {
            expected: CanaryPhase::AwaitingSpend,
            actual: CanaryPhase::NotStarted
        })
    );
    canary.begin(CANARY_AMOUNT_ZATS).unwrap();
    // Arrival cannot be claimed twice, and spend cannot precede arrival.
    assert!(canary.spend_confirmed().is_err());
    canary.arrival_confirmed().unwrap();
    assert_eq!(
        canary.arrival_confirmed(),
        Err(CanaryError::WrongPhase {
            expected: CanaryPhase::AwaitingArrival,
            actual: CanaryPhase::AwaitingSpend
        })
    );
}

#[test]
fn a_canary_cannot_be_restarted() {
    let mut canary = Canary::new();
    canary.begin(CANARY_AMOUNT_ZATS).unwrap();
    assert_eq!(canary.begin(CANARY_AMOUNT_ZATS), Err(CanaryError::AlreadyStarted(CanaryPhase::AwaitingArrival)));
}
