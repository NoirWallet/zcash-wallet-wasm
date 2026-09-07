#![cfg(all(feature = "browser", not(target_arch = "wasm32")))]

#[path = "support/browser_storage_cases.rs"]
mod browser_storage_cases;

#[test]
fn round_session_context_distinguishes_absence_and_is_wallet_scoped() {
    use zcash_voting::{storage::VotingDb, Network, VotingRoundParams};
    let db = VotingDb::open(":memory:").unwrap();
    let params = VotingRoundParams {
        vote_round_id: "01".repeat(32),
        snapshot_height: 100,
        ea_pk: vec![7; 32],
        nc_root: vec![1; 32],
        nullifier_imt_root: vec![2; 32],
    };
    db.set_wallet_id("owner-a");
    assert!(db.round_session_json(&params.vote_round_id).is_err());
    db.init_round(Network::Testnet, &params, None).unwrap();
    assert_eq!(db.round_session_json(&params.vote_round_id).unwrap(), None);
    db.clear_round(&params.vote_round_id).unwrap();
    db.init_round(Network::Testnet, &params, Some("opaque caller context"))
        .unwrap();
    assert_eq!(
        db.round_session_json(&params.vote_round_id)
            .unwrap()
            .as_deref(),
        Some("opaque caller context")
    );
    db.set_wallet_id("owner-b");
    assert!(db.round_session_json(&params.vote_round_id).is_err());
    db.init_round(Network::Testnet, &params, None).unwrap();
    assert_eq!(db.round_session_json(&params.vote_round_id).unwrap(), None);
}

#[test]
fn round_protocol_params_are_complete_and_wallet_scoped() {
    use zcash_voting::{storage::VotingDb, Network, VotingRoundParams};
    let db = VotingDb::open(":memory:").unwrap();
    let params = VotingRoundParams {
        vote_round_id: "01".repeat(32),
        snapshot_height: 100,
        ea_pk: vec![7; 32],
        nc_root: vec![1; 32],
        nullifier_imt_root: vec![2; 32],
    };
    db.set_wallet_id("owner-a");
    assert!(db.round_params_with_network(&params.vote_round_id).is_err());
    db.init_round(Network::Testnet, &params, None).unwrap();
    assert_eq!(
        db.round_params_with_network(&params.vote_round_id).unwrap(),
        (params.clone(), Network::Testnet)
    );
    db.set_wallet_id("owner-b");
    assert!(db.round_params_with_network(&params.vote_round_id).is_err());
    let mut other = params.clone();
    other.snapshot_height += 1;
    other.ea_pk[0] ^= 1;
    other.nc_root[0] ^= 1;
    other.nullifier_imt_root[0] ^= 1;
    db.init_round(Network::Mainnet, &other, None).unwrap();
    assert_eq!(
        db.round_params_with_network(&params.vote_round_id).unwrap(),
        (other, Network::Mainnet)
    );
    db.set_wallet_id("owner-a");
    assert_eq!(
        db.round_params_with_network(&params.vote_round_id).unwrap(),
        (params, Network::Testnet)
    );
}

#[test]
fn authenticated_config_round_params_bind_only_the_signed_authority_key() {
    use sha2::{Digest, Sha256};
    use zcash_voting::config::{
        resolve_dynamic_voting_config, resolve_static_voting_config, ResolveVotingConfigOptions,
    };
    let static_bytes = include_bytes!("../../../src/voting/fixtures/static.json");
    let source = format!(
        "https://example.com/static.json?checksum=sha256:{}",
        hex::encode(Sha256::digest(static_bytes))
    );
    let anchor = resolve_static_voting_config(&source, static_bytes).unwrap();
    let resolved = resolve_dynamic_voting_config(
        anchor,
        include_bytes!("../../../src/voting/fixtures/dynamic.json"),
        ResolveVotingConfigOptions::default(),
    )
    .unwrap();
    let id = resolved.authenticated_rounds[0].round_id.clone();
    let params = resolved
        .trusted_voting_round_params(id, 100, vec![1; 32], vec![2; 32])
        .unwrap();
    assert_eq!(params.ea_pk, vec![7; 32]);
    assert!(resolved
        .trusted_voting_round_params("01".repeat(32), 100, vec![1; 32], vec![2; 32])
        .is_err());
    assert!(resolved
        .trusted_voting_round_params(params.vote_round_id, 100, vec![1; 31], vec![2; 32])
        .is_err());
}

