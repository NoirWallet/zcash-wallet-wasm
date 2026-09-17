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
    new Wallet('main', 'https://example.invalid', undefined, policy, []),
    new Wallet('main', 'https://example.invalid', undefined, policy, [
      'https://zero-indexer-1.invalid',
      'https://zero-indexer-2.invalid',
    ]),
  ];
  for (const wallet of wallets) wallet.free();
  // @ts-expect-error Confirmation counts must be numbers.
  new Wallet('test', 'https://example.invalid', undefined, { ...policy, trusted: '1' });
  // @ts-expect-error A supplied policy must specify both confirmation counts.
  new Wallet('test', 'https://example.invalid', undefined, { trusted: 1 });
  // @ts-expect-error Zero-confirmation shielding is not a frontend setting.
  new Wallet('test', 'https://example.invalid', undefined, { ...policy, allowZeroConfShielding: false });
  // @ts-expect-error Zero-indexer endpoints must be strings.
  new Wallet('test', 'https://example.invalid', undefined, policy, [42]);
}

export function walletConstructorTypes() {
  constructorContract(WebWallet);
}
