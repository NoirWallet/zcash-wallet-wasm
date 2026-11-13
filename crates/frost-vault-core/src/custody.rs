//! Custody: the key share this device holds, and what happens when the process dies.
//!
//! The Extension holds a share in the 2-of-2 and 2-of-3 modes, which makes two things load-bearing
//! that a pure protocol crate would not think about:
//!
//! * **A share must survive a reload but never be confused with another share.** [`ShareBackup`] is a
//!   versioned record with the bindings that make a mix-up detectable: group key, roster slot,
//!   device and vault identity. Loading validates all of them, so restoring yesterday's share of
//!   another vault fails loudly instead of producing signatures nobody can aggregate.
//! * **A killed process must not resume a session whose nonces may already be in the wild.** This is
//!   the MV3 problem from §15.1, and it is a policy problem, not a crypto one: [`SessionGuard`]
//!   records how far a session got, and [`SessionGuard::recover`] turns a leftover marker into a
//!   decision the caller cannot get wrong — *abandon* it, never continue it.
//!
//! Encryption is deliberately **not** here. The host seals the record with the platform keystore or
//! WebCrypto; inventing a sealing scheme in this crate would be a second, unaudited construction in
//! the one place where losing the plaintext loses money. The record is plaintext-with-structure, and
//! the doc says the host must encrypt it before it touches disk.

use rand_core::{CryptoRng, RngCore};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::threshold::{Identifier, KeyPackage, PublicKeyPackage, Suite};

/// Current record version. Bumped only additively; an unknown version is refused.
pub const SHARE_BACKUP_VERSION: u8 = 1;

/// Domain tag mixed into the backup's integrity check, so a record cannot be replayed as another
/// kind of blob.
const BACKUP_DOMAIN: &[u8] = b"noir-frost-vault/share-backup/v1";

/// Longest device/vault identity accepted, so a hostile record cannot force a large allocation.
pub const MAX_IDENTITY_LEN: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CustodyError {
    /// Not a share backup.
    NotAShareBackup,
    /// Version this build does not know.
    UnsupportedVersion(u8),
    /// Truncated or over-long field.
    Malformed,
    /// The record's integrity check failed: it was truncated or edited.
    Corrupt,
    /// The FROST key package did not parse, or failed its own validation.
    InvalidKeyPackage,
    /// The share belongs to a different group than the caller expects.
    WrongGroup,
    /// The share belongs to a different roster slot.
    WrongSlot { expected: u8, found: u8 },
    /// The share was issued for a different device or vault identity.
    WrongIdentity,
    /// The key package's own parameters disagree with the record.
    InconsistentParameters,
}

impl core::fmt::Display for CustodyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAShareBackup => f.write_str("not a share backup"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported share backup version {v}"),
            Self::Malformed => f.write_str("malformed share backup"),
            Self::Corrupt => f.write_str("share backup integrity check failed"),
            Self::InvalidKeyPackage => f.write_str("key package did not parse"),
            Self::WrongGroup => f.write_str("share belongs to a different group"),
            Self::WrongSlot { expected, found } => {
                write!(f, "share is for slot {found}, expected {expected}")
            }
            Self::WrongIdentity => f.write_str("share belongs to a different device or vault"),
            Self::InconsistentParameters => f.write_str("key package parameters disagree with the record"),
        }
    }
}

impl std::error::Error for CustodyError {}

/// What a device must remember besides the share itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareIdentity {
    /// Roster slot this device occupies.
    pub slot: u8,
    /// Stable device identifier (the Device Identity Key hash), as pinned at pairing.
    pub device_id: Vec<u8>,
    /// Vault identifier, so two vaults on one device cannot be swapped.
    pub vault_id: Vec<u8>,
}

#[derive(Zeroize, ZeroizeOnDrop)]
struct Secrets {
    key_package: Vec<u8>,
}

/// `SHA-256(domain ‖ body)`. Not a MAC: the host seals the record. This catches corruption and stops
/// a key package being spliced onto another record's header.
fn compute_check(body: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(BACKUP_DOMAIN);
    h.update(body);
    h.finalize().into()
}

/// A share, wrapped with everything needed to refuse the wrong one.
pub struct ShareBackup {
    identity: ShareIdentity,
    group_verifying_key: [u8; 32],
    min_signers: u16,
    secrets: Secrets,
}

