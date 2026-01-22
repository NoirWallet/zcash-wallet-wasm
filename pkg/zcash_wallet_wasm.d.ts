/* tslint:disable */
/* eslint-disable */
/**
 * The `ReadableStreamType` enum.
 *
 * *This API requires the following crate features to be activated: `ReadableStreamType`*
 */

type ReadableStreamType = "bytes";

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
  start(controller: ReadableByteStreamController): void;
  pull(controller: ReadableByteStreamController): Promise<any>;
  cancel(): void;
  readonly type: ReadableStreamType;
  readonly autoAllocateChunkSize: number;
}

export class IntoUnderlyingSink {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  write(chunk: any): Promise<any>;
  close(): Promise<any>;
  abort(reason: any): Promise<any>;
}

export class IntoUnderlyingSource {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  pull(controller: ReadableStreamDefaultController): Promise<any>;
  cancel(): void;
}

export class PaymentRequest {
  free(): void;
  [Symbol.dispose](): void;
  constructor(recipient_address: string, amount: bigint, memo: Uint8Array | null | undefined, label: string | null | undefined, message: string | null | undefined, other_params: any);
  static simple_payment(recipient_address: string, amount: bigint): PaymentRequest;
  recipient_address(): string;
  amount(): bigint;
  memo(): Uint8Array | undefined;
  label(): string | undefined;
  message(): string | undefined;
  other_params(): any;
}

export class Pczt {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Returns a JSON object with the details of the Pczt.
   */
  to_json(): any;
  /**
   * Returns a Pczt from a JSON object
   */
  static from_json(s: any): Pczt;
  /**
   * Returns the postcard serialization of the Pczt.
   */
  serialize(): Uint8Array;
  /**
   * Deserialize to a Pczt from postcard bytes.
   */
  static from_bytes(bytes: Uint8Array): Pczt;
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
  constructor(seed: Uint8Array);
  to_bytes(): Uint8Array;
  static from_bytes(bytes: Uint8Array): SeedFingerprint;
}

export class TransactionRequest {
  free(): void;
  [Symbol.dispose](): void;
  constructor(payments: PaymentRequest[]);
  static empty(): TransactionRequest;
  payment_requests(): PaymentRequest[];
  total(): bigint;
  static from_uri(uri: string): TransactionRequest;
  to_uri(): string;
}

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
  /**
   * Obtain the UFVK corresponding to this spending key
   */
  to_unified_full_viewing_key(): UnifiedFullViewingKey;
  to_sapling_proof_generation_key(): ProofGenerationKey;
  /**
   * Encode the Sapling Extended Spending Key to Bech32 format
   *
   * # Arguments
   * * `network` - Must be either "main" or "test"
   *
   * # Returns
   * String in format "secret-extended-key-main..." or "secret-extended-key-test..."
   */
  encode_sapling_extended_spending_key(network: string): string;
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
  next_sapling_subtree_index: bigint;
  next_orchard_subtree_index: bigint;
  readonly account_balances: any;
}

export class WebWallet {
  free(): void;
  [Symbol.dispose](): void;
  constructor(network: string, lightwalletd_url: string, min_confirmations: number, db_bytes?: Uint8Array | null);
  create_account(account_name: string, seed_phrase: string, account_hd_index: number, birthday_height?: number | null): Promise<number>;
  create_account_ufvk(account_name: string, encoded_ufvk: string, seed_fingerprint: SeedFingerprint, account_hd_index: number, birthday_height?: number | null): Promise<number>;
  create_account_view_ufvk(account_name: string, encoded_ufvk: string, birthday_height?: number | null): Promise<number>;
  sync(): Promise<void>;
  get_wallet_summary(): Promise<WalletSummary | undefined>;
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
  propose_transfer(account_id: number, to_address: string, value: bigint): Promise<Proposal>;
  create_proposed_transactions(proposal: Proposal, seed_phrase: string, account_hd_index: number): Promise<Uint8Array>;
  db_to_bytes(): Promise<Uint8Array>;
  send_authorized_transactions(txids: Uint8Array): Promise<void>;
  get_current_address(account_id: number): Promise<string>;
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
   * Get unified addresses for an account
   *
   * # Arguments
   *
   * * `account_id` - The account ID to get addresses for
   *
   * # Returns
   *
   * Returns a UnifiedAddresses object containing:
   * - unified: The full unified address (u1 format with all receivers)
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
  pczt_shield(account_id: number): Promise<Pczt>;
  /**
   * Shield transparent funds by moving them to the Orchard shielded pool
   * This is a direct method that doesn't use PCZT (works around metadata limitations)
   *
   * # Arguments
   *
   * * `account_id` - The ID of the account to shield funds from
   * * `mnemonic` - The mnemonic phrase to derive spending keys
   *
   * # Returns
   *
   * Returns transaction IDs of the shielding transactions
   */
  shield_funds(account_id: number, usk: UnifiedSpendingKey): Promise<any>;
  pczt_create(account_id: number, to_address: string, value: bigint): Promise<Pczt>;
  pczt_prove(pczt: Pczt, sapling_proof_gen_key?: ProofGenerationKey | null): Promise<Pczt>;
  pczt_send(pczt: Pczt): Promise<void>;
  pczt_combine(pczts: Pczt[]): Pczt;
  get_current_address_transparent(account_id: number): Promise<string>;
  get_latest_block(): Promise<bigint>;
}

