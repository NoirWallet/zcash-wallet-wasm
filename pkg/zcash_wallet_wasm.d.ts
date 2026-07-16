/* tslint:disable */
/* eslint-disable */
/**
 * The `ReadableStreamType` enum.
 *
 * *This API requires the following crate features to be activated: `ReadableStreamType`*
 */

export type ReadableStreamType = "bytes";

export class BlockRange {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    0: number;
    1: number;
}

export class IntoUnderlyingByteSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableByteStreamController): Promise<any>;
    start(controller: ReadableByteStreamController): void;
    readonly autoAllocateChunkSize: number;
    readonly type: ReadableStreamType;
}

export class IntoUnderlyingSink {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    abort(reason: any): Promise<any>;
    close(): Promise<any>;
    write(chunk: any): Promise<any>;
}

export class IntoUnderlyingSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableStreamDefaultController): Promise<any>;
}

export class PaymentRequest {
    free(): void;
    [Symbol.dispose](): void;
    amount(): bigint;
    label(): string | undefined;
    memo(): Uint8Array | undefined;
    message(): string | undefined;
    constructor(recipient_address: string, amount: bigint, memo: Uint8Array | null | undefined, label: string | null | undefined, message: string | null | undefined, other_params: any);
    other_params(): any;
    recipient_address(): string;
    static simple_payment(recipient_address: string, amount: bigint): PaymentRequest;
}

export class Pczt {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Combines this PCZT with another representing the same transaction.
     *
     * Used after external signing: `self` is the full PCZT kept by the wallet
     * (with witnesses) and `other` is the redacted PCZT returned by the signer
     * (with signatures). The result carries both, ready for proving and
     * extraction.
     */
    combine(other: Pczt): Pczt;
    /**
     * Deserialize to a Pczt from postcard bytes.
     */
    static from_bytes(bytes: Uint8Array): Pczt;
    /**
     * Returns a Pczt from a JSON object
     */
    static from_json(s: any): Pczt;
    /**
     * Strips prover-only data so the PCZT is small enough to transport to an
     * external signer (e.g. Keystone) as an animated QR.
     *
     * Mirrors the "redact for signer" role used by Zashi: removes spend
     * witnesses (the per-note Merkle authentication paths that dominate the
     * serialized size) and the `zcash_client_backend` proprietary metadata.
     * The signer only needs the spend randomizers, values and derivation
     * paths, so the redacted PCZT still produces valid signatures. The
     * original (full) PCZT must be retained and merged back with [`combine`]
     * after signing to restore the witnesses required for proving.
     *
     * [`combine`]: Pczt::combine
     */
    redact_for_signer(): Pczt;
    /**
     * Returns the postcard serialization of the Pczt.
     */
    serialize(): Uint8Array;
    /**
     * Returns a JSON object with the details of the Pczt.
     */
    to_json(): any;
}

export class ProofGenerationKey {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
}

export class Proposal {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
}

export class SeedFingerprint {
    free(): void;
    [Symbol.dispose](): void;
    static from_bytes(bytes: Uint8Array): SeedFingerprint;
    constructor(seed: Uint8Array);
    to_bytes(): Uint8Array;
}

export class TransactionRequest {
    free(): void;
    [Symbol.dispose](): void;
    static empty(): TransactionRequest;
    static from_uri(uri: string): TransactionRequest;
    constructor(payments: PaymentRequest[]);
    payment_requests(): PaymentRequest[];
    to_uri(): string;
    total(): bigint;
}

/**
 * A Zcash viewing key
 *
 * This is a wrapper around the `zcash_keys::keys::ViewingKey` type.
 * UFVKs should be generated from a spending key by calling `to_unified_full_viewing_key`
 * They can also be encoded and decoded to a canonical string representation
 */
export class UnifiedFullViewingKey {
    free(): void;
    [Symbol.dispose](): void;
    encode(network: string): string;
    constructor(network: string, encoding: string);
}

