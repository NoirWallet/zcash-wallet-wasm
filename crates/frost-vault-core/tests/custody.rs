//! Custody: restoring the right share, and refusing to resume the wrong session.
//!
//! Both are MV3 problems. A Service Worker can be stopped between any two awaits, so a restart can
//! happen in the middle of a signing session, and the Extension's share lives in a store the browser
//! owns. The tests here are about the two failures that cost money: a share bound to the wrong vault,
//! and a session resumed after its nonces were already published.

use std::collections::BTreeMap;
use std::collections::HashMap;

use frost_vault_core::custody::{
    new_session_id, CustodyError, GuardError, Recovery, SessionGuard, SessionProgress, SessionStore, ShareBackup, ShareIdentity, MARKER_KEY,
};
use frost_vault_core::threshold::{run_dkg, Identifier};
use rand::rngs::OsRng;

fn dkg(participants: u16, threshold: u16) -> frost_vault_core::threshold::DkgOutput {
    run_dkg(participants, threshold, &mut OsRng).expect("dkg")
}

fn identity(slot: u8, device: &[u8], vault: &[u8]) -> ShareIdentity {
    ShareIdentity {
        slot,
        device_id: device.to_vec(),
        vault_id: vault.to_vec(),
    }
}

fn roster(dkg: &frost_vault_core::threshold::DkgOutput) -> BTreeMap<u8, Identifier> {
    dkg.identifiers.iter().enumerate().map(|(i, id)| (i as u8, *id)).collect()
}

// ---------------------------------------------------------------------------
// Share backup
// ---------------------------------------------------------------------------