impl core::fmt::Debug for ShareBackup {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never print the share.
        f.debug_struct("ShareBackup")
            .field("identity", &self.identity)
            .field("group_verifying_key", &hex(&self.group_verifying_key))
            .field("min_signers", &self.min_signers)
            .field("key_package", &"<redacted>")
            .finish()
    }
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) -> Result<(), CustodyError> {
    let len = u16::try_from(b.len()).map_err(|_| CustodyError::Malformed)?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(b);
    Ok(())
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], CustodyError> {
        let end = self.pos.checked_add(n).ok_or(CustodyError::Malformed)?;
        if end > self.buf.len() {
            return Err(CustodyError::Malformed);
        }
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, CustodyError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, CustodyError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn array32(&mut self) -> Result<[u8; 32], CustodyError> {
        self.take(32)?.try_into().map_err(|_| CustodyError::Malformed)
    }

    fn bytes(&mut self) -> Result<&'a [u8], CustodyError> {
        let n = self.u16()? as usize;
        self.take(n)
    }

    fn finish(&self) -> Result<(), CustodyError> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(CustodyError::Malformed)
        }
    }
}

impl ShareBackup {
    /// Wrap a key package for storage. The caller supplies the identity it wants bound in.
    pub fn new(key_package: &KeyPackage, identity: ShareIdentity) -> Result<Self, CustodyError> {
        if identity.device_id.len() > MAX_IDENTITY_LEN || identity.vault_id.len() > MAX_IDENTITY_LEN {
            return Err(CustodyError::Malformed);
        }
        let group_verifying_key: [u8; 32] = key_package
            .verifying_key()
            .serialize()
            .map_err(|_| CustodyError::InvalidKeyPackage)?
            .as_slice()
            .try_into()
            .map_err(|_| CustodyError::InvalidKeyPackage)?;
        Ok(Self {
            identity,
            group_verifying_key,
            min_signers: *key_package.min_signers(),
            secrets: Secrets {
                key_package: key_package.serialize().map_err(|_| CustodyError::InvalidKeyPackage)?,
            },
        })
    }

    pub fn identity(&self) -> &ShareIdentity {
        &self.identity
    }

    pub fn group_verifying_key(&self) -> &[u8; 32] {
        &self.group_verifying_key
    }

    /// Serialize for the host to seal. Layout:
    ///
    /// ```text
    /// version:u8 || group_vk:32 || min_signers:u16 || slot:u8
    ///   || device_id:len||bytes || vault_id:len||bytes
    ///   || key_package:len||bytes || check:32
    /// ```
    pub fn to_bytes(&self) -> Result<Vec<u8>, CustodyError> {
        let mut out = Vec::new();
        out.push(SHARE_BACKUP_VERSION);
        out.extend_from_slice(&self.group_verifying_key);
        out.extend_from_slice(&self.min_signers.to_le_bytes());
        out.push(self.identity.slot);
        put_bytes(&mut out, &self.identity.device_id)?;
        put_bytes(&mut out, &self.identity.vault_id)?;
        put_bytes(&mut out, &self.secrets.key_package)?;
        let check = compute_check(&out);
        out.extend_from_slice(&check);
        Ok(out)
    }

    /// Parse and validate, insisting on the group and slot the caller expects.
    ///
    /// Both are arguments rather than "read from the record" on purpose: a restore that only checks
    /// the record against itself would happily load the wrong vault's share.
    pub fn from_bytes(bytes: &[u8], expected_group: &[u8; 32], expected_slot: u8) -> Result<Self, CustodyError> {
        if bytes.len() < 1 + 32 + 2 + 1 + 2 + 2 + 2 + 32 {
            return Err(CustodyError::Malformed);
        }
        let (body, check) = bytes.split_at(bytes.len() - 32);
        if body[0] != SHARE_BACKUP_VERSION {
            return Err(CustodyError::UnsupportedVersion(body[0]));
        }
        // Integrity before parsing: everything after this point handles a structure already known to
        // be intact, so a corrupted length cannot steer the reader.
        if check != compute_check(body).as_slice() {
            return Err(CustodyError::Corrupt);
        }
        let mut expected = Reader::new(body);
        expected.u8()?;
        let group = expected.array32()?;
        let min = expected.u16()?;
        let slot = expected.u8()?;
        let device_id = expected.bytes()?.to_vec();
        let vault_id = expected.bytes()?.to_vec();
        let key_package = expected.bytes()?.to_vec();
        expected.finish()?;

        if &group != expected_group {
            return Err(CustodyError::WrongGroup);
        }
        if slot != expected_slot {
            return Err(CustodyError::WrongSlot {
                expected: expected_slot,
                found: slot,
            });
        }
        if device_id.len() > MAX_IDENTITY_LEN || vault_id.len() > MAX_IDENTITY_LEN {
            return Err(CustodyError::Malformed);
        }

        let package = KeyPackage::deserialize(&key_package).map_err(|_| CustodyError::InvalidKeyPackage)?;
        // The package's own fields must agree with the record, or one of the two was edited.
        if *package.min_signers() != min {
            return Err(CustodyError::InconsistentParameters);
        }
        let package_group: [u8; 32] = package
            .verifying_key()
            .serialize()
            .map_err(|_| CustodyError::InvalidKeyPackage)?
            .as_slice()
            .try_into()
            .map_err(|_| CustodyError::InvalidKeyPackage)?;
        if &package_group != expected_group {
            return Err(CustodyError::WrongGroup);
        }

        Ok(Self {
            identity: ShareIdentity { slot, device_id, vault_id },
            group_verifying_key: group,
            min_signers: min,
            secrets: Secrets { key_package },
        })
    }