mod wallet_inputs {
    use incrementalmerkletree::{
        frontier::{CommitmentTree, Frontier},
        Hashable, Level,
    };
    use orchard::{
        note::{ExtractedNoteCommitment, RandomSeed, Rho},
        tree::MerkleHashOrchard,
    };
    use prost::Message;
    use zcash_client_backend::proto::service::TreeState;
    use zcash_voting::storage::VotingDb;
    use zcash_voting::{
        backend::{orchard, zcash_client_backend, zcash_keys, zcash_primitives},
        delegate::DelegationAccountKeys,
        selection::{DelegationWalletInputs, GatherDelegationWalletInputs},
        store_tree_state_and_witnesses, Network, NoteInfo, VotingHotkey, VotingRoundParams,
        WitnessData,
    };
    const HEIGHT: u64 = 100;
    const ROUND: &str = "wallet-inputs-round";

    fn fixture() -> (NoteInfo, DelegationAccountKeys, TreeState, WitnessData) {
        let ufvk = zcash_keys::keys::UnifiedSpendingKey::from_seed(
            &Network::Regtest,
            &[17; 32],
            zip32::AccountId::ZERO,
        )
        .unwrap()
        .to_unified_full_viewing_key();
        let fvk = ufvk.orchard().unwrap();
        let rho = Rho::from_bytes(&[1; 32]).unwrap();
        let seed = (0..=255)
            .find_map(|b| RandomSeed::from_bytes([b; 32], &rho).into_option())
            .unwrap();
        let note = orchard::Note::from_parts(
            fvk.address_at(0u32, zip32::Scope::External),
            orchard::value::NoteValue::from_raw(zcash_voting::governance::BALLOT_DIVISOR * 2),
            rho,
            seed,
            orchard::NoteVersion::V3,
        )
        .unwrap();
        let cmx: ExtractedNoteCommitment = note.commitment().into();
        let mut frontier = Frontier::<MerkleHashOrchard, 32>::empty();
        frontier.append(MerkleHashOrchard::from_cmx(&cmx));
        let mut tree_bytes = Vec::new();
        zcash_primitives::merkle_tree::write_commitment_tree(
            &CommitmentTree::from_frontier(&frontier),
            &mut tree_bytes,
        )
        .unwrap();
        let state = TreeState {
            network: "regtest".into(),
            height: HEIGHT,
            ironwood_tree: hex::encode(tree_bytes),
            ..TreeState::default()
        };
        let info =
            NoteInfo::from_orchard_note(&note, 0, zip32::Scope::External, &ufvk, &Network::Regtest)
                .unwrap();
        let keys = DelegationAccountKeys {
            account_index: 0,
            orchard_fvk_bytes: fvk.to_bytes(),
            seed_fingerprint: [0; 32],
        };
        let witness = WitnessData {
            position: 0,
            note_commitment: info.commitment.clone(),
            root: frontier.root().to_bytes().to_vec(),
            auth_path: (0..32)
                .map(|level| {
                    MerkleHashOrchard::empty_root(Level::from(level))
                        .to_bytes()
                        .to_vec()
                })
                .collect(),
        };
        (info, keys, state, witness)
    }

    fn gather(
        note: NoteInfo,
        keys: DelegationAccountKeys,
        state: TreeState,
        hotkey: &VotingHotkey,
    ) -> Result<DelegationWalletInputs, zcash_voting::VotingError> {
        DelegationWalletInputs::from_parts(GatherDelegationWalletInputs {
            network: Network::Regtest,
            scanned_height: HEIGHT,
            snapshot_height: HEIGHT,
            anchor_tree_state_bytes: state.encode_to_vec(),
            round_note_infos: vec![note],
            account_keys: keys,
            voting_hotkey: hotkey,
            resolved_round_name: "fixture".into(),
        })
    }