export class UnifiedSpendingKey {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Construct a new UnifiedSpendingKey
     *
     * # Arguments
     *
     * * `network` - Must be either "main" or "test"
     * * `seed` - At least 32 bytes of entry. Care should be taken as to how this is derived
     * * `hd_index` - [ZIP32](https://zips.z.cash/zip-0032) hierarchical deterministic index of the account
     */
    constructor(network: string, seed: Uint8Array, hd_index: number);
    to_sapling_proof_generation_key(): ProofGenerationKey;
    /**
     * Obtain the UFVK corresponding to this spending key
     */
    to_unified_full_viewing_key(): UnifiedFullViewingKey;
}

export class WalletSummary {
    private constructor();
    /**
     ** Return copy of self without private attributes.
     */
    toJSON(): Object;
    /**
     * Return stringified version of self.
     */
    toString(): string;
    free(): void;
    [Symbol.dispose](): void;
    chain_tip_height: number;
    fully_scanned_height: number;
    next_ironwood_subtree_index: bigint;
    next_orchard_subtree_index: bigint;
    next_sapling_subtree_index: bigint;
    readonly account_balances: any;
}

export class WebWallet {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Benchmark helper: download a block range over gRPC-web without scanning.
     * Returns the number of blocks fetched. Used to probe download throughput.
     */
    bench_download_range(start_height: number, count: number): Promise<number>;
    /**
     * Benchmark helper: one baseline journal batch with per-stage timings (ms)
     * as JSON `{"subtree","setup","download","scan"}`, or undefined when synced.
     * When `refresh` is false, session setup is skipped (call
     * `sync_session_setup` once first).
     */
    bench_prepare_sync_batch_timed(batch_size: number | null | undefined, refresh: boolean): Promise<string | undefined>;
    create_account(account_name: string, seed_phrase: string, account_hd_index: number, birthday_height?: number | null, passphrase?: string | null): Promise<number>;
    /**
     * Import a watch-only spending account from an encoded UFVK (hardware wallet flow).
     *
     * # Arguments
     * * `account_name` - Display name for the account
     * * `ufvk` - The encoded Unified Full Viewing Key exported by the device
     * * `seed_fingerprint` - 32-byte ZIP-32 seed fingerprint exported by the device
     * * `account_hd_index` - ZIP-32 account index on the device
     * * `birthday_height` - Block height to start scanning from
     * * `key_source` - Optional tag describing the key origin (e.g. "keystone")
     *
     * # Returns
     * The new account id
     */
    create_account_ufvk(account_name: string, ufvk: string, seed_fingerprint: Uint8Array, account_hd_index: number, birthday_height?: number | null, key_source?: string | null): Promise<number>;
    db_to_bytes(): Promise<Uint8Array>;
    db_to_bytes_chunked(callback: Function): Promise<void>;
    estimate_orchard_migration_fee(account_id: number, to_address: string, memo: Uint8Array | null | undefined, marginal_fee_zats: bigint): Promise<bigint>;
    estimate_pczt_shield_fee(account_id: number, marginal_fee_zats: bigint): Promise<bigint>;
    estimate_pczt_transfer_fee(account_id: number, to_address: string, value: bigint, memo: Uint8Array | null | undefined, marginal_fee_zats: bigint): Promise<bigint>;
    /**
     * Get the birthday height for a specific account
     *
     * # Arguments
     *
     * * `account_id` - The account ID to get the birthday height for
     *
     * # Returns
     *
     * The birthday height (block height) for the specified account
     *
     * # Example
     *
     * ```javascript
     * const accountId = 0;
     * const birthdayHeight = await wallet.get_account_birthday(accountId);
     * console.log(`Account ${accountId} birthday height: ${birthdayHeight}`);
     * ```
     */
    get_account_birthday(account_id: number): Promise<number>;
    /**
     * Get the HD account index for an account.
     *
     * This returns the ZIP-32 account index that was used to derive this account's keys.
     * Returns None if the account was imported and doesn't have derivation info.
     *
     * # Arguments
     *
     * * `account_id` - The account ID
     *
     * # Returns
     *
     * The HD account index (u32), or None if not available
     */
    get_account_hd_index(account_id: number): Promise<number | undefined>;
    /**
     * Get account ID by account name
     *
     * # Arguments
     *
     * * `account_name` - The name of the account to find
     *
     * # Returns
     *
     * Returns the account ID as u32 if found
     *
     * # Example
     *
     * ```javascript
     * const accountId = await wallet.get_account_id_by_name("MyAccount");
     * console.log("Account ID:", accountId);
     * ```
     */
    get_account_id_by_name(account_name: string): Promise<number>;
    /**
     * Find the block height closest to a given Unix timestamp
     *
     * Uses binary search to efficiently locate the block height corresponding
     * to a specific time. Useful for determining wallet birthday heights based
     * on dates.
     *
     * # Arguments
     *
     * * `time` - Unix timestamp (seconds since epoch)
     *
     * # Returns
     *
     * Block height closest to the given timestamp (within ~1000 blocks precision)
     *
     * # Example
     *
     * ```javascript
     * // Find block height for January 1, 2024
     * const timestamp = Math.floor(new Date('2024-01-01').getTime() / 1000);
     * const height = await wallet.get_block_by_time(timestamp);
     * console.log(`Block height at 2024-01-01: ${height}`);
     *
     * // Use as birthday height when importing a wallet
     * const birthdayTimestamp = Math.floor(new Date('2023-06-15').getTime() / 1000);
     * const birthdayHeight = await wallet.get_block_by_time(birthdayTimestamp);
     * ```
     */
    get_block_by_time(time: number): Promise<number>;
    /**
     * Get the Unix timestamp of a block at a given height
     *
     * # Arguments
     *
     * * `height` - The block height
     *
     * # Returns
     *
     * Unix timestamp (seconds since epoch) when the block was mined
     *
     * # Example
     *
     * ```javascript
     * const height = 2855000;
     * const timestamp = await wallet.get_block_time(height);
     * const date = new Date(timestamp * 1000);
     * console.log(`Block ${height} was mined at: ${date}`);
     * ```
     */
    get_block_time(height: number): Promise<number>;
    /**
     * Returns the configured NU6.3 activation height for the selected network.
     */
    get_ironwood_activation_height(): number | undefined;
    get_latest_block(): Promise<bigint>;
    /**
     * Get the latest block height from the blockchain
     *
     * # Returns
     *
     * The current chain tip height
     *
     * # Example
     *
     * ```javascript
     * const latestHeight = await wallet.get_latest_height();
     * console.log(`Current blockchain height: ${latestHeight}`);
     * ```
     */
    get_latest_height(): Promise<number>;
    /**
     * Generate and persist the next shielded receiving address for an account.
     *
     * This returns a unified address containing the account's available shielded receivers
     * (Orchard/Sapling) and no transparent receiver. It advances the wallet's stored address
     * position, so callers should persist the wallet DB after using this method.
     *
     * # Arguments
     *
     * * `account_id` - The account ID to generate the next shielded address for
     *
     * # Returns
     *
     * Returns a UnifiedAddresses object. `transparent` and `address_index` will be null.
     */
    get_next_shielded_address(account_id: number): Promise<any>;
    get_transaction_history(account_id: number): Promise<any>;
    /**
     * Get addresses for an account
     *
     * # Arguments
     *
     * * `account_id` - The account ID to get addresses for
     *
     * # Returns
     *
     * Returns a UnifiedAddresses object containing:
     * - unified: The shielded unified address (u1 format with Orchard/Sapling receivers only)
     * - orchard: Orchard-only unified address (if available)
     * - sapling: Sapling-only unified address (u1 format, if available)
     * - sapling_zs: Sapling traditional address (zs format, if available)
     * - transparent: Transparent P2PKH address (if available)
     *
     * # Example
     *
     * ```javascript
     * const addresses = await wallet.get_unified_addresses(accountId);
     * console.log("Unified Address:", addresses.unified);
     * console.log("Sapling (zs):", addresses.sapling_zs);
     * console.log("Transparent:", addresses.transparent);
     * ```
     */
    get_unified_addresses(account_id: number): Promise<any>;
    get_wallet_summary(): Promise<WalletSummary | undefined>;
    constructor(network: string, lightwalletd_url: string, db_bytes?: Uint8Array | null);
    oneclick_pczt_orchard_migration(account_id: number, to_address: string, usk: UnifiedSpendingKey, seed_fp: SeedFingerprint, sapling_proof_gen_key: ProofGenerationKey | null | undefined, memo: Uint8Array | null | undefined, marginal_fee_zats: bigint, expiry_delta_blocks?: number | null): Promise<string>;
    oneclick_pczt_shield(account_id: number, usk: UnifiedSpendingKey, seed_fp: SeedFingerprint, sapling_proof_gen_key?: ProofGenerationKey | null): Promise<string>;
    oneclick_pczt_shield_dynamic_fee(account_id: number, usk: UnifiedSpendingKey, seed_fp: SeedFingerprint, sapling_proof_gen_key: ProofGenerationKey | null | undefined, marginal_fee_zats: bigint, expiry_delta_blocks?: number | null): Promise<string>;
    oneclick_pczt_transfer(account_id: number, to_address: string, value: bigint, usk: UnifiedSpendingKey, seed_fp: SeedFingerprint, sapling_proof_gen_key?: ProofGenerationKey | null, memo?: Uint8Array | null): Promise<string>;
    oneclick_pczt_transfer_dynamic_fee(account_id: number, to_address: string, value: bigint, usk: UnifiedSpendingKey, seed_fp: SeedFingerprint, sapling_proof_gen_key: ProofGenerationKey | null | undefined, memo: Uint8Array | null | undefined, marginal_fee_zats: bigint, expiry_delta_blocks?: number | null): Promise<string>;
    /**
     * Create an unsigned, unproven PCZT for a transfer (hardware wallet flow).
     *
     * The returned PCZT carries ZIP-32 derivation metadata so an external signer
     * (e.g. Keystone) can locate its keys. Serialize it with `Pczt.serialize()`
     * before transporting via QR. `marginal_fee_zats` selects the ZIP-317 fee
     * tier so hardware sends honor the same dynamic-fee tiers as software sends.
     */
    pczt_create(account_id: number, to_address: string, value: bigint, memo: Uint8Array | null | undefined, marginal_fee_zats: bigint): Promise<Pczt>;
    /**
     * Create a V6 PCZT that sweeps spendable Orchard funds to this account's Ironwood pool.
     */
    pczt_orchard_migration_create(account_id: number, to_address: string, memo: Uint8Array | null | undefined, marginal_fee_zats: bigint, expiry_delta_blocks?: number | null): Promise<Pczt>;
    /**
     * Prove and broadcast an externally signed PCZT (hardware wallet flow).
     *
     * Hardware-wallet accounts never hold Sapling notes (Keystone supports
     * transparent + Orchard only), so no Sapling proof generation key is needed.
     *
     * # Returns
     * Transaction ID string on success
     */
    pczt_prove_and_send(pczt: Pczt): Promise<string>;
    /**
     * Create an unsigned, unproven shielding PCZT (transparent -> Orchard) for
     * external signing (hardware wallet flow). `marginal_fee_zats` selects the
     * ZIP-317 fee tier so hardware shields honor the dynamic-fee tiers too.
     */
    pczt_shield_create(account_id: number, marginal_fee_zats: bigint): Promise<Pczt>;
    /**
     * Journal-aware sync: download + scan one batch, return journal data for persistence.
     * Returns null if fully synced, or Uint8Array with journal entry data.
     * When `refresh` is false, session setup is skipped (call `sync_session_setup`
     * once at the start of the catch-up session first).
     */
    prepare_sync_batch(batch_size: number | null | undefined, refresh: boolean): Promise<Uint8Array | undefined>;
    /**
     * Optimized journal-aware sync: same as [`prepare_sync_batch`] but uses
     * External-only IVK scanning for contiguous forward ranges.
     */
    prepare_sync_batch_optimized(batch_size?: number | null): Promise<Uint8Array | undefined>;
    /**
     * Replay multiple journal entries in sequence (for recovery).
     */
    replay_journal_entries(entries: Uint8Array[]): Promise<void>;
    /**
     * Scan a journal entry for recovery (replay from base snapshot + journal).
     * Returns true if more scan ranges remain.
     */
    scan_journal_entry(journal_data: Uint8Array): Promise<boolean>;
    send_authorized_transactions(txids: Uint8Array): Promise<void>;
    sync(): Promise<void>;
    /**
     * Runs a single batch of the sync process without looping.
     * Returns `true` if a scan range was processed, `false` if no scan ranges were suggested.
     */
    sync_next_batch(batch_size?: number | null): Promise<boolean>;
    /**
     * Runs a single batch using the optimized scanning strategy.
     * Returns `true` if a scan range was processed, `false` if no scan ranges were suggested.
     */
    sync_next_batch_optimized(batch_size?: number | null): Promise<boolean>;
    /**
     * Run session-level setup (subtree roots, chain tip, transparent UTXOs)
     * once before a multi-batch catch-up so subsequent batches can skip it.
     */
    sync_session_setup(): Promise<void>;
}

/**
 * Generate a new 24-word BIP-39 mnemonic phrase.
 *
 * # Arguments
 *
 * * `word_count` - Number of words in the mnemonic. Must be 24 for new wallet generation.
 *
 * # Returns
 *
 * A string containing the mnemonic phrase
 *
 * # Example
 *
 * ```javascript
 * const mnemonic = generate_mnemonic(24);
 * console.log(mnemonic); // "abandon abandon abandon ... art"
 * ```
 */
export function generate_mnemonic(word_count: number): string;

export function initThreadPool(num_threads: number): Promise<any>;

/**
 * Get all addresses (shielded unified, orchard, sapling, transparent) from a mnemonic phrase.
 *
 * # Arguments
 *
 * * `network` - Network name ("main" or "test")
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string "" if none)
 * * `account_index` - The account index for HD derivation (usually 0)
 *
 * # Returns
 *
 * A JSON object containing all address types
 *
 * # Example
 *
 * ```javascript
 * const mnemonic = generate_mnemonic(24);
 * const addresses = mnemonic_to_addresses(
 *     "main",
 *     mnemonic,
 *     "",  // no passphrase
 *     0    // first account
 * );
 * console.log("Unified:", addresses.unified);
 * console.log("Orchard:", addresses.orchard);
 * console.log("Sapling:", addresses.sapling);
 * console.log("Transparent:", addresses.transparent);
 * ```
 */
export function mnemonic_to_addresses(network: string, phrase: string, passphrase: string, account_index: number): any;

/**
 * Convert a mnemonic phrase to a seed with an optional passphrase.
 *
 * # Arguments
 *
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string for no passphrase)
 *
 * # Returns
 *
 * A 64-byte seed that can be used for key derivation
 *
 * # Example
 *
 * ```javascript
 * const seed = mnemonic_to_seed("abandon abandon abandon ... art", "");
 * console.log(seed); // Uint8Array of 64 bytes
 * ```
 */
export function mnemonic_to_seed(phrase: string, passphrase: string): Uint8Array;

/**
 * Generate a SeedFingerprint from a mnemonic phrase.
 *
 * # Arguments
 *
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string "" if none)
 *
 * # Returns
 *
 * A SeedFingerprint that can be used for key derivation path identification
 *
 * # Example
 *
 * ```javascript
 * const mnemonic = generate_mnemonic(24);
 * const seed_fp = mnemonic_to_seed_fingerprint(
 *     mnemonic,
 *     ""  // no passphrase
 * );
 * ```
 */
export function mnemonic_to_seed_fingerprint(phrase: string, passphrase: string): SeedFingerprint;

/**
 * Generate a UnifiedSpendingKey from a mnemonic phrase.
 *
 * # Arguments
 *
 * * `network` - Network name ("main" or "test")
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string "" if none)
 * * `account_index` - The account index for HD derivation (usually 0)
 *
 * # Returns
 *
 * A UnifiedSpendingKey that can be used for signing transactions
 *
 * # Example
 *
 * ```javascript
 * const mnemonic = generate_mnemonic(24);
 * const usk = mnemonic_to_spending_key(
 *     "main",
 *     mnemonic,
 *     "",  // no passphrase
 *     0    // first account
 * );
 * ```
 */
export function mnemonic_to_spending_key(network: string, phrase: string, passphrase: string, account_index: number): UnifiedSpendingKey;

/**
 * Get a transparent address and public key from a mnemonic at a specific external address index.
 *
 * # Arguments
 *
 * * `network` - Network name ("main" or "test")
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string "" if none)
 * * `account_index` - The account index for HD derivation (usually 0)
 * * `address_index` - The external transparent child address index
 */
export function mnemonic_to_transparent_address(network: string, phrase: string, passphrase: string, account_index: number, address_index: number): any;

/**
 * Parse a unified address string and extract individual receiver addresses.
 *
 * # Arguments
 *
 * * `network` - Network name ("main" or "test")
 * * `unified_address` - The unified address string to parse
 *
 * # Returns
 *
 * A JSON object containing individual address types extracted from the unified address
 *
 * # Example
 *
 * ```javascript
 * const addresses = parse_unified_address(
 *     "main",
 *     "u1..." // unified address string
 * );
 * console.log("Orchard:", addresses.orchard);
 * console.log("Sapling:", addresses.sapling);
 * console.log("Transparent:", addresses.transparent);
 * ```
 */
export function parse_unified_address(network: string, unified_address: string): any;

/**
 * Sign a message with the transparent address private key
 *
 * This implements the Bitcoin/Zcash message signing convention compatible with
 * the `signmessage` RPC call.
 *
 * # Arguments
 *
 * * `network` - Network type ("main" or "test")
 * * `usk` - The Unified Spending Key
 * * `message` - The message to sign
 * * `address_index` - Optional external transparent address index to sign with. If omitted, the default full unified address index is used.
 *
 * # Returns
 *
 * Returns a JSON object with:
 * - signature: Hex-encoded signature (65 bytes: recovery_id + r + s)
 * - pubkey: Hex-encoded uncompressed public key (65 bytes: 04 + x + y)
 * - address: The transparent address used for signing
 *
 * # Example
 *
 * ```javascript
 * const result = await sign_message_with_transparent("main", usk, "Hello Zcash!", undefined);
 * console.log("Signature:", result.signature);  // hex format
 * console.log("Public Key:", result.pubkey);     // uncompressed hex (65 bytes)
 * console.log("Address:", result.address);       // t-address
 * ```
 */
export function sign_message_with_transparent(network: string, usk: UnifiedSpendingKey, message: string, address_index?: number | null): any;

export function start(): void;

/**
 * Validate a BIP-39 mnemonic phrase.
 *
 * # Arguments
 *
 * * `phrase` - The mnemonic phrase to validate
 *
 * # Returns
 *
 * Returns Ok(()) if valid, or an error if invalid
 *
 * # Example
 *
 * ```javascript
 * try {
 *     validate_mnemonic("abandon abandon abandon ... art");
 *     console.log("Valid mnemonic");
 * } catch (e) {
 *     console.error("Invalid mnemonic:", e);
 * }
 * ```
 */
export function validate_mnemonic(phrase: string): void;

export class wbg_rayon_PoolBuilder {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    build(): void;
    numThreads(): number;
    receiver(): number;
}

export function wbg_rayon_start_worker(receiver: number): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly __wbg_seedfingerprint_free: (a: number, b: number) => void;
    readonly seedfingerprint_new: (a: number, b: number, c: number) => void;
    readonly seedfingerprint_to_bytes: (a: number, b: number) => void;
    readonly seedfingerprint_from_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_proofgenerationkey_free: (a: number, b: number) => void;
    readonly __wbg_unifiedspendingkey_free: (a: number, b: number) => void;
    readonly unifiedspendingkey_new: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly unifiedspendingkey_to_unified_full_viewing_key: (a: number) => number;
    readonly unifiedspendingkey_to_sapling_proof_generation_key: (a: number) => number;
    readonly __wbg_unifiedfullviewingkey_free: (a: number, b: number) => void;
    readonly unifiedfullviewingkey_encode: (a: number, b: number, c: number, d: number) => void;
    readonly unifiedfullviewingkey_new: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly generate_mnemonic: (a: number, b: number) => void;
    readonly validate_mnemonic: (a: number, b: number, c: number) => void;
    readonly mnemonic_to_seed: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly mnemonic_to_spending_key: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly mnemonic_to_seed_fingerprint: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly mnemonic_to_transparent_address: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly mnemonic_to_addresses: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly parse_unified_address: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly __wbg_pczt_free: (a: number, b: number) => void;
    readonly pczt_to_json: (a: number, b: number) => void;
    readonly pczt_from_json: (a: number, b: number) => void;
    readonly pczt_serialize: (a: number, b: number) => void;
    readonly pczt_from_bytes: (a: number, b: number, c: number) => void;
    readonly pczt_redact_for_signer: (a: number) => number;
    readonly pczt_combine: (a: number, b: number, c: number) => void;
    readonly __wbg_transactionrequest_free: (a: number, b: number) => void;
    readonly transactionrequest_new: (a: number, b: number, c: number) => void;
    readonly transactionrequest_empty: () => number;
    readonly transactionrequest_payment_requests: (a: number, b: number) => void;
    readonly transactionrequest_total: (a: number, b: number) => void;
    readonly transactionrequest_from_uri: (a: number, b: number, c: number) => void;
    readonly transactionrequest_to_uri: (a: number, b: number) => void;
    readonly __wbg_paymentrequest_free: (a: number, b: number) => void;
    readonly paymentrequest_new: (a: number, b: number, c: number, d: bigint, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly paymentrequest_simple_payment: (a: number, b: number, c: number, d: bigint) => void;
    readonly paymentrequest_recipient_address: (a: number, b: number) => void;
    readonly paymentrequest_amount: (a: number) => bigint;
    readonly paymentrequest_memo: (a: number, b: number) => void;
    readonly paymentrequest_label: (a: number, b: number) => void;
    readonly paymentrequest_message: (a: number, b: number) => void;
    readonly paymentrequest_other_params: (a: number, b: number) => void;
    readonly sign_message_with_transparent: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly __wbg_webwallet_free: (a: number, b: number) => void;
    readonly webwallet_new: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly webwallet_create_account: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => number;
    readonly webwallet_sync: (a: number) => number;
    readonly webwallet_sync_next_batch: (a: number, b: number) => number;
    readonly webwallet_sync_next_batch_optimized: (a: number, b: number) => number;
    readonly webwallet_get_wallet_summary: (a: number) => number;
    readonly webwallet_get_account_birthday: (a: number, b: number) => number;
    readonly webwallet_get_block_time: (a: number, b: number) => number;
    readonly webwallet_get_latest_height: (a: number) => number;
    readonly webwallet_get_ironwood_activation_height: (a: number) => number;
    readonly webwallet_get_block_by_time: (a: number, b: number) => number;
    readonly webwallet_get_transaction_history: (a: number, b: number) => number;
    readonly webwallet_prepare_sync_batch: (a: number, b: number, c: number) => number;
    readonly webwallet_bench_download_range: (a: number, b: number, c: number) => number;
    readonly webwallet_sync_session_setup: (a: number) => number;
    readonly webwallet_bench_prepare_sync_batch_timed: (a: number, b: number, c: number) => number;
    readonly webwallet_prepare_sync_batch_optimized: (a: number, b: number) => number;
    readonly webwallet_scan_journal_entry: (a: number, b: number, c: number) => number;
    readonly webwallet_replay_journal_entries: (a: number, b: number, c: number) => number;
    readonly webwallet_db_to_bytes: (a: number) => number;
    readonly webwallet_db_to_bytes_chunked: (a: number, b: number) => number;
    readonly webwallet_send_authorized_transactions: (a: number, b: number, c: number) => number;
    readonly webwallet_get_account_id_by_name: (a: number, b: number, c: number) => number;
    readonly webwallet_get_account_hd_index: (a: number, b: number) => number;
    readonly webwallet_get_unified_addresses: (a: number, b: number) => number;
    readonly webwallet_get_next_shielded_address: (a: number, b: number) => number;
    readonly webwallet_oneclick_pczt_transfer: (a: number, b: number, c: number, d: number, e: bigint, f: number, g: number, h: number, i: number, j: number) => number;
    readonly webwallet_oneclick_pczt_shield: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly webwallet_create_account_ufvk: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => number;
    readonly webwallet_pczt_create: (a: number, b: number, c: number, d: number, e: bigint, f: number, g: number, h: bigint) => number;
    readonly webwallet_pczt_shield_create: (a: number, b: number, c: bigint) => number;
    readonly webwallet_pczt_orchard_migration_create: (a: number, b: number, c: number, d: number, e: number, f: number, g: bigint, h: number) => number;
    readonly webwallet_oneclick_pczt_orchard_migration: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: bigint, k: number) => number;
    readonly webwallet_estimate_orchard_migration_fee: (a: number, b: number, c: number, d: number, e: number, f: number, g: bigint) => number;
    readonly webwallet_pczt_prove_and_send: (a: number, b: number) => number;
    readonly webwallet_oneclick_pczt_transfer_dynamic_fee: (a: number, b: number, c: number, d: number, e: bigint, f: number, g: number, h: number, i: number, j: number, k: bigint, l: number) => number;
    readonly webwallet_oneclick_pczt_shield_dynamic_fee: (a: number, b: number, c: number, d: number, e: number, f: bigint, g: number) => number;
    readonly webwallet_estimate_pczt_transfer_fee: (a: number, b: number, c: number, d: number, e: bigint, f: number, g: number, h: bigint) => number;
    readonly webwallet_estimate_pczt_shield_fee: (a: number, b: number, c: bigint) => number;
    readonly webwallet_get_latest_block: (a: number) => number;
    readonly __wbg_proposal_free: (a: number, b: number) => void;
    readonly __wbg_walletsummary_free: (a: number, b: number) => void;
    readonly __wbg_get_walletsummary_chain_tip_height: (a: number) => number;
    readonly __wbg_set_walletsummary_chain_tip_height: (a: number, b: number) => void;
    readonly __wbg_get_walletsummary_fully_scanned_height: (a: number) => number;
    readonly __wbg_set_walletsummary_fully_scanned_height: (a: number, b: number) => void;
    readonly __wbg_get_walletsummary_next_sapling_subtree_index: (a: number) => bigint;
    readonly __wbg_set_walletsummary_next_sapling_subtree_index: (a: number, b: bigint) => void;
    readonly __wbg_get_walletsummary_next_orchard_subtree_index: (a: number) => bigint;
    readonly __wbg_set_walletsummary_next_orchard_subtree_index: (a: number, b: bigint) => void;
    readonly __wbg_get_walletsummary_next_ironwood_subtree_index: (a: number) => bigint;
    readonly __wbg_set_walletsummary_next_ironwood_subtree_index: (a: number, b: bigint) => void;
    readonly walletsummary_account_balances: (a: number, b: number) => void;
    readonly start: () => void;
    readonly __wbg_blockrange_free: (a: number, b: number) => void;
    readonly __wbg_get_blockrange_0: (a: number) => number;
    readonly __wbg_set_blockrange_0: (a: number, b: number) => void;
    readonly __wbg_get_blockrange_1: (a: number) => number;
    readonly __wbg_set_blockrange_1: (a: number, b: number) => void;
    readonly rustsecp256k1_v0_8_1_context_create: (a: number) => number;
    readonly rustsecp256k1_v0_8_1_context_destroy: (a: number) => void;
    readonly rustsecp256k1_v0_8_1_default_illegal_callback_fn: (a: number, b: number) => void;
    readonly rustsecp256k1_v0_8_1_default_error_callback_fn: (a: number, b: number) => void;
    readonly __wbg_wbg_rayon_poolbuilder_free: (a: number, b: number) => void;
    readonly wbg_rayon_poolbuilder_numThreads: (a: number) => number;
    readonly wbg_rayon_poolbuilder_receiver: (a: number) => number;
    readonly wbg_rayon_poolbuilder_build: (a: number) => void;
    readonly initThreadPool: (a: number) => number;
    readonly wbg_rayon_start_worker: (a: number) => void;
    readonly __wbg_intounderlyingbytesource_free: (a: number, b: number) => void;
    readonly intounderlyingbytesource_type: (a: number) => number;
    readonly intounderlyingbytesource_autoAllocateChunkSize: (a: number) => number;
    readonly intounderlyingbytesource_start: (a: number, b: number) => void;
    readonly intounderlyingbytesource_pull: (a: number, b: number) => number;
    readonly intounderlyingbytesource_cancel: (a: number) => void;
    readonly __wbg_intounderlyingsource_free: (a: number, b: number) => void;
    readonly intounderlyingsource_pull: (a: number, b: number) => number;
    readonly intounderlyingsource_cancel: (a: number) => void;
    readonly __wbg_intounderlyingsink_free: (a: number, b: number) => void;
    readonly intounderlyingsink_write: (a: number, b: number) => number;
    readonly intounderlyingsink_close: (a: number) => number;
    readonly intounderlyingsink_abort: (a: number, b: number) => number;
    readonly rustsecp256k1_v0_10_0_context_create: (a: number) => number;
    readonly rustsecp256k1_v0_10_0_context_destroy: (a: number) => void;
    readonly rustsecp256k1_v0_10_0_default_illegal_callback_fn: (a: number, b: number) => void;
    readonly rustsecp256k1_v0_10_0_default_error_callback_fn: (a: number, b: number) => void;
    readonly __wasm_bindgen_func_elem_7940: (a: number, b: number, c: number, d: number) => void;
    readonly __wasm_bindgen_func_elem_7968: (a: number, b: number, c: number, d: number) => void;
    readonly __wasm_bindgen_func_elem_3345: (a: number, b: number, c: number) => void;
    readonly __wasm_bindgen_func_elem_7936: (a: number, b: number, c: number) => void;
    readonly __wasm_bindgen_func_elem_3280: (a: number, b: number) => void;
    readonly memory: WebAssembly.Memory;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number) => void;
    readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export5: (a: number, b: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_thread_destroy: (a?: number, b?: number, c?: number) => void;
    readonly __wbindgen_start: (a: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput, memory?: WebAssembly.Memory, thread_stack_size?: number }} module - Passing `SyncInitInput` directly is deprecated.
 * @param {WebAssembly.Memory} memory - Deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput, memory?: WebAssembly.Memory, thread_stack_size?: number } | SyncInitInput, memory?: WebAssembly.Memory): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput>, memory?: WebAssembly.Memory, thread_stack_size?: number }} module_or_path - Passing `InitInput` directly is deprecated.
 * @param {WebAssembly.Memory} memory - Deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput>, memory?: WebAssembly.Memory, thread_stack_size?: number } | InitInput | Promise<InitInput>, memory?: WebAssembly.Memory): Promise<InitOutput>;