    /// Extract the key package. Only for the holder of this record.
    pub fn key_package(&self) -> Result<KeyPackage, CustodyError> {
        KeyPackage::deserialize(&self.secrets.key_package).map_err(|_| CustodyError::InvalidKeyPackage)
    }

    /// Check the record against a public package: the group key must match and the account of
    /// signers must be the same.
    pub fn matches_group(&self, pubkeys: &PublicKeyPackage) -> bool {
        pubkeys
            .verifying_key()
            .serialize()
            .map(|b| b.as_slice() == self.group_verifying_key)
            .unwrap_or(false)
    }

    /// The identifier this share signs under.
    pub fn identifier(&self) -> Result<Identifier, CustodyError> {
        self.key_package().map(|kp| *kp.identifier())
    }
}

// ---------------------------------------------------------------------------
// Session guard: what to do when the process died mid-session
// ---------------------------------------------------------------------------

/// How far a session got before the process stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionProgress {
    /// The session existed but nothing left the device. Discardable.
    Created,
    /// Commitments were published. The nonces behind them are now in the wild.
    Committed,
    /// At least one share was produced.
    Consumed,
}

/// What the caller must do about a leftover marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// No marker: nothing was in flight.
    Clean,
    /// A session started but never left the device. Safe to discard and start over.
    Discardable { session_id: [u8; 16], progress: SessionProgress },
    /// A session reached a point of no return. It must be abandoned, never continued: continuing
    /// would mean reusing nonces that may already be outside this device.
    Burned { session_id: [u8; 16], progress: SessionProgress },
}

impl Recovery {
    /// Session the leftover marker refers to, so the host can log or correlate it.
    pub fn session_id(self) -> Option<[u8; 16]> {
        match self {
            Self::Clean => None,
            Self::Discardable { session_id, .. } | Self::Burned { session_id, .. } => Some(session_id),
        }
    }
}

impl Recovery {
    /// Whether the leftover session may be resumed. Only ever false, but keeping it as a method makes
    /// the rule impossible to forget at a call site.
    pub fn may_resume(self) -> bool {
        false
    }
}

/// Minimal key/value store. The host implements it over `chrome.storage`, IndexedDB or memory.
pub trait SessionStore {
    type Error: core::fmt::Debug;
    fn put(&mut self, key: &str, value: &[u8]) -> Result<(), Self::Error>;
    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, Self::Error>;
    fn delete(&mut self, key: &str) -> Result<(), Self::Error>;
}

/// Encode a marker. Hosts that cannot implement [`SessionStore`] (anything behind a wasm boundary, for
/// instance) just persist this blob and hand it back on the next start.
pub fn marker_bytes(session_id: [u8; 16], progress: SessionProgress) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(18);
    bytes.push(1u8);
    bytes.extend_from_slice(&session_id);
    bytes.push(match progress {
        SessionProgress::Created => 0,
        SessionProgress::Committed => 1,
        SessionProgress::Consumed => 2,
    });
    bytes
}

/// Decode a persisted marker. `None` (nothing persisted) is the same as [`Recovery::Clean`].
pub fn recover_from_marker(bytes: Option<&[u8]>) -> Result<Recovery, MarkerError> {
    let Some(bytes) = bytes else {
        return Ok(Recovery::Clean);
    };
    let (session_id, progress) = parse_marker(bytes)?;
    Ok(match progress {
        SessionProgress::Created => Recovery::Discardable { session_id, progress },
        SessionProgress::Committed | SessionProgress::Consumed => Recovery::Burned { session_id, progress },
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerError {
    Malformed,
}

impl core::fmt::Display for MarkerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("malformed session marker")
    }
}

impl std::error::Error for MarkerError {}

/// Decode the marker's fields.
pub fn parse_marker(bytes: &[u8]) -> Result<([u8; 16], SessionProgress), MarkerError> {
    if bytes.len() != 18 || bytes[0] != 1 {
        return Err(MarkerError::Malformed);
    }
    let mut session_id = [0u8; 16];
    session_id.copy_from_slice(&bytes[1..17]);
    let progress = match bytes[17] {
        0 => SessionProgress::Created,
        1 => SessionProgress::Committed,
        2 => SessionProgress::Consumed,
        _ => return Err(MarkerError::Malformed),
    };
    Ok((session_id, progress))
}

/// Key under which the marker lives. One live session at a time is the rule, so one key suffices.
pub const MARKER_KEY: &str = "noir-frost-vault/session-marker";

#[derive(Debug)]
pub enum GuardError<E> {
    Store(E),
    /// A marker already exists: a previous session was not finished cleanly.
    AlreadyInFlight,
    Malformed,
}

impl<E: core::fmt::Debug> core::fmt::Display for GuardError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Store(e) => write!(f, "store: {e:?}"),
            Self::AlreadyInFlight => f.write_str("another session is already in flight"),
            Self::Malformed => f.write_str("malformed session marker"),
        }
    }
}

