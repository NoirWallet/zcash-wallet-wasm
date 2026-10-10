import { WebWallet } from '../pkg/zcash_wallet_wasm.js';

/** @param {typeof WebWallet} Wallet */
function constructorContract(Wallet) {
  /** @type {import('../pkg/zcash_wallet_wasm.js').ConfirmationsPolicy} */
  const policy = { trusted: 1, untrusted: 2 };
  /** @type {import('../pkg/zcash_wallet_wasm.js').CompactBlockCdnOptions} */
  const cdn = {
    baseUrl: 'http://127.0.0.1:8081/compact-blocks/v1',
    requestTimeoutMs: 5000,
    maxConcurrentChunkRequests: 2,
  };
  /** @type {import('../pkg/zcash_wallet_wasm.js').ZeroIndexerOptions} */
  const zeroIndexer = { urls: ['https://example.invalid'], sendTimeoutMs: 5000 };
  const wallets = [
    new Wallet('test', 'https://example.invalid'),
    new Wallet('test', 'https://example.invalid', undefined),
    new Wallet('test', 'https://example.invalid', null, undefined),
    new Wallet('main', 'https://example.invalid', undefined, null),
    new Wallet('main', 'https://example.invalid', undefined, policy),
    new Wallet('test', 'https://example.invalid', new Uint8Array(), policy),
    new Wallet('test', 'https://example.invalid', undefined, policy, cdn),
    new Wallet('test', 'https://example.invalid', undefined, undefined, cdn),
    new Wallet('test', 'https://example.invalid', undefined, policy, cdn, zeroIndexer),
  ];
  wallets[0].set_zero_indexer_options(zeroIndexer);
  wallets[0].set_zero_indexer_options(null);
  for (const wallet of wallets) wallet.free();
  // @ts-expect-error Confirmation counts must be numbers.
  new Wallet('test', 'https://example.invalid', undefined, { ...policy, trusted: '1' });
  // @ts-expect-error A supplied policy must specify both confirmation counts.
  new Wallet('test', 'https://example.invalid', undefined, { trusted: 1 });
  // @ts-expect-error Zero-confirmation shielding is not a frontend setting.
  new Wallet('test', 'https://example.invalid', undefined, { ...policy, allowZeroConfShielding: false });
  // @ts-expect-error CDN baseUrl is required.
  new Wallet('test', 'https://example.invalid', undefined, policy, { requestTimeoutMs: 5000 });
  // @ts-expect-error CDN timeouts must be numbers.
  new Wallet('test', 'https://example.invalid', undefined, policy, { ...cdn, requestTimeoutMs: '5000' });
  // @ts-expect-error CDN concurrency must be a number.
  new Wallet('test', 'https://example.invalid', undefined, policy, { ...cdn, maxConcurrentChunkRequests: '2' });
  // @ts-expect-error Zero-indexer URLs are required.
  new Wallet('test', 'https://example.invalid', undefined, policy, cdn, { sendTimeoutMs: 5000 });
  // @ts-expect-error Zero-indexer timeouts must be numbers.
  wallets[0].set_zero_indexer_options({ ...zeroIndexer, sendTimeoutMs: '5000' });
}

export function walletConstructorTypes() {
  constructorContract(WebWallet);
}
