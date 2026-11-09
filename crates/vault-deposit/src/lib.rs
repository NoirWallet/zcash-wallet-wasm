//! Deposit policy: how much to send into a vault, in how many pieces, and when.
//!
//! Two rules, both from the review notes, and neither of them about cryptography:
//!
//! 1. **Every deposit output is drawn from the canonical `{1, 2, 5} × 10^k` set, capped at
//!    `DENOM_CAP`, with sub-`MAX_RESIDUAL_VALUE` leftovers left behind.** It is tempting to treat
//!    this as a privacy nicety. It is not: a deposit is an Ironwood → Ironwood transfer whose amount
//!    is visible on chain, and a unique amount is a fingerprint that links the sender to the vault.
//!    Rounding to the shared denomination set is the only thing that makes one deposit look like any
//!    other. The leftover is deliberately *not* swept: a distinctive dust amount would undo the
//!    point of rounding.
//! 2. **A vault only accepts a large deposit after it has been shown to be spendable.** [`Canary`]
//!    is the gate. Until a small deposit has arrived *and* been spent back out through the threshold
//!    signature, bulk deposits are refused. This is what stops the worst failure mode in the whole
//!    design: moving a balance into a vault whose shares cannot actually be combined.
//!
//! What this module cannot do is hide the *link*: the last hop into the vault is a transfer from a
//! known address, and no amount of denomination rounding changes that. The product wording must not
//! claim otherwise.

use zcash_protocol::zip318::{largest_one_two_five, DENOM_CAP, MAX_RESIDUAL_VALUE};

/// Zatoshi in one ZEC.
pub const COIN: u64 = 100_000_000;
/// Largest canonical value a single deposit output may carry (10,000 ZEC).
pub const DENOM_CAP_ZATS: u64 = 10_000 * COIN;
/// Smallest canonical value worth sending (0.01 ZEC). Below this, leave it behind.
pub const MAX_RESIDUAL_ZATS: u64 = COIN / 100;
/// Largest first deposit a canary may use (0.01 ZEC). Small enough that losing it is not a crisis,
/// large enough to exercise a real transaction end to end.
pub const CANARY_AMOUNT_ZATS: u64 = COIN / 100;

/// The constants above are copies of ZIP 318's, because `Zatoshis` has no const accessor. A test
/// asserts they still agree; if ZIP 318's caps move, this module must be revisited rather than
/// silently rounding to different values.
pub fn zip318_caps_agree() -> bool {
    DENOM_CAP.into_u64() == DENOM_CAP_ZATS && MAX_RESIDUAL_VALUE.into_u64() == MAX_RESIDUAL_ZATS
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepositError {
    /// The amount is below the smallest sendable denomination, so nothing would move.
    BelowMinimum { minimum: u64 },
    /// Too many outputs for one transaction; the caller must split the deposit.
    TooManyOutputs { outputs: usize, max: usize },
}

impl core::fmt::Display for DepositError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BelowMinimum { minimum } => write!(
                f,
                "amount is below the smallest sendable denomination ({minimum} zatoshi)"
            ),
            Self::TooManyOutputs { outputs, max } => {
                write!(
                    f,
                    "{outputs} outputs exceeds the per-transaction limit of {max}"
                )
            }
        }
    }
}

impl std::error::Error for DepositError {}

/// A deposit broken into canonical pieces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepositPlan {
    /// Each output's value in zatoshi, all from `{1, 2, 5} × 10^k`, all ≤ [`DENOM_CAP_ZATS`].
    pub outputs: Vec<u64>,
    /// Left in the source, never sent: below [`MAX_RESIDUAL_ZATS`].
    pub residual: u64,
    /// Sum of `outputs`.
    pub total: u64,
}

impl DepositPlan {
    /// Number of QR frames this many outputs implies, for a one-action-per-output estimate.
    pub fn estimated_actions(&self) -> usize {
        self.outputs.len()
    }
}

/// Maximum output count in one deposit transaction. Beyond this the transaction gets expensive and
/// the QR envelope grows; the caller should make several deposits instead.
pub const MAX_OUTPUTS: usize = 16;