/**
 * Decrypt a string using age with a passphrase
 *
 * # Arguments
 *
 * * `ciphertext` - Base64-encoded encrypted data
 * * `passphrase` - The passphrase to use for decryption
 *
 * # Returns
 *
 * The decrypted plaintext string
 *
 * # Example
 *
 * ```javascript
 * const decrypted = decrypt_string(encrypted, "my-secret-password");
 * console.log(decrypted); // "Hello, World!"
 * ```
 */
export function decrypt_string(ciphertext: string, passphrase: string): string;

/**
 * Encrypt a string using age with a passphrase
 *
 * # Arguments
 *
 * * `plaintext` - The string to encrypt
 * * `passphrase` - The passphrase to use for encryption
 *
 * # Returns
 *
 * Base64-encoded encrypted data
 *
 * # Example
 *
 * ```javascript
 * const encrypted = encrypt_string("Hello, World!", "my-secret-password");
 * console.log(encrypted); // Base64-encoded ciphertext
 * ```
 */
export function encrypt_string(plaintext: string, passphrase: string): string;

/**
 * Convert entropy bytes to a mnemonic phrase.
 *
 * # Arguments
 *
 * * `entropy` - The entropy bytes (16, 20, 24, 28, or 32 bytes)
 *
 * # Returns
 *
 * A string containing the mnemonic phrase
 *
 * # Example
 *
 * ```javascript
 * const entropy = new Uint8Array(32); // 32 bytes for 24 words
 * crypto.getRandomValues(entropy);
 * const mnemonic = entropy_to_mnemonic(entropy);
 * console.log(mnemonic);
 * ```
 */
export function entropy_to_mnemonic(entropy: Uint8Array): string;

/**
 * Generate a new BIP-39 mnemonic phrase with the specified word count.
 *
 * # Arguments
 *
 * * `word_count` - Number of words in the mnemonic (12, 15, 18, 21, or 24)
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
 * Get all addresses (unified, orchard, sapling, transparent) from a mnemonic phrase.
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
 * Convert a mnemonic phrase to its entropy bytes.
 *
 * # Arguments
 *
 * * `phrase` - The mnemonic phrase
 *
 * # Returns
 *
 * A vector of bytes representing the entropy
 *
 * # Example
 *
 * ```javascript
 * const entropy = mnemonic_to_entropy("abandon abandon abandon ... art");
 * console.log(entropy); // Uint8Array
 * ```
 */
export function mnemonic_to_entropy(phrase: string): Uint8Array;

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
 * Generate a UnifiedFullViewingKey from a mnemonic phrase.
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
 * A UnifiedFullViewingKey that can be used for viewing transactions
 *
 * # Example
 *
 * ```javascript
 * const mnemonic = generate_mnemonic(24);
 * const ufvk = mnemonic_to_viewing_key(
 *     "main",
 *     mnemonic,
 *     "",  // no passphrase
 *     0    // first account
 * );
 * ```
 */
export function mnemonic_to_viewing_key(network: string, phrase: string, passphrase: string, account_index: number): UnifiedFullViewingKey;

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

