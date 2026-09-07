CREATE TABLE delegation_attempts (
    round_id TEXT NOT NULL,
    wallet_id TEXT NOT NULL,
    bundle_index INTEGER NOT NULL,
    canonical_json BLOB NOT NULL,
    tx_hash TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('prepared','dispatching','accepted','ambiguous','rejected')),
    PRIMARY KEY (round_id,wallet_id,bundle_index),
    FOREIGN KEY (round_id,wallet_id,bundle_index)
        REFERENCES bundles(round_id,wallet_id,bundle_index) ON DELETE CASCADE
);