/// Split `amount` into canonical denominations.
///
/// Greedy from the largest allowed value downwards. Greedy is correct here because the set is
/// `{1, 2, 5} × 10^k`, which is `2`-superincreasing in practice: any value reachable with fewer
/// outputs is reached by the greedy choice first.
pub fn plan_deposit(amount: u64) -> Result<DepositPlan, DepositError> {
    if amount < MAX_RESIDUAL_ZATS {
        return Err(DepositError::BelowMinimum {
            minimum: MAX_RESIDUAL_ZATS,
        });
    }

    let mut remaining = amount;
    let mut outputs = Vec::new();
    while remaining >= MAX_RESIDUAL_ZATS {
        // Never pick a piece above the per-output cap; the zip318 helper alone would.
        let hi = remaining.min(DENOM_CAP_ZATS);
        let value = largest_one_two_five(hi, MAX_RESIDUAL_ZATS);
        if value == 0 {
            break;
        }
        outputs.push(value);
        remaining -= value;
        if outputs.len() > MAX_OUTPUTS {
            return Err(DepositError::TooManyOutputs {
                outputs: outputs.len(),
                max: MAX_OUTPUTS,
            });
        }
    }

    if outputs.is_empty() {
        return Err(DepositError::BelowMinimum {
            minimum: MAX_RESIDUAL_ZATS,
        });
    }

    let total = outputs.iter().sum();
    Ok(DepositPlan {
        outputs,
        residual: amount - total,
        total,
    })
}

/// Whether a value is one of the canonical denominations.
pub fn is_canonical(value: u64) -> bool {
    value >= MAX_RESIDUAL_ZATS
        && value <= DENOM_CAP_ZATS
        && largest_one_two_five(value, MAX_RESIDUAL_ZATS) == value
}

// ---------------------------------------------------------------------------
// Canary
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanaryPhase {
    /// No canary has been sent.
    NotStarted,
    /// A small deposit was made; waiting for it to appear in the vault.
    AwaitingArrival,
    /// The deposit arrived; waiting for the vault to actually spend it.
    AwaitingSpend,
    /// The vault proved it can spend. Bulk deposits are now allowed.
    Proven,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanaryError {
    /// A canary was already started.
    AlreadyStarted(CanaryPhase),
    /// The canary amount is not a small canonical denomination.
    BadAmount { amount: u64, max: u64 },
    /// The step does not follow from the current phase.
    WrongPhase {
        expected: CanaryPhase,
        actual: CanaryPhase,
    },
}

impl core::fmt::Display for CanaryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AlreadyStarted(p) => write!(f, "canary already started ({p:?})"),
            Self::BadAmount { amount, max } => write!(
                f,
                "canary amount {amount} must be a canonical denomination no larger than {max}"
            ),
            Self::WrongPhase { expected, actual } => {
                write!(f, "expected phase {expected:?}, currently {actual:?}")
            }
        }
    }
}

impl std::error::Error for CanaryError {}

/// The gate that keeps a vault from being funded before it has been shown to be spendable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canary {
    phase: CanaryPhase,
    amount: Option<u64>,
}

impl Default for Canary {
    fn default() -> Self {
        Self::new()
    }
}

impl Canary {
    pub fn new() -> Self {
        Self {
            phase: CanaryPhase::NotStarted,
            amount: None,
        }
    }

    pub fn phase(&self) -> CanaryPhase {
        self.phase
    }

    pub fn amount(&self) -> Option<u64> {
        self.amount
    }

    /// Register the first (small) deposit.
    pub fn begin(&mut self, amount: u64) -> Result<(), CanaryError> {
        if self.phase != CanaryPhase::NotStarted {
            return Err(CanaryError::AlreadyStarted(self.phase));
        }
        if !is_canonical(amount) || amount > CANARY_AMOUNT_ZATS {
            return Err(CanaryError::BadAmount {
                amount,
                max: CANARY_AMOUNT_ZATS,
            });
        }
        self.phase = CanaryPhase::AwaitingArrival;
        self.amount = Some(amount);
        Ok(())
    }

    /// The canary deposit appeared in the vault.
    pub fn arrival_confirmed(&mut self) -> Result<(), CanaryError> {
        self.expect(CanaryPhase::AwaitingArrival)?;
        self.phase = CanaryPhase::AwaitingSpend;
        Ok(())
    }

    /// The vault spent the canary through the threshold signature. Only now is it proven.
    pub fn spend_confirmed(&mut self) -> Result<(), CanaryError> {
        self.expect(CanaryPhase::AwaitingSpend)?;
        self.phase = CanaryPhase::Proven;
        Ok(())
    }

    /// Whether a deposit of `amount` may be initiated now.
    ///
    /// Only the canary itself is allowed before [`CanaryPhase::Proven`]. A bulk deposit attempted
    /// earlier is refused — not "warned about" — because the point of the exercise is to never put a
    /// balance somewhere it cannot come back from.
    pub fn allows(&self, amount: u64) -> bool {
        match self.phase {
            CanaryPhase::NotStarted => false,
            CanaryPhase::AwaitingArrival | CanaryPhase::AwaitingSpend => {
                Some(amount) == self.amount
            }
            CanaryPhase::Proven => amount >= MAX_RESIDUAL_ZATS,
        }
    }

    fn expect(&self, expected: CanaryPhase) -> Result<(), CanaryError> {
        if self.phase != expected {
            return Err(CanaryError::WrongPhase {
                expected,
                actual: self.phase,
            });
        }
        Ok(())
    }
}
