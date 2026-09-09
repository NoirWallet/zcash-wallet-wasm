import { WebVotingSession, WebWallet, mnemonic_to_spending_key, mnemonic_to_seed_fingerprint } from '../pkg-voting/zcash_wallet_wasm.js';

/** Type-check the page's actual stable API calls without executing private actions.
 * @param {WebWallet} wallet
 * @param {string} runtimePhrase
 * @param {import('../pkg-voting/zcash_wallet_wasm.js').BallotDecision[]} decisions
 */
export async function pageContract(wallet, runtimePhrase, decisions) {
  const session=new WebVotingSession(wallet,0,undefined,async (/** @type {Uint8Array} */ bytes,/** @type {bigint} */ revision)=>{void bytes;void revision;});
  /** @type {import('../pkg-voting/zcash_wallet_wasm.js').VotingDiscovery} */
  const discovery=await session.discoverRounds();
  const id=discovery.accepted[0].roundId;
  /** @type {import('../pkg-voting/zcash_wallet_wasm.js').VotingProposal[]} */
  const proposals=await session.roundProposals(id);
  /** @type {import('../pkg-voting/zcash_wallet_wasm.js').VotingEligibility} */
  const eligibility=await session.checkEligibility(id);
  /** @type {import('../pkg-voting/zcash_wallet_wasm.js').PreparedVotingRound} */
  const prepared=await session.prepareRound(id);
  const testKey=mnemonic_to_spending_key('test',runtimePhrase,'',0);
  const mainKey=mnemonic_to_spending_key('main',runtimePhrase,'',0);
  void mainKey;
  /** @type {import('../pkg-voting/zcash_wallet_wasm.js').VotingActionResult} */
  let result=await session.submitDelegation(id,testKey,mnemonic_to_seed_fingerprint(runtimePhrase,''));
  result=await session.submitBallot(id,decisions);result=await session.resumeRound(id);
  /** @type {import('../pkg-voting/zcash_wallet_wasm.js').VotingRoundStatus} */
  const status=await session.roundStatus(id);
  /** @type {import('../pkg-voting/zcash_wallet_wasm.js').VotingRoundPhase} */
  const phase=status.phase;
  /** @type {import('../pkg-voting/zcash_wallet_wasm.js').VotingErrorCode} */
  const code='ROUND_UNAUTHENTICATED';
  /** @type {import('../pkg-voting/zcash_wallet_wasm.js').VotingErrorCode} */
  // @ts-expect-error Stable error codes reject arbitrary server text.
  const badCode='private server text';
  // @ts-expect-error Choices require a numeric option.
  await session.submitBallot(id,[{kind:'choice',proposalId:1,option:'bad'}]);
  await session.clearRound(id);const bytes=session.exportState();const key=session.storageKey;session.free();
  return {proposals,eligibility,prepared,result,phase,code,bytes,key,badCode};
}
