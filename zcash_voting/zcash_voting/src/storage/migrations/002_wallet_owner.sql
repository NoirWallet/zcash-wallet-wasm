CREATE TABLE wallet_metadata (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    wallet_id TEXT NOT NULL,
    network TEXT NOT NULL CHECK (network IN ('mainnet', 'testnet', 'regtest'))
);
