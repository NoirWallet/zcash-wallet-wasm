CREATE TABLE IF NOT EXISTS vote_attempts (
    round_id                       TEXT NOT NULL,
    wallet_id                      TEXT NOT NULL,
    bundle_index                   INTEGER NOT NULL,
    anchor_proposal_id             INTEGER NOT NULL,
    canonical_json                 BLOB NOT NULL,
    tx_hash                        TEXT NOT NULL,
    proposal_ids_json              TEXT NOT NULL,
    batch_digest                   BLOB,
    outcome                        TEXT NOT NULL CHECK (outcome IN ('prepared','dispatching','accepted','ambiguous','rejected')),
    confirmation_tx_hash           TEXT,
    confirmation_van_position      INTEGER CHECK (confirmation_van_position IS NULL OR confirmation_van_position BETWEEN 0 AND 4294967295),
    confirmation_vc_positions_json TEXT,
    PRIMARY KEY (round_id, wallet_id, bundle_index, anchor_proposal_id),
    FOREIGN KEY (round_id, wallet_id, bundle_index, anchor_proposal_id)
        REFERENCES votes(round_id, wallet_id, bundle_index, proposal_id) ON DELETE CASCADE
);
