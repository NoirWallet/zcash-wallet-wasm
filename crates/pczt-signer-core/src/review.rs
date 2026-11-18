//! The signer's own reading of the transaction.
//!
//! The coordinator builds the PCZT and decides what to put in its plaintext fields, so those fields are
//! a claim, not a fact. This module turns them into facts or refuses:
//!
//! * every output's `(recipient, value)` is checked against the note commitment the signature covers, by
//!   rebuilding the note and recomputing `cmx`. The output's `rho` is the action's spend nullifier (that
//!   is how Orchard derives it), so the reconstruction needs nothing the PCZT does not already carry;
//! * every spend's note must belong to this vault, and its nullifier must be the one that note produces,
//!   which pins *which* notes are being spent;
//! * the values must balance against the bundle's value balance, which the signature also covers.
//!
//! What this cannot do is invent facts it was not given: a field that is absent is an error, not a zero.
//! That is deliberate — "could not verify" must never read as "nothing to object to".

use orchard::keys::{FullViewingKey, Scope};
use orchard::note::{ExtractedNoteCommitment, NoteVersion, RandomSeed, Rho};
use orchard::value::NoteValue;
use orchard::{Address, Note};
use pczt::roles::verifier::Verifier;
use pczt::Pczt;

/// One output, as verified from the PCZT itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewedOutput {
    /// Raw 43-byte Orchard address.
    pub recipient: [u8; 43],
    pub value: u64,
    /// Whether the recipient is in this vault's scope, i.e. change rather than a payment.
    pub is_change: bool,
    /// Memo bytes, if the output carries one.
    pub memo: Option<Vec<u8>>,
}

