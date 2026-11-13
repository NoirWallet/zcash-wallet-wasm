//! `KEYSET_SETUP`: turning a FROST group key into a usable Zcash wallet.
//!
//! FROST's DKG only solves spend authorization. A wallet also needs `nk`, `rivk`, a full viewing
//! key, an address and a fingerprint, and every device must arrive at byte-identical results without
//! any of them being able to spend on their own.
//!
//! The construction, in one line: `ak` from the DKG is spliced over the first 32 bytes of the
//! standard `ak ‖ nk ‖ rivk` encoding, where `nk` and `rivk` come from an unrelated random
//! `SpendingKey`. That key never signs — only `ak` authorizes spends — so holding it does not weaken
//! the threshold. It is the natural candidate for the ZIP-2005 `qsk` backup discussed in the review
//! notes, and is exported separately by [`QuantumRecoveryMaterial`].
//!
//! Cross-device agreement is the whole point, so the only inputs are `(ak, qsk, network,
//! diversifier_index)` and everything else is derived. `qsk` must therefore be generated once and
//! distributed; deriving it locally on each device would give each device a different address.

use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use zcash_address::unified::{Address as UnifiedAddress, Encoding, Fvk, Receiver, Revision, Ufvk, Uitem};
use zcash_protocol::consensus::{Network, Parameters};

/// Domain tag for the keyset fingerprint, so it can never be confused with another hash in the
/// protocol.
const FINGERPRINT_DOMAIN: &[u8] = b"noir-frost-vault/keyset-fingerprint/v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeysetError {
    /// `qsk` is not a valid spending key (wrong length, or not a canonical scalar).
    InvalidRecoveryKey,
    /// The spliced encoding did not pass `FullViewingKey::from_bytes` validation, which rejects the
    /// identity and re-derives `ivk` to reject the zero and ⊥ cases.
    InvalidFullViewingKey,
    /// Building the unified encoding failed.
    Encoding,
}

impl core::fmt::Display for KeysetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidRecoveryKey => f.write_str("qsk is not a valid spending key"),
            Self::InvalidFullViewingKey => f.write_str("ak ‖ nk ‖ rivk did not validate as a full viewing key"),
            Self::Encoding => f.write_str("unified encoding failed"),
        }
    }
}

impl std::error::Error for KeysetError {}

/// The material a vault must be able to back up given that FROST addresses are not quantum
/// recoverable: the `nk`/`rivk` source, together with the group key it belongs to.
///
/// It cannot spend. Losing it costs the ability to re-derive the viewing key and rescan history; it
/// does not lose funds, and holding it does not let anyone produce a spend authorization signature.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct QuantumRecoveryMaterial {
    pub group_verifying_key: [u8; 32],
    pub qsk: [u8; 32],
}

impl QuantumRecoveryMaterial {
    /// Serialize for backup: `ak ‖ qsk`, 64 bytes.
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&self.group_verifying_key);
        out[32..].copy_from_slice(&self.qsk);
        out
    }

    pub fn from_bytes(bytes: &[u8; 64]) -> Self {
        let mut group_verifying_key = [0u8; 32];
        group_verifying_key.copy_from_slice(&bytes[..32]);
        let mut qsk = [0u8; 32];
        qsk.copy_from_slice(&bytes[32..]);
        Self { group_verifying_key, qsk }
    }
}

impl core::fmt::Debug for QuantumRecoveryMaterial {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never print the recovery key.
        f.debug_struct("QuantumRecoveryMaterial")
            .field("group_verifying_key", &hex32(&self.group_verifying_key))
            .field("qsk", &"<redacted>")
            .finish()
    }
}

fn hex32(b: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

/// The derived keyset. Every device must compute exactly these bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultKeyset {
    /// The FROST group verifying key, `ak`.
    pub group_verifying_key: [u8; 32],
    /// The Orchard-protocol full viewing key, `ak ‖ nk ‖ rivk`, 96 bytes.
    pub fvk: [u8; 96],
    /// `uview1…`: an Orchard-only UFVK. Carries no transparent or Sapling component by construction.
    pub ufvk: String,
    /// `u1…`: the vault's receiving address.
    pub address: String,
    /// Raw 43-byte Orchard address, for callers that need to compare rather than parse.
    pub raw_address: [u8; 43],
    pub diversifier_index: u32,
    /// `H(domain ‖ network ‖ fvk)`. Devices compare this out of band (§5.9), so it must be
    /// derivable without any secret.
    pub fingerprint: [u8; 32],
}

