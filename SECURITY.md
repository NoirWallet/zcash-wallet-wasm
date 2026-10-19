# Security Policy

## Report a vulnerability

Please send security reports privately to **support@zknoir.com**. Do not post
exploitable details in a public issue or discussion before coordinated
disclosure. Include the affected source commit or release package hash, build
features, impact, and steps to reproduce with test data where possible.

Do not send real seed phrases, private keys, viewing keys, wallet databases,
or other user data. Redact sensitive values from logs and screenshots. We will
triage reports and coordinate a fix and disclosure with the reporter; this
policy does not promise a fixed response or remediation time.

## Published audit

BlockSec published a [June 9, 2026 audit report for zcash-wallet-wasm](https://github.com/blocksecteam/audit-reports/blob/main/rust/blocksec_noirwallet_zcash_wallet_wasm_signed_20260609.pdf).
The report identifies its own reviewed commits and a limited set of source
files. It does not establish that later source changes, dependencies, the
browser extension, or a deployed WASM package were reviewed. Check the
report's scope against the exact release commit before making an audit claim.

This repository contains the WASM wallet core, not the complete browser
extension. See [BUILDING.md](BUILDING.md) for source and package identity and
[PRIVACY.md](PRIVACY.md) for the host and network trust boundaries.
