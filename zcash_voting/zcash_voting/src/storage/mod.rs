mod migrations;
pub mod operations;
pub mod queries;

use std::sync::Mutex;
#[cfg(target_arch = "wasm32")]
use std::{
    cell::{RefCell, RefMut},
    rc::Rc,
};
#[cfg(not(target_arch = "wasm32"))]
use std::{sync::MutexGuard, time::Duration};

use rusqlite::Connection;

use crate::types::{Network, VotingError};

#[cfg(not(target_arch = "wasm32"))]
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(not(target_arch = "wasm32"))]
type ConnectionOwner = Mutex<Connection>;
#[cfg(target_arch = "wasm32")]
type ConnectionOwner = Rc<RefCell<Connection>>;
#[cfg(not(target_arch = "wasm32"))]
type ConnectionGuard<'a> = MutexGuard<'a, Connection>;
#[cfg(target_arch = "wasm32")]
type ConnectionGuard<'a> = RefMut<'a, Connection>;

/// Current phase of a voting round.
///
/// Discriminants are ordered lifecycle ranks; `advance_round_phase` compares
/// them to enforce forward-only progression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum RoundPhase {
    Initialized = 0,
    HotkeyGenerated = 1,
    DelegationConstructed = 2,
    DelegationProved = 3,
    VoteReady = 4,
}

impl RoundPhase {
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Initialized,
            1 => Self::HotkeyGenerated,
            2 => Self::DelegationConstructed,
            3 => Self::DelegationProved,
            4 => Self::VoteReady,
            _ => Self::Initialized,
        }
    }
}

/// Summary state of a voting round (for UI / SDK queries).
#[derive(Clone, Debug)]
pub struct RoundState {
    pub round_id: String,
    pub phase: RoundPhase,
    pub network: Network,
    pub snapshot_height: u64,
    pub hotkey_address: Option<String>,
    pub delegated_weight: Option<u64>,
    pub proof_generated: bool,
}

/// A vote record from the votes table.
pub use crate::wire::VoteRecord;

/// Compact round info for list_rounds().
#[derive(Clone, Debug)]
pub struct RoundSummary {
    pub round_id: String,
    pub wallet_id: String,
    pub phase: RoundPhase,
    pub network: Network,
    pub snapshot_height: u64,
    pub created_at: u64,
}

/// A Keystone bundle signature stored in the DB.
pub use crate::wire::KeystoneSignatureRecord;

/// One Keystone signature tuple to store as part of an atomic batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeystoneSignatureInput {
    pub bundle_index: u32,
    pub sig: Vec<u8>,
    pub sighash: Vec<u8>,
    pub rk: Vec<u8>,
}

/// Counts from an idempotent atomic Keystone signature batch write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeystoneSignatureBatchResult {
    pub inserted: u32,
    pub already_present: u32,
}

/// Database handle for voting state. Wraps a SQLite connection and a
/// wallet identifier that scopes all round data to a single wallet.
pub struct VotingDb {
    conn: ConnectionOwner,
    wallet_id: Mutex<String>,
    browser_owner: Option<(String, Network)>,
}

impl VotingDb {
    /// Open (or create) the voting database at the given path.
    /// Runs migrations automatically.
    /// Call `set_wallet_id` before performing any round operations.
    pub fn open(path: &str) -> Result<Self, VotingError> {
        #[cfg(target_arch = "wasm32")]
        if path != ":memory:" {
            return Err(VotingError::InvalidInput {
                message: "browser voting databases must be in memory".into(),
            });
        }
        let mut conn = if path == ":memory:" {
            Connection::open_in_memory()
        } else {
            Connection::open(path)
        }
        .map_err(|e| VotingError::Internal {
            message: format!("failed to open database: {}", e),
        })?;

        #[cfg(not(target_arch = "wasm32"))]
        conn.busy_timeout(SQLITE_BUSY_TIMEOUT)
            .map_err(|e| VotingError::Internal {
                message: format!("failed to configure database busy timeout: {}", e),
            })?;
        #[cfg(not(target_arch = "wasm32"))]
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| VotingError::Internal {
                message: format!("failed to set journal mode: {e}"),
            })?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(|e| VotingError::Internal {
                message: format!("failed to set pragmas: {}", e),
            })?;

