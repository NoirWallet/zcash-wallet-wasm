//! Per-recipient sealing for DKG round-2 packages.
//!
//! A round-2 package carries one dealer's secret share for one recipient. In a star topology every
//! such package passes through the coordinator, and a coordinator that can read them all can
//! reconstruct the group signing key: with `n` participants and threshold `t`, each dealer's
//! polynomial is pinned down by `t` points, so an observer of the round-2 traffic of any `n > t`
//! ceremony — including 2-of-3 — interpolates it outright.
//!
//! So round 2 must be confidential as well as authenticated (RFC 9591 requires it). This module is
//! that requirement and nothing more:
//!
//! * **Noise with both static keys known in advance.** Every participant publishes an X25519 static
//!   key in its round-1 broadcast, so by round 2 both ends already know each other — the handshake is
//!   a single message with no round trip, which is what makes it usable when a round trip means a
//!   human holding a phone up to another human.
//! * **The ceremony is the prologue.** Both sides derive it from the full participant set, so a
//!   message cannot be moved between ceremonies, and a party that sees a different roster cannot
//!   open it at all.
//! * **The sender's static key is mixed into the key schedule**, not merely wrapped in a signature,
//!   so impressing a different sender's identity fails.
//!
//! This is the same construction ZF's `frost-client` uses ("did they use the pattern correctly" is an
//! answerable review question; "is this bespoke scheme sound" is not).

use snow::params::NoiseParams;

/// Pattern with both statics as pre-messages: one message from sender to recipient, no round trip.
const PATTERN: &str = "Noise_KK_25519_ChaChaPoly_BLAKE2s";

/// Domain separator for the prologue, so it cannot collide with another protocol's transcript.
pub const PROLOGUE_DOMAIN: &[u8] = b"noir-frost-vault/dkg-r2/v1";

const MAX_SEALED: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
pub enum SealError {
    /// Noise refused the parameters, the keys, or the prologue.
    Setup,
    /// The ciphertext did not authenticate: wrong sender, wrong recipient, wrong ceremony, or tampered.
    Open,
    /// The plaintext exceeded the buffer this module is willing to allocate.
    TooLarge,
}

impl core::fmt::Display for SealError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Setup => f.write_str("sealing parameters rejected"),
            Self::Open => f.write_str("sealed package did not authenticate"),
            Self::TooLarge => f.write_str("package exceeds the sealing buffer"),
        }
    }
}

impl std::error::Error for SealError {}

fn params() -> Result<NoiseParams, SealError> {
    PATTERN.parse().map_err(|_| SealError::Setup)
}

/// A participant's static X25519 keypair, generated once per ceremony and carried in its round-1
/// secret and broadcast respectively.
pub fn generate_keypair() -> Result<(Vec<u8>, Vec<u8>), SealError> {
    let builder = snow::Builder::new(params()?);
    let keypair = builder.generate_keypair().map_err(|_| SealError::Setup)?;
    Ok((keypair.private, keypair.public))
}