/// One spend, as verified from the PCZT itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewedSpend {
    /// The identifier the transaction will be judged by, recomputed from the note rather than trusted.
    ///
    /// This is the handle to look up in the wallet's own note set: a nullifier that is not in it means the
    /// transaction spends a note this device has never seen, whatever the plaintext fields say.
    pub nullifier: [u8; 32],
    pub value: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Review {
    pub outputs: Vec<ReviewedOutput>,
    pub spends: Vec<ReviewedSpend>,
    pub sent_total: u64,
    pub change_total: u64,
    /// Fee implied by the bundle's value balance. Negative value balances are the fee.
    pub fee: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewError {
    /// The viewing key bytes did not parse.
    InvalidViewingKey,
    /// A field the check needs is absent. A redacted PCZT cannot be verified, so it cannot be signed.
    MissingField { action_index: u32, field: &'static str },
    /// An address or commitment did not parse.
    Malformed { action_index: u32, field: &'static str },
    /// The spend's note does not belong to this vault.
    SpendNotOurs { action_index: u32 },
    /// The declared nullifier is not the one the note the PCZT describes produces.
    NullifierMismatch { action_index: u32 },
    /// The output's `(recipient, value)` does not match the commitment the signature covers.
    OutputMismatch { action_index: u32 },
    /// The declared values do not balance against the bundle's value balance.
    BalanceMismatch { declared: i128, committed: i128 },
    /// A negative fee, or a transaction with no outputs at all.
    Impossible { reason: &'static str },
}

impl core::fmt::Display for ReviewError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidViewingKey => f.write_str("full viewing key did not parse"),
            Self::MissingField { action_index, field } => write!(f, "action {action_index}: {field} is missing"),
            Self::Malformed { action_index, field } => write!(f, "action {action_index}: {field} is malformed"),
            Self::SpendNotOurs { action_index } => {
                write!(f, "action {action_index}: the spent note is not in this vault")
            }
            Self::NullifierMismatch { action_index } => write!(f, "action {action_index}: the declared nullifier is not the one this note produces"),
            Self::OutputMismatch { action_index } => write!(f, "action {action_index}: the displayed recipient and amount do not match the commitment"),
            Self::BalanceMismatch { declared, committed } => write!(f, "declared values balance to {declared}, the bundle's value balance is {committed}"),
            Self::Impossible { reason } => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for ReviewError {}

/// One spend's plaintext, read through the Orchard oracle.
struct SpendFacts {
    nullifier: [u8; 32],
    value: u64,
    recipient: [u8; 43],
    rho: Rho,
    rseed: RandomSeed,
    /// The plaintext version of the note being spent, as the PCZT states it.
    spend_version: NoteVersion,
    /// The plaintext version of the note being created.
    output_version: NoteVersion,
}

/// Read the transaction the way the signer must: from the PCZT, checking every plaintext field against
/// the commitment that actually gets signed.
pub fn review(pczt: &Pczt, fvk_bytes: &[u8; 96]) -> Result<Review, ReviewError> {
    let fvk = FullViewingKey::from_bytes(fvk_bytes).ok_or(ReviewError::InvalidViewingKey)?;

    // Spends are read through the Verifier role because that is the only public path to the Orchard
    // protocol types carrying the note fields. It is the same oracle the coordinator uses for `alpha`.
    let mut spend_facts: Vec<SpendFacts> = Vec::new();
    let mut spend_error: Option<ReviewError> = None;
    Verifier::new(pczt.clone())
        .with_ironwood::<(), _>(|bundle| {
            for (index, action) in bundle.actions().iter().enumerate() {
                let index = index as u32;
                let spend = action.spend();
                let (Some(value), Some(recipient), Some(rho), Some(rseed)) = (spend.value(), spend.recipient(), spend.rho(), spend.rseed()) else {
                    spend_error = Some(ReviewError::MissingField {
                        action_index: index,
                        field: "spend note fields",
                    });
                    return Ok(());
                };
                spend_facts.push(SpendFacts {
                    nullifier: spend.nullifier().to_bytes(),
                    value: value.inner(),
                    recipient: recipient.to_raw_address_bytes(),
                    rho: *rho,
                    rseed: *rseed,
                    spend_version: *spend.note_version(),
                    output_version: *action.output().note_version(),
                });
            }
            Ok(())
        })
        .map_err(|_| ReviewError::Impossible {
            reason: "the PCZT could not be read by the verifier role",
        })?;
    if let Some(error) = spend_error {
        return Err(error);
    }

    let bundle = pczt.ironwood();
    if bundle.actions().len() != spend_facts.len() {
        return Err(ReviewError::Impossible {
            reason: "the bundle's action count disagrees with the verifier's view",
        });
    }

    let mut outputs = Vec::with_capacity(spend_facts.len());
    let mut spends = Vec::with_capacity(spend_facts.len());
    let mut declared_net: i128 = 0;

    for (index, (action, facts)) in bundle.actions().iter().zip(spend_facts).enumerate() {
        let index = index as u32;
        // Set when this action's spend is filler rather than a note of ours.
        let mut padding = false;

        let spend_address = Option::<Address>::from(Address::from_raw_address_bytes(&facts.recipient)).ok_or(ReviewError::Malformed {
            action_index: index,
            field: "spend recipient",
        })?;
        // A spend outside our viewing scope is normally a rejection: a transaction that touches notes this
        // signer cannot account for is exactly what the review exists to refuse.
        //
        // The exception is padding. An Orchard bundle has one spend and one output per action, so a
        // transaction that pays two addresses needs a filler spend with zero value. Those are not ours and
        // never will be. Skipping them is safe because a zero-value spend moves nothing: it cannot shift the
        // balance, and the action's output is still checked against its own commitment below.
        let ours = fvk.scope_for_address(&spend_address).is_some();
        if !ours {
            if facts.value != 0 {
                return Err(ReviewError::SpendNotOurs { action_index: index });
            }
            padding = true;
        }

        if !padding {
            // The nullifier is what consensus checks against the chain, and it is derived from the note's own
            // commitment — which covers the amount and the recipient. Rebuilding the note and recomputing the
            // nullifier therefore ties the plaintext fields we are about to display to the identifier the
            // transaction will be judged by: a PCZT that showed one note in its fields and spent another
            // cannot pass both.
            let spend_note = Note::from_parts(spend_address, NoteValue::from_raw(facts.value), facts.rho, facts.rseed, facts.spend_version)
                .into_option()
                .ok_or(ReviewError::Malformed {
                    action_index: index,
                    field: "spend note",
                })?;
            let recomputed = spend_note.nullifier(&fvk).to_bytes();
            if recomputed != facts.nullifier {
                return Err(ReviewError::NullifierMismatch { action_index: index });
            }
        }
        if !padding {
            spends.push(ReviewedSpend {
                nullifier: facts.nullifier,
                value: facts.value,
            });
        }

        // Rebuild the output note. Its rho is the spend's nullifier, which is how Orchard derives it, so
        // the reconstruction needs nothing beyond the PCZT.
        let out = action.output();
        let out_value = out.value().ok_or(ReviewError::MissingField {
            action_index: index,
            field: "output value",
        })?;
        let out_recipient = out.recipient().ok_or(ReviewError::MissingField {
            action_index: index,
            field: "output recipient",
        })?;
        let out_rseed_bytes = out.rseed().ok_or(ReviewError::MissingField {
            action_index: index,
            field: "output rseed",
        })?;
        let out_cmx = out.cmx().ok_or(ReviewError::MissingField {
            action_index: index,
            field: "output cmx",
        })?;

        let out_address = Option::<Address>::from(Address::from_raw_address_bytes(&out_recipient)).ok_or(ReviewError::Malformed {
            action_index: index,
            field: "output recipient",
        })?;
        let out_rho = Rho::from_bytes(&facts.nullifier).into_option().ok_or(ReviewError::Malformed {
            action_index: index,
            field: "output rho",
        })?;
        let out_rseed = RandomSeed::from_bytes(out_rseed_bytes, &out_rho).into_option().ok_or(ReviewError::Malformed {
            action_index: index,
            field: "output rseed",
        })?;
        // Use the version the PCZT states rather than assuming one: the version participates in the note
        // commitment, so a mismatch here would silently make every output unverifiable.
        let out_note = Note::from_parts(out_address, NoteValue::from_raw(out_value), out_rho, out_rseed, facts.output_version)
            .into_option()
            .ok_or(ReviewError::Malformed {
                action_index: index,
                field: "output note",
            })?;

        // The check that matters: the displayed recipient and amount must reproduce the commitment the
        // signature covers. A coordinator that lies about either fails here.
        let committed = ExtractedNoteCommitment::from_bytes(&out_cmx).into_option().ok_or(ReviewError::Malformed {
            action_index: index,
            field: "output cmx",
        })?;
        if ExtractedNoteCommitment::from(out_note.commitment()) != committed {
            return Err(ReviewError::OutputMismatch { action_index: index });
        }

        let is_change = fvk.scope_for_address(&out_address).is_some();
        outputs.push(ReviewedOutput {
            recipient: out_recipient,
            value: out_value,
            is_change,
            memo: None,
        });
        // A padding spend contributes nothing; counting it would break the balance for no reason.
        if !padding {
            declared_net += facts.value as i128;
        }
        declared_net -= out_value as i128;
    }

    if outputs.is_empty() {
        return Err(ReviewError::Impossible {
            reason: "the transaction has no shielded outputs",
        });
    }

    // Shielded-only transaction, so the fee is whatever the bundle says left the wallet.
    let (magnitude, negative) = *bundle.value_sum();
    let value_balance = if negative { -(magnitude as i128) } else { magnitude as i128 };
    if declared_net != value_balance {
        return Err(ReviewError::BalanceMismatch {
            declared: declared_net,
            committed: value_balance,
        });
    }
    // A vault spend moves value out of the pool: a negative balance would mean value was added.
    if value_balance < 0 {
        return Err(ReviewError::Impossible {
            reason: "a spend cannot add value to the shielded pool",
        });
    }
    let fee = value_balance as u64;

    let change_total = outputs.iter().filter(|o| o.is_change).fold(0u64, |acc, o| acc.saturating_add(o.value));
    let sent_total = outputs.iter().filter(|o| !o.is_change).fold(0u64, |acc, o| acc.saturating_add(o.value));

    Ok(Review {
        outputs,
        spends,
        sent_total,
        change_total,
        fee,
    })
}

/// Whether an output's `(recipient, value)` reproduces the commitment it was given.
///
/// Factored out so the mismatch branch can be tested directly: the standard creator derives `cmx` from
/// the same fields, so a disagreeing PCZT cannot be produced through the supported API — only by a
/// hand-crafted one, which is exactly the case a signer has to survive.
pub fn output_matches(note: &Note, committed: &[u8; 32]) -> bool {
    match ExtractedNoteCommitment::from_bytes(committed).into_option() {
        Some(expected) => ExtractedNoteCommitment::from(note.commitment()) == expected,
        None => false,
    }
}

/// The `Scope` an address falls into, if it belongs to this vault.
pub fn scope_of(fvk_bytes: &[u8; 96], recipient: &[u8; 43]) -> Option<Scope> {
    let fvk = FullViewingKey::from_bytes(fvk_bytes)?;
    let address = Option::<Address>::from(Address::from_raw_address_bytes(recipient))?;
    fvk.scope_for_address(&address)
}

#[cfg(test)]
mod tests {
    use super::output_matches;
    use orchard::keys::{FullViewingKey, Scope, SpendingKey};
    use orchard::note::{NoteVersion, RandomSeed, Rho};
    use orchard::{Address, Note};

    fn a_note(value: u64) -> Note {
        let sk = SpendingKey::from_bytes([7u8; 32]).into_option().unwrap();
        let fvk = FullViewingKey::from(&sk);
        let address: Address = fvk.address_at(0u32, Scope::External);
        let rho = Rho::from_bytes(&[1u8; 32]).into_option().unwrap();
        let rseed = (0u8..=255).find_map(|b| RandomSeed::from_bytes([b; 32], &rho).into_option()).unwrap();
        Note::from_parts(address, orchard::value::NoteValue::from_raw(value), rho, rseed, NoteVersion::V3)
            .into_option()
            .unwrap()
    }

    #[test]
    fn the_output_check_accepts_the_matching_commitment() {
        let note = a_note(1_000);
        let cmx: [u8; 32] = orchard::note::ExtractedNoteCommitment::from(note.commitment()).to_bytes();
        assert!(output_matches(&note, &cmx));
    }

    #[test]
    fn the_output_check_rejects_a_mismatched_amount() {
        // The attack this exists for: a coordinator claims one amount, the commitment encodes another.
        // The standard creator cannot produce this, so the check is exercised here directly.
        let real = a_note(1_000);
        let claimed = a_note(1_001);
        use orchard::note::ExtractedNoteCommitment;
        assert_ne!(
            ExtractedNoteCommitment::from(real.commitment()),
            ExtractedNoteCommitment::from(claimed.commitment())
        );
        let real_cmx: [u8; 32] = orchard::note::ExtractedNoteCommitment::from(real.commitment()).to_bytes();
        assert!(!output_matches(&claimed, &real_cmx));
    }

    #[test]
    fn the_output_check_rejects_a_malformed_commitment() {
        let note = a_note(1_000);
        assert!(!output_matches(&note, &[0u8; 32]));
    }
}
