import { WebWallet } from '../pkg/zcash_wallet_wasm.js';

/** @param {typeof WebWallet} Wallet */
function constructorContract(Wallet) {
  /** @type {import('../pkg/zcash_wallet_wasm.js').ConfirmationsPolicy} */
  const policy = { trusted: 1, untrusted: 2 };
  const wallets = [
    new Wallet('test', 'https://example.invalid'),
    new Wallet('test', 'https://example.invalid', undefined),
    new Wallet('test', 'https://example.invalid', null, undefined),
    new Wallet('main', 'https://example.invalid', undefined, null),
    new Wallet('main', 'https://example.invalid', undefined, policy),
    new Wallet('test', 'https://example.invalid', new Uint8Array(), policy),
    new Wallet('main', 'https://example.invalid', undefined, policy, {}),
    new Wallet('main', 'https://example.invalid', undefined, policy, {
      urls: [
        'https://zero-indexer-1.invalid',
        'https://zero-indexer-2.invalid',
      ],
      sendTransactionTimeoutMs: 30_000,
    }),
  ];
  for (const wallet of wallets) wallet.free();
  // @ts-expect-error Confirmation counts must be numbers.
  new Wallet('test', 'https://example.invalid', undefined, { ...policy, trusted: '1' });
  // @ts-expect-error A supplied policy must specify both confirmation counts.
  new Wallet('test', 'https://example.invalid', undefined, { trusted: 1 });
  // @ts-expect-error Zero-confirmation shielding is not a frontend setting.
  new Wallet('test', 'https://example.invalid', undefined, { ...policy, allowZeroConfShielding: false });
  // @ts-expect-error Zero-indexer endpoints must be strings.
  new Wallet('test', 'https://example.invalid', undefined, policy, { urls: [42] });
  // @ts-expect-error The send timeout must be a number.
  new Wallet('test', 'https://example.invalid', undefined, policy, { sendTransactionTimeoutMs: '30000' });
  // @ts-expect-error GetTransaction is not configured through zero-indexer.
  new Wallet('test', 'https://example.invalid', undefined, policy, { getTransactionTimeoutMs: 30_000 });
}

/** @param {WebWallet} wallet */
async function submissionContract(wallet) {
  /** @type {import('../pkg/zcash_wallet_wasm.js').TransferResult} */
  const result = await wallet.send_authorized_transactions(new Uint8Array(32));
  /** @type {import('../pkg/zcash_wallet_wasm.js').TransferOverallStatus} */
  const status = result.status;
  return status;
}

export function walletConstructorTypes() {
  constructorContract(WebWallet);
  void submissionContract;
}
