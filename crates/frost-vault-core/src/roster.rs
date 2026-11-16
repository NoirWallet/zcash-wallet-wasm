//! The roster manifest and its hash.
//!
//! Three implementations of this encoding would drift apart the first time someone typed a length
//! prefix in the wrong endianness, so it lives here — in the crate both the wasm module and the native
//! signer link — and nowhere else. The wire format below is fixed by agreement with the mobile client;
//! treat it as a protocol constant, not as an implementation detail.
//!
//! ```text
//! rosterHash = SHA-256(
//!     "noir-frost-vault/roster/v1"          // raw UTF-8, no length prefix
//!   || field(group_id)
//!   || field(protocol_version as text)
//!   || field(ciphersuite)
//!   || field("mainnet" | "testnet")
//!   || field(threshold as text)
//!   || field(participant_count as text)
//!   || for each participant, ascending by slot:
//!        field([slot; 1 byte])
//!     || field(identity_public_key)
//!     || field(transport_public_key)
//! )
//!
//! field(x) = u32 big-endian length || x
//! ```
//!
//! Numbers are decimal **text**, not binary. That is unusual but settled: the canonical form is
//! "no leading zeros, no sign, no whitespace", and [`RosterManifest`] is the only thing that writes it.

use std::collections::BTreeMap;

/// Domain tag. Concatenated raw, deliberately without a length prefix, so no reader can mistake it for
/// a field.
pub const ROSTER_DOMAIN: &[u8] = b"noir-frost-vault/roster/v1";

/// Longest field this crate will hash. Real fields are tens of bytes; the cap keeps a hostile length
/// from forcing a large allocation.
pub const MAX_FIELD: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RosterError {
    /// A field exceeded [`MAX_FIELD`].
    FieldTooLong { len: usize },
    /// Slot outside the roster.
    BadSlot(u8),
    /// A slot was added twice.
    DuplicateSlot(u8),
    /// More participants than the manifest declares.
    TooManyParticipants { declared: u16, added: usize },
    /// The hash was requested before every declared participant was added.
    Incomplete { declared: u16, added: usize },
    /// Threshold is zero, or larger than the declared participant count.
    BadThreshold { threshold: u16, participants: u16 },
    /// The manifest was constructed with a participant count of zero.
    EmptyRoster,
}

impl core::fmt::Display for RosterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FieldTooLong { len } => write!(f, "field of {len} bytes exceeds {MAX_FIELD}"),
            Self::BadSlot(slot) => write!(f, "slot {slot} is outside the roster"),
            Self::DuplicateSlot(slot) => write!(f, "slot {slot} appears twice"),
            Self::TooManyParticipants { declared, added } => {
                write!(f, "{added} participants added for a roster of {declared}")
            }
            Self::Incomplete { declared, added } => {
                write!(f, "{added} of {declared} participants present")
            }
            Self::BadThreshold { threshold, participants } => write!(f, "threshold {threshold} is not within 1..={participants}"),
            Self::EmptyRoster => f.write_str("a roster needs at least one participant"),
        }
    }
}

impl std::error::Error for RosterError {}

/// One device's entry in the roster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RosterEntry {
    /// Device identity public key (Ed25519), as raw bytes.
    pub identity_public_key: Vec<u8>,
    /// Transport public key. Under BLE this is the 32-byte Noise static key; under QR it is 32 zero
    /// bytes, per the agreed convention.
    pub transport_public_key: Vec<u8>,
}

/// A roster, ready to be hashed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RosterManifest {
    group_id: String,
    protocol_version: u32,
    ciphersuite: String,
    mainnet: bool,
    threshold: u16,
    participants: u16,
    entries: BTreeMap<u8, RosterEntry>,
}

fn field(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), RosterError> {
    if bytes.len() > MAX_FIELD {
        return Err(RosterError::FieldTooLong { len: bytes.len() });
    }
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

impl RosterManifest {
    pub fn new(group_id: &str, protocol_version: u32, ciphersuite: &str, mainnet: bool, threshold: u16, participants: u16) -> Result<Self, RosterError> {
        if participants == 0 {
            return Err(RosterError::EmptyRoster);
        }
        if threshold == 0 || threshold > participants {
            return Err(RosterError::BadThreshold { threshold, participants });
        }
        Ok(Self {
            group_id: group_id.to_string(),
            protocol_version,
            ciphersuite: ciphersuite.to_string(),
            mainnet,
            threshold,
            participants,
            entries: BTreeMap::new(),
        })
    }

    /// Add a participant. Order of calls does not matter: encoding sorts by slot.
    pub fn add_participant(&mut self, slot: u8, identity_public_key: &[u8], transport_public_key: &[u8]) -> Result<(), RosterError> {
        if slot as u16 >= self.participants {
            return Err(RosterError::BadSlot(slot));
        }
        if self.entries.contains_key(&slot) {
            return Err(RosterError::DuplicateSlot(slot));
        }
        if self.entries.len() >= self.participants as usize {
            return Err(RosterError::TooManyParticipants {
                declared: self.participants,
                added: self.entries.len() + 1,
            });
        }
        if identity_public_key.len() > MAX_FIELD || transport_public_key.len() > MAX_FIELD {
            return Err(RosterError::FieldTooLong {
                len: identity_public_key.len().max(transport_public_key.len()),
            });
        }
        self.entries.insert(
            slot,
            RosterEntry {
                identity_public_key: identity_public_key.to_vec(),
                transport_public_key: transport_public_key.to_vec(),
            },
        );
        Ok(())
    }

    /// The exact bytes that are hashed. Exposed for test vectors and debugging only.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, RosterError> {
        if self.entries.len() != self.participants as usize {
            return Err(RosterError::Incomplete {
                declared: self.participants,
                added: self.entries.len(),
            });
        }
        let mut out = Vec::with_capacity(128 + self.entries.len() * 128);
        out.extend_from_slice(ROSTER_DOMAIN);
        field(&mut out, self.group_id.as_bytes())?;
        field(&mut out, self.protocol_version.to_string().as_bytes())?;
        field(&mut out, self.ciphersuite.as_bytes())?;
        field(&mut out, if self.mainnet { b"mainnet" } else { b"testnet" })?;
        field(&mut out, self.threshold.to_string().as_bytes())?;
        field(&mut out, self.participants.to_string().as_bytes())?;
        for (slot, entry) in &self.entries {
            field(&mut out, &[*slot])?;
            field(&mut out, &entry.identity_public_key)?;
            field(&mut out, &entry.transport_public_key)?;
        }
        Ok(out)
    }

    /// `SHA-256(canonical_bytes)`.
    pub fn hash(&self) -> Result<[u8; 32], RosterError> {
        use sha2::{Digest, Sha256};
        let bytes = self.canonical_bytes()?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(hasher.finalize().into())
    }

    /// Lowercase hex of [`Self::hash`], for comparison with the agreed test vectors.
    pub fn hash_hex(&self) -> Result<String, RosterError> {
        let hash = self.hash()?;
        let mut out = String::with_capacity(64);
        for byte in hash {
            out.push_str(&format!("{byte:02x}"));
        }
        Ok(out)
    }
}