impl VaultKeyset {
    /// Derive the keyset from `ak` and `qsk`. Deterministic in every input.
    pub fn derive(group_verifying_key: &[u8; 32], qsk: &[u8; 32], network: Network, diversifier_index: u32) -> Result<Self, KeysetError> {
        let fvk = splice_fvk(group_verifying_key, qsk)?;
        let orchard_address = fvk.address_at(diversifier_index, Scope::External);
        let raw_address = orchard_address.to_raw_address_bytes();

        // Built from the Orchard item alone so that no transparent receiver can appear: the vault
        // cannot spend one, and a transparent receiver would let someone send funds that are
        // unrecoverable forever. Revision 0 is the shielded-only form and needs no feature flags to
        // express.
        let ua = UnifiedAddress::try_from_items(Revision::R0, vec![Uitem::Data(Receiver::Orchard(raw_address))]).map_err(|_| KeysetError::Encoding)?;
        let fvk_bytes = fvk.to_bytes();
        let ufvk = Ufvk::try_from_items(Revision::R0, vec![Uitem::Data(Fvk::Orchard(fvk_bytes))]).map_err(|_| KeysetError::InvalidFullViewingKey)?;

        Ok(Self {
            group_verifying_key: *group_verifying_key,
            fvk: fvk_bytes,
            ufvk: ufvk.encode(&network.network_type()),
            address: ua.encode(&network.network_type()),
            raw_address,
            diversifier_index,
            fingerprint: fingerprint_of(network, &fvk_bytes),
        })
    }

    /// The address at another diversifier index. The UFVK and fingerprint do not change.
    pub fn address_at(&self, qsk: &[u8; 32], network: Network, diversifier_index: u32) -> Result<String, KeysetError> {
        let fvk = splice_fvk(&self.group_verifying_key, qsk)?;
        let raw = fvk.address_at(diversifier_index, Scope::External).to_raw_address_bytes();
        let ua = UnifiedAddress::try_from_items(Revision::R0, vec![Uitem::Data(Receiver::Orchard(raw))]).map_err(|_| KeysetError::Encoding)?;
        Ok(ua.encode(&network.network_type()))
    }

    /// Whether this address carries only an Orchard receiver — the invariant that keeps the vault
    /// from holding funds it cannot spend. Re-decodes the string rather than trusting the derivation.
    pub fn has_only_orchard_receiver(&self) -> bool {
        // Decode and ask the pool question, rather than sniffing the string: the bech32 data part can
        // contain "t1" by chance, so a substring test would be both wrong and confusing.
        match UnifiedAddress::decode(&self.address) {
            Ok((_, _, ua)) => {
                ua.has_receiver_of_type(zcash_protocol::PoolType::ORCHARD)
                    && !ua.has_receiver_of_type(zcash_protocol::PoolType::TRANSPARENT)
                    && !ua.has_receiver_of_type(zcash_protocol::PoolType::SAPLING)
            }
            Err(_) => false,
        }
    }

    /// `uview1…`-style UFVK for callers that only have the string.
    pub fn ufvk_orchard_component(&self) -> &[u8; 96] {
        &self.fvk
    }
}

/// The 96-byte FVK inside a UFVK string.
///
/// A recovering device receives a viewing key but not the `qsk` it was derived from, so it cannot use
/// [`VaultKeyset::derive`] to check anything. This reads the key back out.
pub fn fvk_from_ufvk(ufvk: &str) -> Result<[u8; 96], KeysetError> {
    use zcash_address::unified::Container;
    let (_, _, decoded) = Ufvk::decode(ufvk).map_err(|_| KeysetError::Encoding)?;
    decoded
        .items()
        .into_iter()
        .find_map(|item| match item {
            Fvk::Orchard(bytes) => Some(bytes),
            _ => None,
        })
        .ok_or(KeysetError::Encoding)
}

/// Whether a UFVK was derived from this group key.
///
/// This is the check a recovering device must run before trusting a viewing key handed to it by a
/// coordinator or a peer: `ak` is the first 32 bytes of the FVK, and a mismatched key would restore
/// the device into a different — and empty — wallet.
pub fn ufvk_matches_ak(ufvk: &str, group_verifying_key: &[u8; 32]) -> Result<bool, KeysetError> {
    let fvk = fvk_from_ufvk(ufvk)?;
    Ok(&fvk[..32] == group_verifying_key)
}

/// Splice `ak` over the `ak` field of the FVK that `qsk` implies.
fn splice_fvk(group_verifying_key: &[u8; 32], qsk: &[u8; 32]) -> Result<FullViewingKey, KeysetError> {
    let sk = SpendingKey::from_bytes(*qsk).into_option().ok_or(KeysetError::InvalidRecoveryKey)?;
    let mut bytes = FullViewingKey::from(&sk).to_bytes();
    bytes[..32].copy_from_slice(group_verifying_key);
    // `from_bytes` is not a cast: it validates the point, rejects the identity, and re-derives `ivk`
    // to reject the zero/⊥ cases. A garbage `ak` cannot slip through here.
    FullViewingKey::from_bytes(&bytes).ok_or(KeysetError::InvalidFullViewingKey)
}

/// `H(domain ‖ network ‖ fvk)`.
fn fingerprint_of(network: Network, fvk: &[u8; 96]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(FINGERPRINT_DOMAIN);
    let tag = if network == Network::MainNetwork { 0u8 } else { 1u8 };
    h.update([tag]);
    h.update(fvk);
    h.finalize().into()
}

/// `Parameters` is in scope for `UnifiedAddress::encode`; keep the import honest.
#[allow(dead_code)]
fn _assert_parameters<P: Parameters>(_: &P) {}
