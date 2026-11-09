//! Metric baseline for the frontend team.
//!
//! This file is the only place where the wasm tiers are asked "how big is this on a QR". The numbers
//! are produced by running the real code, not by estimating from field widths, so a change in FROST
//! serialization or in the envelope shows up here.
//!
//! Run `cargo test -- --ignored --nocapture metrics_report` to print the table.

use std::collections::BTreeMap;

use frost_vault_core::protocol::Envelope;
use frost_vault_core::session::{CoordinatorSession, SessionConfig, SignerSession, SESSION_ID_LEN};
use frost_vault_core::threshold::{run_dkg, Identifier};
use rand::{rand_core::UnwrapErr, rngs::SysRng};
use rand08::rngs::OsRng as FrostRng;
use vault_pczt::frames;

const SESSION: [u8; SESSION_ID_LEN] = [0x11u8; SESSION_ID_LEN];

struct Messages {
    init: usize,
    commitments: usize,
    payload: usize,
    share: usize,
}

/// Drive one 2-of-3 session for `actions` actions and record every message size.
fn measure(actions: usize) -> Messages {
    let mut rng = FrostRng;
    let dkg = run_dkg(3, 2, &mut rng).expect("dkg");
    let identifiers: BTreeMap<u8, Identifier> = dkg
        .identifiers
        .iter()
        .enumerate()
        .map(|(i, id)| (i as u8, *id))
        .collect();
    let cfg = SessionConfig {
        session_id: SESSION,
        participants: 3,
        threshold: 2,
        signers: vec![0, 1],
        identifiers: identifiers.clone(),
    };
    let mut coordinator =
        CoordinatorSession::new(cfg, dkg.pubkeys.clone(), actions).expect("coordinator");

    let mut out = Messages {
        init: 0,
        commitments: 0,
        payload: 0,
        share: 0,
    };

    let inits = coordinator.init().expect("init");
    out.init = inits[0].encode().len();

    // Keep the signer sessions alive: their nonces must be the ones the coordinator saw commitments
    // for, so re-creating them after the fact would produce shares for a different commitment set.
    let mut signers: BTreeMap<u8, SignerSession> = BTreeMap::new();
    for env in &inits {
        let slot = env.recipient;
        let mut signer = SignerSession::new(SESSION, slot, identifiers.clone());
        let commitments = signer
            .on_init(env, &dkg.key_packages[&identifiers[&slot]], &mut rng)
            .expect("commitments");
        out.commitments = commitments.encode().len();
        coordinator.on_commitments(&commitments).expect("collect");
        signers.insert(slot, signer);
    }

    let alphas: Vec<[u8; 32]> = (0..actions)
        .map(|i| {
            let mut a = [0u8; 32];
            a[0] = i as u8 + 1;
            a[31] = 0x01;
            a
        })
        .collect();
    coordinator
        .prepare_payload([0x5Au8; 32], alphas)
        .expect("prepare");
    let payloads = coordinator.payloads().expect("payloads");
    out.payload = payloads[0].encode().len();

    for env in &payloads {
        let slot = env.recipient;
        let signer = signers.get_mut(&slot).expect("signer");
        signer.on_payload(env).expect("payload");
        signer.approve().expect("approve");
        let share = signer
            .share(&dkg.key_packages[&identifiers[&slot]])
            .expect("share");
        out.share = share.encode().len();
        coordinator.on_share(&share).expect("collect share");
    }

    coordinator.aggregate().expect("aggregate");
    out
}

fn seconds(len: usize, frame_size: usize, fps: f64) -> f64 {
    frames(len, frame_size) as f64 / fps
}

#[test]
fn session_messages_fit_in_a_small_number_of_frames() {
    // Loose regression bounds: the point is to notice a blow-up, not to pin FROST's serialization.
    for actions in 1..=3 {
        let m = measure(actions);
        assert!(m.init < 400, "init {actions}: {}", m.init);
        assert!(
            m.commitments < 400,
            "commitments {actions}: {}",
            m.commitments
        );
        // The payload carries the whole commitment grid, so it grows with signers × actions.
        assert!(m.payload < 2_000, "payload {actions}: {}", m.payload);
        assert!(m.share < 400, "share {actions}: {}", m.share);
        // One 800-byte frame must carry every control message for the daily 1-action case.
        assert_eq!(frames(m.payload, 800), 1);
    }
}

#[test]
#[ignore]
fn metrics_report() {
    println!("\nSession control messages (2-of-3, session id 16 B, envelope header 29 B)\n");
    println!("| actions | SIGN_INIT | SIGN_COMMITMENTS | SIGN_PAYLOAD | SIGN_SHARE |");
    println!("| --- | --- | --- | --- | --- |");
    for actions in 1..=3 {
        let m = measure(actions);
        println!(
            "| {actions} | {} B | {} B | {} B | {} B |",
            m.init, m.commitments, m.payload, m.share
        );
    }

    // PCZT sizes, measured elsewhere in this workspace on a real one-action Ironwood transaction:
    // vault-pczt's `prepare` test prints the same numbers.
    let proven = 7_356usize;
    let stripped = 2_362usize;

    println!("\nPCZT direction (1 action, measured on a real proven transaction)\n");
    println!("| payload | bytes | frames @200 | @400 | @800 | @2953 |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for (name, len) in [
        ("proven (coordinator keeps)", proven),
        ("stripped (transport)", stripped),
    ] {
        println!(
            "| {name} | {len} | {} | {} | {} | {} |",
            frames(len, 200),
            frames(len, 400),
            frames(len, 800),
            frames(len, 2953)
        );
    }

    println!("\nWall-clock for the stripped 2362 B request\n");
    println!("| frame size | frames | @5 fps | @7 fps | @10 fps |");
    println!("| --- | --- | --- | --- | --- |");
    for size in [200usize, 400, 800, 2953] {
        let n = frames(stripped, size);
        println!(
            "| {size} | {n} | {:.1} s | {:.1} s | {:.1} s |",
            seconds(stripped, size, 5.0),
            seconds(stripped, size, 7.0),
            seconds(stripped, size, 10.0)
        );
    }
    println!();
}