        migrations::migrate(&mut conn)?;

        Ok(Self {
            conn: ConnectionOwner::new(conn.into()),
            wallet_id: Mutex::new(String::new()),
            browser_owner: None,
        })
    }

    /// Opens an account- and network-bound, in-memory database from owned bytes.
    /// The input slice can be changed or dropped immediately after this returns.
    /// Use snapshots from `serialize_bytes`, including for file-backed sources.
    /// Raw WAL-format images require normalized export; copying a main database
    /// file alone can also omit committed content still in its WAL sidecar.
    pub fn open_browser(
        wallet_id: &str,
        network: Network,
        bytes: Option<&[u8]>,
    ) -> Result<Self, VotingError> {
        if wallet_id.is_empty() {
            return Err(VotingError::InvalidInput {
                message: "wallet owner must not be empty".into(),
            });
        }
        let mut conn = Connection::open_in_memory().map_err(browser_storage_error)?;
        if let Some(bytes) = bytes.filter(|bytes| !bytes.is_empty()) {
            if bytes.starts_with(b"SQLite format 3\0")
                && bytes
                    .get(18..20)
                    .is_some_and(|versions| versions.contains(&2))
            {
                return Err(VotingError::InvalidInput {
                    message: "WAL snapshot requires normalized export with serialize_bytes".into(),
                });
            }
            conn.deserialize_read_exact(
                rusqlite::MAIN_DB,
                std::io::Cursor::new(bytes),
                bytes.len(),
                false,
            )
            .map_err(browser_storage_error)?;
        }
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(browser_storage_error)?;
        validate_browser_identity(&conn, wallet_id, network)?;
        migrations::migrate(&mut conn)?;
        validate_browser_identity(&conn, wallet_id, network)?;
        conn.execute(
            "INSERT OR IGNORE INTO wallet_metadata (singleton, wallet_id, network) VALUES (1, ?1, ?2)",
            rusqlite::params![wallet_id, queries::network_to_storage(network)],
        ).map_err(browser_storage_error)?;
        Ok(Self {
            conn: ConnectionOwner::new(conn.into()),
            wallet_id: Mutex::new(wallet_id.into()),
            browser_owner: Some((wallet_id.into(), network)),
        })
    }

    /// Returns an independent snapshot of all durable voting state.
    /// File-backed WAL databases are copied through SQLite's online backup API
    /// so committed WAL pages are included without changing the source journal.
    pub fn serialize_bytes(&self) -> Result<Vec<u8>, VotingError> {
        let conn = self.conn();
        #[cfg(not(target_arch = "wasm32"))]
        let normalized = normalized_wal_snapshot(&conn)?;
        #[cfg(not(target_arch = "wasm32"))]
        let conn = normalized.as_ref().unwrap_or(&conn);
        conn.serialize(rusqlite::MAIN_DB)
            .map(|bytes| bytes.to_vec())
            .map_err(|e| VotingError::Internal {
                message: format!("failed to serialize voting database: {e}"),
            })
    }

    /// Set the wallet identifier used to scope all subsequent operations.
    /// Panics if this would rebind a database opened with `open_browser`.
    pub fn set_wallet_id(&self, id: &str) {
        if let Some((owner, _)) = &self.browser_owner {
            assert_eq!(owner, id, "wallet owner mismatch");
        }
        *self.wallet_id.lock().expect("wallet_id mutex poisoned") = id.to_string();
    }

    /// Get the current wallet identifier. Panics if not set.
    pub fn wallet_id(&self) -> String {
        let id = self
            .wallet_id
            .lock()
            .expect("wallet_id mutex poisoned")
            .clone();
        assert!(
            !id.is_empty(),
            "wallet_id must be set before performing voting operations"
        );
        id
    }

    /// Get a lock on the underlying connection for query execution.
    pub(crate) fn conn(&self) -> ConnectionGuard<'_> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.conn.lock().expect("database mutex poisoned")
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.conn.borrow_mut()
        }
    }

    pub(crate) fn validate_network(&self, network: Network) -> Result<(), VotingError> {
        if self
            .browser_owner
            .as_ref()
            .is_some_and(|(_, owner_network)| *owner_network != network)
        {
            return Err(VotingError::InvalidInput {
                message: "network mismatch".into(),
            });
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn normalized_wal_snapshot(conn: &Connection) -> Result<Option<Connection>, VotingError> {
    let mode: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .map_err(browser_storage_error)?;
    if mode != "wal" {
        return Ok(None);
    }
    let mut snapshot = Connection::open_in_memory().map_err(browser_storage_error)?;
    let page_size: u32 = conn
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .map_err(browser_storage_error)?;
    // In-memory backup destinations must match the source page size.
    snapshot
        .pragma_update(None, "page_size", page_size)
        .map_err(browser_storage_error)?;
    let result = rusqlite::backup::Backup::new(conn, &mut snapshot)
        .map_err(browser_storage_error)?
        .step(-1)
        .map_err(browser_storage_error)?;
    if result != rusqlite::backup::StepResult::Done {
        return Err(VotingError::Internal {
            message: format!("voting database snapshot did not complete: {result:?}"),
        });
    }
    // Backup copies page-one's WAL format marker as well. VACUUM rebuilds only
    // this private in-memory copy using SQLite's rollback-format writer.
    snapshot
        .execute_batch("VACUUM")
        .map_err(browser_storage_error)?;
    Ok(Some(snapshot))
}

fn browser_storage_error(error: rusqlite::Error) -> VotingError {
    VotingError::Internal {
        message: format!("invalid browser voting database: {error}"),
    }
}

fn validate_browser_identity(
    conn: &Connection,
    wallet_id: &str,
    network: Network,
) -> Result<(), VotingError> {
    // Inspect every wallet-bearing table, including orphaned legacy child rows,
    // before migrations can reset a pre-launch database.
    let mut tables = conn
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table'")
        .map_err(browser_storage_error)?;
    let names = tables
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(browser_storage_error)?;
    for name in names {
        let name = name.map_err(browser_storage_error)?;
        let mut columns = conn
            .prepare("SELECT name FROM pragma_table_info(?1)")
            .map_err(browser_storage_error)?;
        let columns = columns
            .query_map([&name], |r| r.get::<_, String>(0))
            .map_err(browser_storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(browser_storage_error)?;
        let table = name.replace('"', "\"\"");
        for (column, expected, message) in [
            ("wallet_id", wallet_id, "wallet owner mismatch"),
            (
                "network",
                queries::network_to_storage(network),
                "network mismatch",
            ),
        ] {
            if !columns.iter().any(|value| value == column) {
                continue;
            }
            let sql = format!(
                "SELECT EXISTS(SELECT 1 FROM \"{table}\" WHERE {column} != ?1 {} )",
                if column == "wallet_id" && name != "wallet_metadata" {
                    "AND wallet_id != ''"
                } else {
                    ""
                }
            );
            let mismatch: bool = conn
                .query_row(&sql, [expected], |r| r.get(0))
                .map_err(browser_storage_error)?;
            if mismatch {
                return Err(VotingError::InvalidInput {
                    message: message.into(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VotingRoundParams;

    const W: &str = "test-wallet";

    fn test_db() -> VotingDb {
        VotingDb::open(":memory:").unwrap()
    }

    fn test_params() -> VotingRoundParams {
        VotingRoundParams {
            vote_round_id: "test-round-1".to_string(),
            snapshot_height: 1000,
            ea_pk: vec![0xEA; 32],
            nc_root: vec![0xAA; 32],
            nullifier_imt_root: vec![0xBB; 32],
        }
    }

    #[test]
    fn test_open_in_memory() {
        let db = test_db();
        let conn = db.conn();
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 21);
    }

    #[test]
    fn writes_wait_for_a_transient_external_writer() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "zcash-voting-busy-timeout-{}-{unique}.sqlite",
            std::process::id()
        ));
        let path_string = path.to_string_lossy().into_owned();
        let db = VotingDb::open(&path_string).unwrap();
        db.conn()
            .execute_batch("CREATE TABLE busy_timeout_probe (value INTEGER NOT NULL)")
            .unwrap();

        let lock = Connection::open(&path).unwrap();
        lock.busy_timeout(SQLITE_BUSY_TIMEOUT).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            lock.execute_batch("ROLLBACK").unwrap();
        });

        let started = std::time::Instant::now();
        db.conn()
            .execute("INSERT INTO busy_timeout_probe (value) VALUES (1)", [])
            .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(300));

        release.join().unwrap();
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{path_string}-shm"));
        let _ = std::fs::remove_file(format!("{path_string}-wal"));
    }

    #[test]
    fn test_round_lifecycle() {
        let db = test_db();
        let conn = db.conn();
        let params = test_params();

        queries::insert_round(&conn, W, Network::Testnet, &params, None).unwrap();

        let state = queries::get_round_state(&conn, "test-round-1", W).unwrap();
        assert_eq!(state.phase, RoundPhase::Initialized);
        assert_eq!(state.network, Network::Testnet);
        assert_eq!(state.snapshot_height, 1000);
        assert!(!state.proof_generated);

        let rounds = queries::list_rounds(&conn, W).unwrap();
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0].round_id, "test-round-1");
        assert_eq!(rounds[0].network, Network::Testnet);

        queries::clear_round(&conn, "test-round-1", W).unwrap();
        let rounds = queries::list_rounds(&conn, W).unwrap();
        assert!(rounds.is_empty());
    }

    #[test]
    fn test_tree_state_cache() {
        let db = test_db();
        let conn = db.conn();
        queries::insert_round(&conn, W, Network::Testnet, &test_params(), None).unwrap();

        let tree_state = vec![0xCC; 1024];
        queries::store_tree_state(&conn, "test-round-1", W, 1000, &tree_state).unwrap();

        let loaded = queries::load_tree_state(&conn, "test-round-1", W).unwrap();
        assert_eq!(loaded, tree_state);
    }

    #[test]
    fn test_proof_storage() {
        let db = test_db();
        let conn = db.conn();
        queries::insert_round(&conn, W, Network::Testnet, &test_params(), None).unwrap();
        queries::insert_bundle(&conn, "test-round-1", W, 0, &[]).unwrap();
        queries::store_proof(&conn, "test-round-1", W, 0, &vec![0xAB; 256]).unwrap();

        let state = queries::get_round_state(&conn, "test-round-1", W).unwrap();
        assert!(!state.proof_generated, "proof alone should not be enough");

        queries::store_van_position(&conn, "test-round-1", W, 0, 42).unwrap();
        let state = queries::get_round_state(&conn, "test-round-1", W).unwrap();
        assert!(
            state.proof_generated,
            "proof + VAN position should be enough"
        );
    }

    #[test]
    fn test_vote_storage() {
        let db = test_db();
        let conn = db.conn();
        queries::insert_round(&conn, W, Network::Testnet, &test_params(), None).unwrap();
        queries::insert_bundle(&conn, "test-round-1", W, 0, &[]).unwrap();

        let commitment = vec![0xCC; 128];
        queries::store_vote(&conn, "test-round-1", W, 0, 0, 0, &commitment).unwrap();
        queries::store_vote(&conn, "test-round-1", W, 0, 1, 1, &commitment).unwrap();

        queries::record_vote_submission(&conn, "test-round-1", W, 0, 0, "vote-tx").unwrap();
        queries::record_vote_submission(&conn, "test-round-1", W, 0, 0, "vote-tx").unwrap();
        queries::store_vote(&conn, "test-round-1", W, 0, 0, 0, &commitment).unwrap();
        let replace_err =
            queries::store_vote(&conn, "test-round-1", W, 0, 0, 1, &commitment).unwrap_err();
        assert!(
            replace_err
                .to_string()
                .contains("cannot replace submitted vote"),
            "{replace_err}"
        );
        assert_eq!(
            queries::get_vote_tx_hash(&conn, "test-round-1", W, 0, 0).unwrap(),
            Some("vote-tx".to_string())
        );

        let err = queries::record_vote_submission(&conn, "test-round-1", W, 0, 99, "vote-tx")
            .unwrap_err();
        assert!(matches!(err, VotingError::InvalidInput { .. }));
    }

    #[test]
    fn test_get_votes() {
        let db = test_db();
        let conn = db.conn();
        queries::insert_round(&conn, W, Network::Testnet, &test_params(), None).unwrap();
        queries::insert_bundle(&conn, "test-round-1", W, 0, &[]).unwrap();

        let votes = queries::get_votes(&conn, "test-round-1", W).unwrap();
        assert!(votes.is_empty());

        let commitment = vec![0xCC; 128];
        queries::store_vote(&conn, "test-round-1", W, 0, 0, 0, &commitment).unwrap();
        queries::store_vote(&conn, "test-round-1", W, 0, 1, 2, &commitment).unwrap();

        let votes = queries::get_votes(&conn, "test-round-1", W).unwrap();
        assert_eq!(votes.len(), 2);
        assert_eq!(votes[0].proposal_id, 0);
        assert_eq!(votes[0].choice, 0);
        assert_eq!(votes[1].proposal_id, 1);
        assert_eq!(votes[1].choice, 2);

        queries::record_vote_submission(&conn, "test-round-1", W, 0, 0, "vote-tx").unwrap();
        let votes = queries::get_votes(&conn, "test-round-1", W).unwrap();
        assert_eq!(
            queries::get_vote_tx_hash(&conn, "test-round-1", W, 0, 0).unwrap(),
            Some("vote-tx".to_string())
        );
        assert_eq!(votes.len(), 2);
    }

    #[test]
    fn test_wallet_isolation() {
        let db = test_db();
        let conn = db.conn();
        let params = test_params();

        queries::insert_round(&conn, "wallet-a", Network::Testnet, &params, None).unwrap();
        queries::insert_round(&conn, "wallet-b", Network::Testnet, &params, None).unwrap();

        queries::insert_bundle(&conn, "test-round-1", "wallet-a", 0, &[]).unwrap();
        queries::insert_bundle(&conn, "test-round-1", "wallet-b", 0, &[]).unwrap();

        let commitment = vec![0xCC; 128];
        queries::store_vote(&conn, "test-round-1", "wallet-a", 0, 0, 1, &commitment).unwrap();
        queries::store_vote(&conn, "test-round-1", "wallet-b", 0, 0, 2, &commitment).unwrap();

        let votes_a = queries::get_votes(&conn, "test-round-1", "wallet-a").unwrap();
        let votes_b = queries::get_votes(&conn, "test-round-1", "wallet-b").unwrap();
        assert_eq!(votes_a.len(), 1);
        assert_eq!(votes_b.len(), 1);
        assert_eq!(votes_a[0].choice, 1);
        assert_eq!(votes_b[0].choice, 2);

        queries::clear_round(&conn, "test-round-1", "wallet-a").unwrap();
        let rounds_b = queries::list_rounds(&conn, "wallet-b").unwrap();
        assert_eq!(
            rounds_b.len(),
            1,
            "wallet-b round should survive wallet-a clear"
        );
    }
}
