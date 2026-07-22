# Transparent One-Click Transfer Design

## Goal

Add explicit one-click transfer APIs that spend an account's transparent balance and support both transparent and shielded recipients without changing the behavior of the existing shielded transfer APIs.

## API Surface

Expose two new `WebWallet` methods:

- `oneclick_pczt_transparent_transfer`
- `oneclick_pczt_transparent_transfer_dynamic_fee`

The fixed-fee method has the same arguments as `oneclick_pczt_transfer`. The dynamic-fee method has the same arguments, including the optional expiry delta, as `oneclick_pczt_transfer_dynamic_fee`.

Both methods parse the destination with `ZcashAddress`, create a PCZT, sign every transparent input with the supplied unified spending key, prove the transaction, and submit it through the existing one-click pipeline.

No existing transfer method changes its public signature or funding policy. No unsigned hardware-wallet API is added in this change.

## Funding Policy

The new methods use a transparent-only spend policy:

```rust
SpendPolicy::shielded_pools([])
    .with_transparent(TransparentSpendPolicy::any_account_addr())
```

This policy may select ordinary, non-coinbase UTXOs from any non-ephemeral transparent receiver belonging to the account. It must not select Sapling, Orchard, or Ironwood notes when the transparent balance is insufficient.

This matches the wallet summary's `unshielded_balance`, which aggregates spendable UTXOs across the account's non-ephemeral transparent receivers. Mature coinbase UTXOs are a documented exception: they can appear in the unshielded balance but are not eligible for the general transfer policy and must continue to use the dedicated coinbase shielding path.

## Recipient And Change Behavior

The existing ZIP-321 payment construction selects the output pool from the destination address:

- A transparent destination produces a transparent payment output.
- A Sapling address or a Unified Address with a shielded receiver produces a shielded payment output in the selected active pool.
- A memo on a transparent payment remains invalid and is rejected by the existing ZIP-321 validation.

The change strategy uses `TransparentChangePolicy::TransparentChangeAllowed`:

- A fully transparent `T -> T` transaction returns change to a newly reserved internal-scope transparent address.
- A `T -> shielded` transaction contains a shielded value flow, so its change remains shielded regardless of the transparent-change setting.
- An exactly balanced transaction produces no change output.

## Wallet Internals

Add fixed-fee and dynamic-fee PCZT creation paths that accept the transparent-only spend policy. Reuse the existing request construction, PCZT creation, one-click signing, proving, and broadcasting behavior. Existing shielded PCZT creation continues to use `SpendPolicy::default()` and shielded change.

The memory wallet backend must implement `WalletWrite::reserve_next_n_internal_addresses`, because the official transparent-change path calls this method while constructing a transaction. Internal addresses must:

- Be derived from the account's internal transparent key scope with sequential non-hardened indices.
- Be reserved without reuse and returned with internal-scope metadata.
- Be discoverable by account and address lookup methods.
- Be included by `get_transparent_receivers` only when change addresses are requested.
- Be recognized as account-owned when received change is scanned, so it contributes to `unshielded_balance`.
- Survive memory-wallet protobuf serialization and deserialization.
- Observe the backend's internal transparent-address gap limit and return the existing reached-gap-limit error when exhausted.

The memory backend changes are local integration work. They are not intended to be submitted as an upstream `librustzcash` pull request from this repository.

## Errors

The new APIs preserve existing error categories and add no JavaScript error type:

- Invalid destination or invalid transparent memo: address/ZIP-321 error.
- No eligible transparent UTXOs or insufficient transparent value: insufficient-funds proposal error wrapped by the existing wallet error surface.
- Internal change address exhaustion: memory-backend gap-limit error wrapped by the wallet error surface.
- Signing, proving, or broadcast failures: existing PCZT one-click errors.

The insufficient-funds message should identify the operation as a transparent transfer instead of suggesting that shielded balances may be used.

## Testing

Tests must cover the behavior at the lowest practical layer and the exported integration surface:

1. A transparent-only spend policy permits transparent UTXOs and no shielded pools.
2. A `T -> T` proposal selects transparent inputs and produces internal transparent change.
3. A `T -> shielded` proposal selects transparent inputs and produces the requested shielded output.
4. Insufficient transparent funds do not fall back to Sapling, Orchard, or Ironwood notes.
5. Internal change addresses are sequential, discoverable, scoped as internal, and preserved by wallet serialization.
6. Transparent PCZT inputs are signed by the existing one-click signer.
7. Both fixed-fee and dynamic-fee WASM methods compile and appear in the generated bindings after rebuilding the package.

Verification includes focused memory-backend tests, root Rust tests, formatting, a WASM target check, and rebuilding the tracked `pkg` artifacts.