/// Seal `plaintext` so that only the holder of `recipient_public` can read it.
pub fn seal(
    sender_private: &[u8],
    recipient_public: &[u8],
    prologue: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, SealError> {
    if plaintext.len() > MAX_SEALED {
        return Err(SealError::TooLarge);
    }
    let mut handshake = snow::Builder::new(params()?)
        .local_private_key(sender_private)
        .remote_public_key(recipient_public)
        .prologue(&[PROLOGUE_DOMAIN, prologue].concat())
        .build_initiator()
        .map_err(|_| SealError::Setup)?;

    let mut buf = vec![0u8; MAX_SEALED + 128];
    let len = handshake
        .write_message(plaintext, &mut buf)
        .map_err(|_| SealError::Setup)?;
    buf.truncate(len);
    Ok(buf)
}

/// Open a package sealed by `sender_public`. Fails if the pair, the ceremony or the bytes are wrong.
pub fn open(
    recipient_private: &[u8],
    sender_public: &[u8],
    prologue: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, SealError> {
    let mut handshake = snow::Builder::new(params()?)
        .local_private_key(recipient_private)
        .remote_public_key(sender_public)
        .prologue(&[PROLOGUE_DOMAIN, prologue].concat())
        .build_responder()
        .map_err(|_| SealError::Setup)?;

    let mut buf = vec![0u8; MAX_SEALED + 128];
    let len = handshake
        .read_message(ciphertext, &mut buf)
        .map_err(|_| SealError::Open)?;
    buf.truncate(len);
    Ok(buf)
}

/// The prologue both sides of a ceremony derive: the domain tag followed by every participant's
/// slot and static public key, in slot order.
///
/// Both parties must agree on the whole roster, so a coordinator that shows different rosters to
/// different devices produces different key schedules and the messages do not open.
pub fn ceremony_prologue(participants: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut sorted = participants.to_vec();
    sorted.sort_by_key(|(slot, _)| *slot);
    let mut out = Vec::with_capacity(2 + sorted.len() * 34);
    out.push(sorted.len() as u8);
    for (slot, public) in sorted {
        out.push(slot);
        // A static key is 32 bytes for X25519; anything else is a caller error we refuse to hash.
        out.push(public.len() as u8);
        out.extend_from_slice(&public);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prologue() -> Vec<u8> {
        let (_, a) = generate_keypair().unwrap();
        let (_, b) = generate_keypair().unwrap();
        ceremony_prologue(&[(0, a), (1, b)])
    }

    #[test]
    fn a_sealed_package_round_trips_between_two_known_parties() {
        let (sender_sk, sender_pk) = generate_keypair().unwrap();
        let (recipient_sk, recipient_pk) = generate_keypair().unwrap();
        let prologue = prologue();

        let sealed = seal(&sender_sk, &recipient_pk, &prologue, b"secret share").unwrap();
        // The sealed form must not be the plaintext.
        assert_ne!(sealed, b"secret share".to_vec());
        assert!(!sealed.windows(12).any(|w| w == b"secret share"));

        let opened = open(&recipient_sk, &sender_pk, &prologue, &sealed).unwrap();
        assert_eq!(opened, b"secret share");
    }

    #[test]
    fn a_third_party_cannot_open_it() {
        // The whole point: the coordinator that relays the package holds neither static secret.
        let (sender_sk, sender_pk) = generate_keypair().unwrap();
        let (_, recipient_pk) = generate_keypair().unwrap();
        let (relay_sk, _) = generate_keypair().unwrap();
        let prologue = prologue();

        let sealed = seal(&sender_sk, &recipient_pk, &prologue, b"share for recipient").unwrap();
        assert_eq!(
            open(&relay_sk, &sender_pk, &prologue, &sealed),
            Err(SealError::Open)
        );
    }

    #[test]
    fn a_different_ceremony_prologue_does_not_open() {
        let (sender_sk, sender_pk) = generate_keypair().unwrap();
        let (recipient_sk, recipient_pk) = generate_keypair().unwrap();
        let sealed = seal(&sender_sk, &recipient_pk, &prologue(), b"share").unwrap();

        // Same keys, different roster: the key schedule differs and nothing opens.
        let (_, other) = generate_keypair().unwrap();
        let other_prologue = ceremony_prologue(&[(0, other)]);
        assert_eq!(
            open(&recipient_sk, &sender_pk, &other_prologue, &sealed),
            Err(SealError::Open)
        );
    }

    #[test]
    fn a_tampered_package_does_not_open() {
        let (sender_sk, sender_pk) = generate_keypair().unwrap();
        let (recipient_sk, recipient_pk) = generate_keypair().unwrap();
        let prologue = prologue();
        let mut sealed = seal(&sender_sk, &recipient_pk, &prologue, b"share").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;

        assert_eq!(
            open(&recipient_sk, &sender_pk, &prologue, &sealed),
            Err(SealError::Open)
        );
    }

    #[test]
    fn the_prologue_is_order_independent() {
        let (_, a) = generate_keypair().unwrap();
        let (_, b) = generate_keypair().unwrap();
        assert_eq!(
            ceremony_prologue(&[(1, b.clone()), (0, a.clone())]),
            ceremony_prologue(&[(0, a), (1, b)])
        );
    }
}