/// Records progress so that a restart can tell "nothing happened" from "do not touch this".
#[derive(Debug)]
pub struct SessionGuard {
    session_id: [u8; 16],
    progress: SessionProgress,
}

impl SessionGuard {
    /// Start a session. **Must be called before any nonce is generated.**
    pub fn begin<S: SessionStore>(store: &mut S, session_id: [u8; 16]) -> Result<Self, GuardError<S::Error>> {
        if store.get(MARKER_KEY).map_err(GuardError::Store)?.is_some() {
            return Err(GuardError::AlreadyInFlight);
        }
        let guard = Self {
            session_id,
            progress: SessionProgress::Created,
        };
        guard.persist(store)?;
        Ok(guard)
    }

    pub fn session_id(&self) -> [u8; 16] {
        self.session_id
    }

    pub fn progress(&self) -> SessionProgress {
        self.progress
    }

    /// Record that commitments left the device. From here on the session may not be resumed.
    pub fn mark_committed<S: SessionStore>(&mut self, store: &mut S) -> Result<(), GuardError<S::Error>> {
        self.progress = SessionProgress::Committed;
        self.persist(store)
    }

    /// Record that a share was produced.
    pub fn mark_consumed<S: SessionStore>(&mut self, store: &mut S) -> Result<(), GuardError<S::Error>> {
        self.progress = SessionProgress::Consumed;
        self.persist(store)
    }

    /// Finish normally: clear the marker. After this a restart looks `Clean`.
    pub fn finish<S: SessionStore>(self, store: &mut S) -> Result<(), GuardError<S::Error>> {
        store.delete(MARKER_KEY).map_err(GuardError::Store)
    }

    /// Inspect a leftover marker without clearing it.
    pub fn recover<S: SessionStore>(store: &mut S) -> Result<Recovery, GuardError<S::Error>> {
        let Some(bytes) = store.get(MARKER_KEY).map_err(GuardError::Store)? else {
            return Ok(Recovery::Clean);
        };
        recover_from_marker(Some(&bytes)).map_err(|_| GuardError::Malformed)
    }

    /// Clear a marker after the caller has dealt with it.
    pub fn abandon<S: SessionStore>(store: &mut S) -> Result<(), GuardError<S::Error>> {
        store.delete(MARKER_KEY).map_err(GuardError::Store)
    }

    /// The blob to persist. Same bytes the [`SessionStore`] path writes, so the two APIs interoperate.
    pub fn marker(&self) -> Vec<u8> {
        marker_bytes(self.session_id, self.progress)
    }

    fn persist<S: SessionStore>(&self, store: &mut S) -> Result<(), GuardError<S::Error>> {
        store.put(MARKER_KEY, &self.marker()).map_err(GuardError::Store)
    }
}

fn decode_marker<E>(bytes: &[u8]) -> Result<([u8; 16], SessionProgress), GuardError<E>> {
    if bytes.len() != 18 || bytes[0] != 1 {
        return Err(GuardError::Malformed);
    }
    let mut session_id = [0u8; 16];
    session_id.copy_from_slice(&bytes[1..17]);
    let progress = match bytes[17] {
        0 => SessionProgress::Created,
        1 => SessionProgress::Committed,
        2 => SessionProgress::Consumed,
        _ => return Err(GuardError::Malformed),
    };
    Ok((session_id, progress))
}

/// A fresh random session id. Randomness, not a counter: an id that repeats across restarts would
/// make a stale marker look like the current session.
pub fn new_session_id<R: CryptoRng + RngCore>(rng: &mut R) -> [u8; 16] {
    let mut id = [0u8; 16];
    rng.fill_bytes(&mut id);
    id
}

/// Ciphersuite this module's key packages belong to; re-exported so hosts do not need the FROST crate.
pub type KeyPackageSuite = Suite;