    #[test]
    fn validates_note_identity_network_position_and_tree_height() {
        let (note, keys, state, _) = fixture();
        let hotkey = VotingHotkey::from_stored_secret(&[0x77; 64], Network::Regtest).unwrap();
        assert!(gather(note.clone(), keys.clone(), state.clone(), &hotkey).is_ok());
        for field in [
            "commitment",
            "nullifier",
            "rho",
            "rseed",
            "scope",
            "position",
            "ufvk",
            "value",
        ] {
            let mut invalid = note.clone();
            match field {
                "commitment" => invalid.commitment[0] ^= 1,
                "nullifier" => invalid.nullifier[0] ^= 1,
                "rho" => invalid.rho.clear(),
                "rseed" => invalid.rseed.clear(),
                "scope" => invalid.scope = 2,
                "position" => invalid.position = 1u64 << 32,
                "ufvk" => invalid.ufvk_str.clear(),
                _ => invalid.value += 1,
            }
            assert!(
                gather(invalid, keys.clone(), state.clone(), &hotkey).is_err(),
                "{field}"
            );
        }
        let mut wrong_keys = keys.clone();
        wrong_keys.orchard_fvk_bytes[0] ^= 1;
        assert!(gather(note.clone(), wrong_keys, state.clone(), &hotkey).is_err());
        let mut invalid_account = keys.clone();
        invalid_account.account_index = u32::MAX;
        assert!(gather(note.clone(), invalid_account, state.clone(), &hotkey).is_err());
        let mut stale = state.clone();
        stale.height -= 1;
        assert!(gather(note.clone(), keys.clone(), stale, &hotkey).is_err());
        let mut wrong_network = state.clone();
        wrong_network.network = "test".into();
        assert!(gather(note.clone(), keys.clone(), wrong_network, &hotkey).is_err());
        let wrong_hotkey = VotingHotkey::from_stored_secret(&[0x77; 64], Network::Testnet).unwrap();
        assert!(gather(note, keys, state, &wrong_hotkey).is_err());
    }

    #[test]
    fn rejects_invalid_paths_without_writes_and_retries_complete_cache() {
        let (note, _, state, witness) = fixture();
        let db = VotingDb::open_browser("wallet-inputs", Network::Regtest, None).unwrap();
        db.init_round(
            Network::Regtest,
            &VotingRoundParams {
                vote_round_id: ROUND.into(),
                snapshot_height: HEIGHT,
                nc_root: witness.root.clone(),
                ea_pk: vec![1; 32],
                nullifier_imt_root: vec![2; 32],
            },
            None,
        )
        .unwrap();
        db.ensure_bundles_with_policy(ROUND, &[note.clone()], Default::default())
            .unwrap();
        let before = db.serialize_bytes().unwrap();
        for field in ["root", "path", "position", "commitment", "length"] {
            let mut invalid = witness.clone();
            match field {
                "root" => invalid.root[0] ^= 1,
                "path" => invalid.auth_path[0][0] ^= 1,
                "position" => invalid.position = 1u64 << 32,
                "commitment" => invalid.note_commitment[0] ^= 1,
                _ => {
                    invalid.auth_path.pop();
                }
            }
            assert!(
                store_tree_state_and_witnesses(
                    &db,
                    ROUND,
                    0,
                    &state.encode_to_vec(),
                    &[note.clone()],
                    &[invalid]
                )
                .is_err(),
                "{field}"
            );
            assert_eq!(db.serialize_bytes().unwrap(), before);
        }
        for _ in 0..2 {
            store_tree_state_and_witnesses(
                &db,
                ROUND,
                0,
                &state.encode_to_vec(),
                &[note.clone()],
                &[witness.clone()],
            )
            .unwrap();
        }
        assert!(db.has_complete_witnesses(ROUND, 0, &[note]).unwrap());
    }
}

#[test]
fn wallet_inputs_reject_unsynced_parts_before_construction() {
    use zcash_voting::{
        delegate::DelegationAccountKeys,
        selection::{gather_delegation_wallet_inputs_from_parts, GatherDelegationWalletInputs},
        Network, VotingHotkey,
    };
    let hotkey = VotingHotkey::from_stored_secret(&[0x77; 64], Network::Testnet).unwrap();
    let result = gather_delegation_wallet_inputs_from_parts(GatherDelegationWalletInputs {
        network: Network::Testnet,
        scanned_height: 1,
        snapshot_height: 2,
        anchor_tree_state_bytes: Vec::new(),
        round_note_infos: Vec::new(),
        account_keys: DelegationAccountKeys {
            account_index: 0,
            orchard_fvk_bytes: [0; 96],
            seed_fingerprint: [0; 32],
        },
        voting_hotkey: &hotkey,
        resolved_round_name: "fixture".into(),
    });
    assert!(result.unwrap_err().to_string().contains("not synced"));
}

