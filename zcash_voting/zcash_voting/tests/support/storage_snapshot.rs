use rusqlite::{Connection, MAIN_DB};
use zcash_voting::{round::VotingDb, types::Network};

pub fn snapshot(db: &VotingDb) -> Connection {
    let bytes = db.serialize_bytes().unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    conn.deserialize_read_exact(MAIN_DB, std::io::Cursor::new(&bytes), bytes.len(), false)
        .unwrap();
    conn
}

pub fn restore(conn: Connection, wallet_id: &str) -> VotingDb {
    VotingDb::open_browser(
        wallet_id,
        Network::Testnet,
        Some(&conn.serialize(MAIN_DB).unwrap()),
    )
    .unwrap()
}
