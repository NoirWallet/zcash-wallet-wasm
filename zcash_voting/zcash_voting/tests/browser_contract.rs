#![cfg(all(feature = "browser", not(target_arch = "wasm32")))]

#[path = "support/browser_storage_cases.rs"]
mod browser_storage_cases;

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