mod transport {
    use zcash_voting::platform::{
        HttpTransport, OwnedHttpRequest, OwnedHttpResponse, RequestKind, TransportError,
        TransportFuture,
    };

    fn request(kind: RequestKind) -> OwnedHttpRequest {
        OwnedHttpRequest {
            method: if kind == RequestKind::Idempotent {
                "GET"
            } else {
                "POST"
            }
            .into(),
            url: "https://example.com/vote".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: Vec::new(),
            timeout_ms: 30_000,
            max_response_bytes: 4,
            kind,
        }
    }

    #[test]
    fn invalid_request_is_definite_before_dispatch() {
        for url in [
            "not a URL",
            "/relative",
            "ftp://example.com",
            "https://user:pass@example.com",
            "https://example.com/\n",
        ] {
            let mut req = request(RequestKind::NonIdempotent);
            req.url = url.into();
            assert_eq!(req.validate(), Err(TransportError::Definite), "{url:?}");
        }
        let mut req = request(RequestKind::NonIdempotent);
        req.method = "bad method".into();
        assert_eq!(req.validate(), Err(TransportError::Definite));
        req.method = "TRACE".into();
        assert_eq!(req.validate(), Err(TransportError::Definite));
        req.method = "POST".into();
        req.headers
            .push(("X-Test".into(), "value\r\ninjected: true".into()));
        assert_eq!(req.validate(), Err(TransportError::Definite));
        req.headers.pop();
        for timeout in [0, u32::MAX] {
            req.timeout_ms = timeout;
            assert_eq!(req.validate(), Err(TransportError::Definite));
        }
        let mut get = request(RequestKind::Idempotent);
        get.body.push(1);
        assert_eq!(get.validate(), Err(TransportError::Definite));
        assert_eq!(request(RequestKind::Idempotent).validate(), Ok(()));
    }

    #[test]
    fn rejected_get_is_retryable_but_dispatched_post_is_ambiguous() {
        let get = TransportError::after_dispatch(RequestKind::Idempotent);
        assert_eq!(get, TransportError::Definite);
        assert!(get.is_safe_to_retry());
        let post = TransportError::after_dispatch(RequestKind::NonIdempotent);
        assert_eq!(post, TransportError::Ambiguous);
        assert!(!post.is_safe_to_retry());
    }

    #[test]
    fn limits_apply_to_headers_and_actual_bytes_without_erasing_post_ambiguity() {
        for kind in [RequestKind::Idempotent, RequestKind::NonIdempotent] {
            let req = request(kind);
            let oversized = if kind == RequestKind::Idempotent {
                TransportError::ResponseTooLarge { limit: 4 }
            } else {
                TransportError::Ambiguous
            };
            assert_eq!(req.check_response_size(4), Ok(()));
            assert_eq!(req.check_response_size(5), Err(oversized.clone()));
            for length in ["5", " 5 ", "9999999999999999999999999999999999999999999"] {
                assert_eq!(
                    req.check_content_length(Some(length)),
                    Err(oversized.clone())
                );
            }
            for length in [
                None,
                Some("4"),
                Some(""),
                Some("invalid"),
                Some("-1"),
                Some("+5"),
                Some("4, 5"),
            ] {
                assert_eq!(req.check_content_length(length), Ok(()));
                // An absent, invalid or understated header cannot bypass the body limit.
                assert_eq!(req.check_response_size(5), Err(oversized.clone()));
            }
        }
        assert!(!TransportError::ResponseTooLarge { limit: 4 }.is_safe_to_retry());
    }

    #[test]
    fn owned_boundary_is_send_and_transport_is_object_safe() {
        fn send<T: Send>() {}
        fn object_safe(_: Option<&dyn HttpTransport>) {}
        send::<OwnedHttpRequest>();
        send::<OwnedHttpResponse>();
        send::<TransportError>();
        send::<TransportFuture>();
        object_safe(None);
    }