#[test]
fn a_share_round_trips_and_still_signs() {
    let dkg = dkg(3, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let by_slot = roster(&dkg);
    let kp = &dkg.key_packages[&by_slot[&1]];

    let backup = ShareBackup::new(kp, identity(1, b"device-b", b"vault-1")).expect("wrap");
    let bytes = backup.to_bytes().expect("serialize");

    let restored = ShareBackup::from_bytes(&bytes, &ak, 1).expect("restore");
    assert_eq!(restored.identity().slot, 1);
    assert_eq!(restored.group_verifying_key(), &ak);
    assert!(restored.matches_group(&dkg.pubkeys));
    assert_eq!(
        restored.identifier().unwrap(),
        *kp.identifier(),
        "the restored share must sign under the same identifier"
    );

    // And it is the same key material, byte for byte.
    assert_eq!(restored.key_package().unwrap().signing_share(), kp.signing_share());
}

#[test]
fn a_share_for_another_group_is_refused() {
    let a = dkg(2, 2);
    let b = dkg(2, 2);
    let by_slot = roster(&a);
    let backup = ShareBackup::new(&a.key_packages[&by_slot[&0]], identity(0, b"d", b"v")).expect("wrap");
    let bytes = backup.to_bytes().unwrap();

    // Restoring vault A's share into vault B must fail on the group binding.
    let err = ShareBackup::from_bytes(&bytes, &b.group_verifying_key().unwrap(), 0).unwrap_err();
    assert_eq!(err, CustodyError::WrongGroup);
    assert!(!backup.matches_group(&b.pubkeys));
}

#[test]
fn a_share_restored_into_the_wrong_slot_is_refused() {
    // The dangerous confusion: two devices of the same 2-of-3 vault, shares swapped. Both shares are
    // valid FROST packages for the same group, so only the slot binding catches this.
    let dkg = dkg(3, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let by_slot = roster(&dkg);
    let share_b = ShareBackup::new(&dkg.key_packages[&by_slot[&1]], identity(1, b"b", b"v"))
        .unwrap()
        .to_bytes()
        .unwrap();

    let err = ShareBackup::from_bytes(&share_b, &ak, 2).unwrap_err();
    assert_eq!(err, CustodyError::WrongSlot { expected: 2, found: 1 });
}

#[test]
fn an_edited_record_is_detected() {
    let dkg = dkg(2, 2);
    let ak = dkg.group_verifying_key().unwrap();
    let by_slot = roster(&dkg);
    let mut bytes = ShareBackup::new(&dkg.key_packages[&by_slot[&0]], identity(0, b"d", b"v"))
        .unwrap()
        .to_bytes()
        .unwrap();

    // Flip a byte in the identity section: the trailing check must catch it.
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    assert_eq!(ShareBackup::from_bytes(&bytes, &ak, 0).unwrap_err(), CustodyError::Corrupt);

    // Truncation is refused too, and not as a panic.
    let full = ShareBackup::new(&dkg.key_packages[&by_slot[&0]], identity(0, b"d", b"v"))
        .unwrap()
        .to_bytes()
        .unwrap();
    for cut in [0usize, 1, 10, full.len() - 1] {
        assert!(ShareBackup::from_bytes(&full[..cut], &ak, 0).is_err());
    }
}

#[test]
fn the_debug_output_never_contains_the_share() {
    let dkg = dkg(2, 2);
    let by_slot = roster(&dkg);
    let kp = &dkg.key_packages[&by_slot[&0]];
    let share_hex = hex::encode(kp.serialize().unwrap());
    let backup = ShareBackup::new(kp, identity(0, b"d", b"v")).unwrap();
    let printed = format!("{backup:?}");
    assert!(printed.contains("<redacted>"));
    assert!(!printed.contains(&share_hex));
}

#[test]
fn an_over_long_identity_is_refused() {
    let dkg = dkg(2, 2);
    let by_slot = roster(&dkg);
    let too_long = vec![0u8; 200];
    let err = ShareBackup::new(&dkg.key_packages[&by_slot[&0]], identity(0, &too_long, b"v")).unwrap_err();
    assert_eq!(err, CustodyError::Malformed);
}

// ---------------------------------------------------------------------------
// Session guard
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MemoryStore {
    map: HashMap<String, Vec<u8>>,
}

impl SessionStore for MemoryStore {
    type Error = ();
    fn put(&mut self, key: &str, value: &[u8]) -> Result<(), ()> {
        self.map.insert(key.to_string(), value.to_vec());
        Ok(())
    }
    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, ()> {
        Ok(self.map.get(key).cloned())
    }
    fn delete(&mut self, key: &str) -> Result<(), ()> {
        self.map.remove(key);
        Ok(())
    }
}

fn sid(tag: u8) -> [u8; 16] {
    let mut s = [0u8; 16];
    s[0] = tag;
    s
}

#[test]
fn a_clean_start_reports_clean() {
    let mut store = MemoryStore::default();
    assert_eq!(SessionGuard::recover(&mut store).unwrap(), Recovery::Clean);
}

#[test]
fn a_finished_session_leaves_nothing_behind() {
    let mut store = MemoryStore::default();
    let guard = SessionGuard::begin(&mut store, sid(1)).unwrap();
    guard.finish(&mut store).unwrap();
    assert_eq!(SessionGuard::recover(&mut store).unwrap(), Recovery::Clean);
}

#[test]
fn killing_before_any_commitment_is_only_discardable() {
    let mut store = MemoryStore::default();
    let _guard = SessionGuard::begin(&mut store, sid(2)).unwrap();
    // Process dies here: nothing left the device, so the nonces were never published.
    assert_eq!(
        SessionGuard::recover(&mut store).unwrap(),
        Recovery::Discardable {
            session_id: sid(2),
            progress: SessionProgress::Created
        }
    );
    assert!(!Recovery::Discardable {
        session_id: sid(2),
        progress: SessionProgress::Created
    }
    .may_resume());
}

#[test]
fn killing_after_commitments_burns_the_session() {
    let mut store = MemoryStore::default();
    let mut guard = SessionGuard::begin(&mut store, sid(3)).unwrap();
    guard.mark_committed(&mut store).unwrap();
    // Process dies. The commitments are already out, so the nonces behind them are spent even though
    // no share was produced locally.
    let recovery = SessionGuard::recover(&mut store).unwrap();
    assert_eq!(
        recovery,
        Recovery::Burned {
            session_id: sid(3),
            progress: SessionProgress::Committed
        }
    );
    assert!(!recovery.may_resume());
    assert_eq!(recovery.session_id(), Some(sid(3)));
}

#[test]
fn killing_after_a_share_burns_the_session() {
    let mut store = MemoryStore::default();
    let mut guard = SessionGuard::begin(&mut store, sid(4)).unwrap();
    guard.mark_committed(&mut store).unwrap();
    guard.mark_consumed(&mut store).unwrap();
    assert_eq!(
        SessionGuard::recover(&mut store).unwrap(),
        Recovery::Burned {
            session_id: sid(4),
            progress: SessionProgress::Consumed
        }
    );
}

#[test]
fn two_sessions_cannot_be_in_flight_at_once() {
    let mut store = MemoryStore::default();
    let _first = SessionGuard::begin(&mut store, sid(5)).unwrap();
    let err = SessionGuard::begin(&mut store, sid(6)).unwrap_err();
    assert!(matches!(err, GuardError::AlreadyInFlight));
}

#[test]
fn abandoning_clears_the_marker_so_a_new_session_can_start() {
    let mut store = MemoryStore::default();
    let mut first = SessionGuard::begin(&mut store, sid(7)).unwrap();
    first.mark_committed(&mut store).unwrap();
    drop(first);
    // Caller inspects, decides the session is unrecoverable, and clears it.
    assert!(matches!(SessionGuard::recover(&mut store).unwrap(), Recovery::Burned { .. }));
    SessionGuard::abandon(&mut store).unwrap();
    // A brand new session id, not the burned one.
    let _second = SessionGuard::begin(&mut store, sid(8)).unwrap();
}

#[test]
fn a_corrupt_marker_is_refused_rather_than_guessed() {
    let mut store = MemoryStore::default();
    store.put(MARKER_KEY, &[9u8; 3]).unwrap();
    assert!(matches!(SessionGuard::recover(&mut store), Err(GuardError::Malformed)));
}

#[test]
fn session_ids_are_random_not_sequential() {
    // A counter would make a stale marker look like the current session after a restart.
    let a = new_session_id(&mut OsRng);
    let b = new_session_id(&mut OsRng);
    assert_ne!(a, b);
    assert_ne!(a, [0u8; 16]);
}
