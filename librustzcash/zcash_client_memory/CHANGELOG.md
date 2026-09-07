# Changelog

## [Unreleased]

### Added

- `zcash_client_memory::voting::{VotingSnapshotNote, VotingSnapshotNotes, VotingSnapshot}`.
- `MemoryWalletDb::{voting_snapshot_notes, voting_snapshot}` with the `orchard` feature.

### Fixed

- Voting snapshots consult canonical Ironwood nullifier spends even when a local
  conflicting transaction overwrites the received-note spend association.