export function pczt_sign(network: string, pczt: Pczt, usk: UnifiedSpendingKey, seed_fp: SeedFingerprint): Promise<Pczt>;

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
 * const result = await sign_message_with_transparent("main", usk, "Hello Zcash!");
 * console.log("Signature:", result.signature);  // hex format
 * console.log("Public Key:", result.pubkey);     // uncompressed hex (65 bytes)
 * console.log("Address:", result.address);       // t-address
 * ```
 */
export function sign_message_with_transparent(network: string, usk: UnifiedSpendingKey, message: string): any;

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
  numThreads(): number;
  receiver(): number;
  build(): void;
}

export function wbg_rayon_start_worker(receiver: number): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly encrypt_string: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly decrypt_string: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly __wbg_seedfingerprint_free: (a: number, b: number) => void;
  readonly seedfingerprint_new: (a: number, b: number, c: number) => void;
  readonly seedfingerprint_to_bytes: (a: number, b: number) => void;
  readonly seedfingerprint_from_bytes: (a: number, b: number, c: number) => void;
  readonly __wbg_proofgenerationkey_free: (a: number, b: number) => void;
  readonly __wbg_unifiedspendingkey_free: (a: number, b: number) => void;
  readonly unifiedspendingkey_new: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly unifiedspendingkey_to_unified_full_viewing_key: (a: number) => number;
  readonly unifiedspendingkey_to_sapling_proof_generation_key: (a: number) => number;
  readonly unifiedspendingkey_encode_sapling_extended_spending_key: (a: number, b: number, c: number, d: number) => void;
  readonly __wbg_unifiedfullviewingkey_free: (a: number, b: number) => void;
  readonly unifiedfullviewingkey_encode: (a: number, b: number, c: number, d: number) => void;
  readonly unifiedfullviewingkey_new: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly generate_mnemonic: (a: number, b: number) => void;
  readonly validate_mnemonic: (a: number, b: number, c: number) => void;
  readonly mnemonic_to_entropy: (a: number, b: number, c: number) => void;
  readonly entropy_to_mnemonic: (a: number, b: number, c: number) => void;
  readonly mnemonic_to_seed: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly mnemonic_to_spending_key: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
  readonly mnemonic_to_seed_fingerprint: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly mnemonic_to_viewing_key: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
  readonly mnemonic_to_addresses: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
  readonly parse_unified_address: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly __wbg_pczt_free: (a: number, b: number) => void;
  readonly pczt_to_json: (a: number) => number;
  readonly pczt_from_json: (a: number) => number;
  readonly pczt_serialize: (a: number, b: number) => void;
  readonly pczt_from_bytes: (a: number, b: number) => number;
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
  readonly paymentrequest_other_params: (a: number) => number;
  readonly pczt_sign: (a: number, b: number, c: number, d: number, e: number) => number;
  readonly sign_message_with_transparent: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly __wbg_webwallet_free: (a: number, b: number) => void;
  readonly webwallet_new: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
  readonly webwallet_create_account: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => number;
  readonly webwallet_create_account_ufvk: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => number;
  readonly webwallet_create_account_view_ufvk: (a: number, b: number, c: number, d: number, e: number, f: number) => number;
  readonly webwallet_sync: (a: number) => number;
  readonly webwallet_get_wallet_summary: (a: number) => number;
  readonly webwallet_get_account_birthday: (a: number, b: number) => number;
  readonly webwallet_get_block_time: (a: number, b: number) => number;
  readonly webwallet_get_latest_height: (a: number) => number;
  readonly webwallet_get_block_by_time: (a: number, b: number) => number;
  readonly webwallet_propose_transfer: (a: number, b: number, c: number, d: number, e: bigint) => number;
  readonly webwallet_create_proposed_transactions: (a: number, b: number, c: number, d: number, e: number) => number;
  readonly webwallet_db_to_bytes: (a: number) => number;
  readonly webwallet_send_authorized_transactions: (a: number, b: number, c: number) => number;
  readonly webwallet_get_current_address: (a: number, b: number) => number;
  readonly webwallet_get_account_id_by_name: (a: number, b: number, c: number) => number;
  readonly webwallet_get_account_hd_index: (a: number, b: number) => number;
  readonly webwallet_get_unified_addresses: (a: number, b: number) => number;
  readonly webwallet_pczt_shield: (a: number, b: number) => number;
  readonly webwallet_shield_funds: (a: number, b: number, c: number) => number;
  readonly webwallet_pczt_create: (a: number, b: number, c: number, d: number, e: bigint) => number;
  readonly webwallet_pczt_prove: (a: number, b: number, c: number) => number;
  readonly webwallet_pczt_send: (a: number, b: number) => number;
  readonly webwallet_pczt_combine: (a: number, b: number, c: number, d: number) => void;
  readonly webwallet_get_current_address_transparent: (a: number, b: number) => number;
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
  readonly walletsummary_account_balances: (a: number) => number;
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
  readonly __wasm_bindgen_func_elem_4696: (a: number, b: number, c: number) => void;
  readonly __wasm_bindgen_func_elem_4695: (a: number, b: number) => void;
  readonly __wasm_bindgen_func_elem_9090: (a: number, b: number, c: number, d: number) => void;
  readonly memory: WebAssembly.Memory;
  readonly __wbindgen_export: (a: number, b: number) => number;
  readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_export3: (a: number) => void;
  readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
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
