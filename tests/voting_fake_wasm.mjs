// UI boundary fixture: real WASM and live discovery have separate browser gates.
export default async function init() {}
export async function initThreadPool() {}
class TemporaryKey { consume() {this.consumed=true;} free() {if(this.consumed) throw new Error('consumed key freed twice');} }
export function mnemonic_to_spending_key() { return new TemporaryKey(); }
export function mnemonic_to_seed_fingerprint() { return new TemporaryKey(); }
export class WebWallet {
  constructor(network, lwd, bytes) { this.bytes = bytes ?? new Uint8Array([11,12]); }
  async create_account() { await new Promise(resolve => setTimeout(resolve, 250)); return 0; }
  async db_to_bytes() { return this.bytes; }
  async sync_next_batch() { await new Promise(resolve => setTimeout(resolve, 250)); this.bytes = new Uint8Array([13,14]); return false; }
  async get_wallet_summary() {
    if (this.bytes[0] !== 13) return undefined;
    return {
      chain_tip_height: 3771741, fully_scanned_height: 3771700,
      account_balances: [[0, {sapling_balance:0,orchard_balance:100000000,ironwood_balance:0,unshielded_balance:0,sapling_pending_balance:0,orchard_pending_balance:20000000,ironwood_pending_balance:0,unshielded_pending_balance:0,sapling_total_balance:0,orchard_total_balance:120000000,ironwood_total_balance:0,unshielded_total_balance:0,spendable_balance:100000000,pending_balance:20000000,total_balance:120000000}]],
      next_sapling_subtree_index:0n,next_orchard_subtree_index:0n,next_ironwood_subtree_index:0n,
      free() {},
    };
  }
  async get_latest_height() { return 3771800; }
  free() {}
}
export class WebVotingSession {
  storageKey = 'voting:test:fixture-owner';
  constructor(wallet, account, bytes, persist) { this.bytes=bytes; this.persist=persist; this.revision=0n; }
  async discoverRounds() { return { configFingerprint:'fixture', accepted:[{roundId:'accepted',name:'Community round',snapshotHeight:3771641,startsAt:'2026-01-01',endsAt:'2099-01-01',proposalCount:1}],rejected:[{roundId:'rejected',code:'ROUND_UNAUTHENTICATED',message:'Unauthenticated round'}] }; }
  async roundProposals() { return [{proposalId:1,title:'Community allocation',options:['Approve','Decline']}]; }
  async checkEligibility() { return {roundId:'accepted',eligible:true,selectedValueZats:'100000000',votingPower:'1',selectedNoteCount:1,bundleCount:1,trimmedValueZats:'0'}; }
  async roundStatus() { return {roundId:'accepted',phase:this.phase??(this.bytes?'round_prepared':'uninitialized'),completedBundles:0,totalBundles:1,confirmedVotes:0,totalVotes:1,confirmedHelpers:0,requiredHelpers:1,nextAction:this.bytes?'submit_delegation':'prepare_round'}; }
  async submitDelegation(id,key,fingerprint) { key.consume();fingerprint.consume();this.phase='delegation_confirmed';await this.persist(new Uint8Array([24]),++this.revision); }
  async submitBallot(id,decisions) { if(decisions.length!==1||decisions[0].proposalId!==1||decisions[0].kind!=='skip') throw new Error('incorrect fixture ballot');this.phase='shares_confirmed';await this.persist(new Uint8Array([25]),++this.revision); }
  async prepareRound() { this.bytes=new Uint8Array([21,22]); await this.persist(this.bytes,++this.revision); return {roundId:'accepted',bundleCount:1,selectedNoteCount:1,votingPower:'1',state:'round_prepared'}; }
  async clearRound() { this.bytes=new Uint8Array([23]); await this.persist(this.bytes,++this.revision); }
  exportState() { return this.bytes ?? new Uint8Array(); }
  free() {}
}