    #[test]
    fn javascript_response_lengths_are_validated_before_integer_conversion() {
        for kind in [RequestKind::Idempotent, RequestKind::NonIdempotent] {
            let req = request(kind);
            assert_eq!(req.check_response_size_number(4.0), Ok(4));
            for size in [5.0, 4_294_967_296.0, f64::MAX] {
                let expected = match kind {
                    RequestKind::Idempotent => TransportError::ResponseTooLarge { limit: 4 },
                    RequestKind::NonIdempotent => TransportError::Ambiguous,
                };
                assert_eq!(req.check_response_size_number(size), Err(expected));
            }
            for size in [-1.0, 0.5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                assert_eq!(
                    req.check_response_size_number(size),
                    Err(TransportError::after_dispatch(kind))
                );
            }
            let mut widest = request(kind);
            widest.max_response_bytes = usize::MAX;
            assert!(widest
                .check_response_size_number(usize::MAX as f64 + 1.0)
                .is_err());
            assert!(widest
                .check_response_size_number(isize::MAX as f64 + 1.0)
                .is_err());
        }
    }
}

#[test]
fn browser_db_file_backed_wal_snapshot_preserves_pending_rows() {
    use rusqlite::Connection;
    use zcash_voting::{
        round::{RoundParams, VotingDb},
        session::{resume_plan, Decision},
        types::Network,
    };

    struct Files(std::path::PathBuf);
    impl Drop for Files {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
            }
        }
    }

    for page_size in [4096, 8192] {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let files = Files(std::env::temp_dir().join(format!(
            "browser-wal-{}-{unique}.sqlite",
            std::process::id()
        )));
        let setup = Connection::open(&files.0).unwrap();
        setup.pragma_update(None, "page_size", page_size).unwrap();
        setup.execute_batch("VACUUM").unwrap();
        drop(setup);

        let source = VotingDb::open_path(&files.0).unwrap();
        source.set_wallet_id("wallet-a");
        let observer = Connection::open(&files.0).unwrap();
        // Exercise a v17 file and leave all subsequent round data in its WAL.
        observer.execute_batch("DROP TABLE wallet_metadata; PRAGMA user_version=17; PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
        let params = RoundParams {
            vote_round_id: "0101010101010101010101010101010101010101010101010101010101010101"
                .into(),
            snapshot_height: 1000,
            ea_pk: vec![0xEA; 32],
            nc_root: vec![0xAA; 32],
            nullifier_imt_root: vec![0xBB; 32],
        };
        source
            .create_round(Network::Testnet, &params, Some("wal-session"))
            .unwrap();
        source
            .set_ballot_intent(&params.vote_round_id, 1, Decision::Skipped, 3)
            .unwrap();
        assert!(
            std::fs::metadata(format!("{}-wal", files.0.display()))
                .unwrap()
                .len()
                > 32
        );
        let expected = resume_plan(&source, &params.vote_round_id, &[1]).unwrap();
        let bytes = source.serialize_bytes().unwrap();
        let restored = VotingDb::open_browser("wallet-a", Network::Testnet, Some(&bytes)).unwrap();
        assert_eq!(
            resume_plan(&restored, &params.vote_round_id, &[1]).unwrap(),
            expected
        );
        assert_eq!(
            restored.ballot_intents(&params.vote_round_id).unwrap(),
            vec![(1, Decision::Skipped)]
        );
        assert_eq!(
            observer
                .pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        assert_eq!(
            observer
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            17
        );
        assert_eq!(
            observer
                .pragma_query_value(None, "page_size", |r| r.get::<_, i64>(0))
                .unwrap(),
            page_size
        );
        source
            .set_ballot_intent(&params.vote_round_id, 1, Decision::Choice(1), 3)
            .unwrap();
        assert_eq!(
            restored.ballot_intents(&params.vote_round_id).unwrap(),
            vec![(1, Decision::Skipped)]
        );

        let raw_wal_bytes = observer.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
        let error = VotingDb::open_browser("wallet-a", Network::Testnet, Some(&raw_wal_bytes))
            .err()
            .expect("raw WAL image must be rejected");
        assert!(
            error
                .to_string()
                .contains("WAL snapshot requires normalized export"),
            "{error}"
        );
    }
}
