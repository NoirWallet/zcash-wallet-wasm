# Synthetic discovery fixtures

These are public, deterministic test documents, with no real operator keys or
wallet material. The signing seed is Valar's existing `[3; 32]` test seed.

`config::tests::browser_discovery_fixtures_match_valar_helpers` reproduces both
JSON documents with Valar's `static_bytes_v2` and `dynamic_bytes` helpers and
compares their parsed contents. The v2 signature uses `RoundAuthPayloadV2`.

`dynamic-invalid-key.json` and `dynamic-identity-key.json` carry correctly
signed, unusable authority keys. Valar's fixture contract reproduces their
signatures with the same test helper and confirms signature authentication;
trusted-parameter construction must reject the malformed/identity points.

The root fixture transport derives a SHA-256 pin from the checked-in static
bytes. Production selects the fixed stage or production pin from the wallet
network; no caller-facing custom-source API exists.

Valar v2 signs the round ID, authority key, and PIR layout. Service endpoint
lists come from the resolved dynamic document fetched through the pinned
static mirror chain. Roots, timing, proposals, and optional helper descriptors
are service metadata, not signature-bound data. Discovery validates their
shape and pins their complete canonical fingerprint for resume comparison.
Preparation must independently verify snapshot height/root against the wallet
frontier and PIR data before proof work. Helpers retain Valar's configured-URL
identity policy; a descriptor key does not establish operator independence.
