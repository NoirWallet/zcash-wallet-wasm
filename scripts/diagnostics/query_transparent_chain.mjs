import { writeFileSync } from 'node:fs';

// Read-only public-chain diagnostic. Never accepts a seed or private key.
// Usage: node scripts/diagnostics/query_transparent_chain.mjs [endpoint] [address] [output.json]
const endpoint = process.argv[2] ?? 'https://zcash-mainnet.chainsafe.dev';
const address = process.argv[3] ?? 't1ZxfFoVr9bqddi1oZpoMp9DH9sDv3iCTNS';
const outputPath = process.argv[4] ?? '/tmp/zcash-transparent-chain.json';
const varint = (value) => {
  let n = BigInt(value);
  const result = [];
  do { result.push(Number(n & 127n) | (n > 127n ? 128 : 0)); n >>= 7n; } while (n);
  return Buffer.from(result);
};
const integer = (field, value) => Buffer.concat([varint(field * 8), varint(value)]);
const bytes = (field, value) => {
  const data = Buffer.from(value);
  return Buffer.concat([varint(field * 8 + 2), varint(data.length), data]);
};
const decode = (data) => {
  const result = new Map();
  let offset = 0;
  const readVarint = () => {
    let value = 0n, shift = 0n, byte;
    do {
      if (offset >= data.length) throw Error('Truncated protobuf varint');
      byte = data[offset++]; value |= BigInt(byte & 127) << shift; shift += 7n;
    } while (byte & 128);
    return Number(value);
  };
  while (offset < data.length) {
    const tag = readVarint(), field = tag >> 3, wire = tag & 7;
    let value;
    if (wire === 0) value = readVarint();
    else if (wire === 2) {
      const length = readVarint(); value = data.subarray(offset, offset + length); offset += length;
    } else throw Error(`Unsupported wire type ${wire}`);
    result.set(field, [...(result.get(field) ?? []), value]);
  }
  return result;
};
const first = (fields, key, fallback = 0) => fields.get(key)?.[0] ?? fallback;
async function rpc(method, payload = Buffer.alloc(0)) {
  if (!['GetLatestBlock', 'GetAddressUtxosStream', 'GetTaddressTxids'].includes(method)) {
    throw Error('Only read-only RPC methods are permitted');
  }
  const header = Buffer.alloc(5); header.writeUInt32BE(payload.length, 1);
  const response = await fetch(`${endpoint}/cash.z.wallet.sdk.rpc.CompactTxStreamer/${method}`, {
    method: 'POST', headers: { 'content-type': 'application/grpc-web+proto', 'x-grpc-web': '1' },
    body: Buffer.concat([header, payload]), signal: AbortSignal.timeout(60000),
  });
  if (!response.ok) throw Error(`${method}: HTTP ${response.status}`);
  const data = Buffer.from(await response.arrayBuffer()), messages = [];
  let trailer = '';
  for (let offset = 0; offset < data.length;) {
    if (offset + 5 > data.length) throw Error('Truncated gRPC frame');
    const flags = data[offset], length = data.readUInt32BE(offset + 1); offset += 5;
    if (offset + length > data.length) throw Error('Truncated gRPC payload');
    const frame = data.subarray(offset, offset + length); offset += length;
    if (flags & 128) trailer += frame.toString();
    else if (flags === 0) messages.push(decode(frame));
    else throw Error('Unexpected compressed frame');
  }
  const status = /grpc-status:\s*(\d+)/.exec(trailer)?.[1] ?? response.headers.get('grpc-status');
  if (status !== '0') throw Error(`${method}: ${trailer || status || 'missing gRPC status'}`);
  return messages;
}
const tip = first((await rpc('GetLatestBlock'))[0], 1);
const [utxoMessages, txMessages] = await Promise.all([
  rpc('GetAddressUtxosStream', bytes(1, address)),
  rpc('GetTaddressTxids', Buffer.concat([
    bytes(1, address), bytes(2, Buffer.concat([bytes(1, integer(1, 1)), bytes(2, integer(1, tip))])),
  ])),
]);
const utxos = utxoMessages.map(fields => ({
  txid: Buffer.from(first(fields, 1)).reverse().toString('hex'),
  index: first(fields, 2), valueZat: first(fields, 4), height: first(fields, 5),
}));
const transactions = txMessages.map(fields => ({
  height: first(fields, 2), raw: first(fields, 1).toString('hex'),
}));
writeFileSync(outputPath, JSON.stringify({ endpoint, address, tip, utxos, transactions }, null, 2) + '\n');
console.log(JSON.stringify({ endpoint, address, tip, utxos,
  balanceZat: utxos.reduce((sum, utxo) => sum + utxo.valueZat, 0),
  transactions: transactions.map(tx => ({ height: tx.height, rawBytes: tx.raw.length / 2 })),
  publicFixture: outputPath,
}, null, 2));
