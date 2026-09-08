ALTER TABLE delegation_attempts ADD COLUMN confirmation_tx_hash TEXT;
ALTER TABLE delegation_attempts ADD COLUMN confirmation_van_position INTEGER
    CHECK (confirmation_van_position IS NULL OR confirmation_van_position BETWEEN 0 AND 4294967295);
