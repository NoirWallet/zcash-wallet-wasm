#![cfg(all(feature = "browser", not(target_arch = "wasm32")))]

#[path = "support/browser_storage_cases.rs"]
mod browser_storage_cases;

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
