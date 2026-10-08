import { VotingStorage } from './voting_storage.mjs';
const $ = id => document.getElementById(id);
let api, storage, wallet, session, metadata, selected, roster = [], busy = true, failed = false, eligible = false, phase = 'uninitialized', nextAction = 'none';
const codes = new Set(['CONFIG_FETCH_FAILED', 'CONFIG_UNTRUSTED', 'ROUND_UNAUTHENTICATED', 'ROUND_EXPIRED', 'NETWORK_MISMATCH', 'WALLET_NOT_SYNCED', 'NOT_ELIGIBLE', 'PIR_SNAPSHOT_MISMATCH', 'PROOF_FAILED', 'PERSISTENCE_FAILED', 'SUBMISSION_REJECTED', 'SUBMISSION_UNKNOWN', 'OPERATION_IN_PROGRESS', 'RECOVERY_CONFLICT', 'STORAGE_IN_USE', 'STORAGE_OWNERSHIP_UNAVAILABLE', 'MNEMONIC_REQUIRED', 'INVALID_LWD_URL', 'BALLOT_INCOMPLETE']);
function controls() {
    const enabled = { initialize: !wallet, sync: !!wallet, 'wallet-refresh': !!wallet, restore: !wallet, discover: !!session, eligibility: !!selected, prepare: !!selected && eligible, delegate: nextAction === 'submit_delegation', vote: phase === 'delegation_confirmed' || phase === 'vote_confirmed', resume: !!selected, 'round-refresh': !!selected, 'clear-round': !!selected, clear: !!storage };
    document.querySelectorAll('[data-mutation]').forEach(button => { button.disabled = busy || failed || !(enabled[button.id] ?? !!session); });
    for (const id of ['lwd', 'birthday', 'account-index'])
        $(id).disabled = busy || !!wallet;
    for (const id of ['mnemonic', 'passphrase'])
        $(id).disabled = busy || failed;
    document.querySelectorAll('#proposals input').forEach(input => { input.disabled = busy || failed; });
}
async function operation(action) {
    if (busy || failed)
        return;
    busy = true;
    controls();
    $('operation-status').textContent = '';
    try {
        await action();
    }
    catch (error) {
        const code = codes.has(error?.name) ? error.name : codes.has(error?.message) ? error.message : 'OPERATION_FAILED';
        $('operation-status').textContent = code;
        if (storage?.poisoned || ['PERSISTENCE_FAILED', 'RECOVERY_CONFLICT', 'STORAGE_IN_USE'].includes(code)) {
            failed = true;
            $('operation-status').textContent += ': reload required';
        }
    }
    finally {
        busy = false;
        controls();
    }
}
function bind(id, action) { $(id).onclick = () => operation(action); }
async function persistWallet() {
    try {
        await storage.saveWallet(await wallet.db_to_bytes(), metadata);
    }
    catch {
        throw new Error('PERSISTENCE_FAILED');
    }
}
async function attachSession() {
    const probe = new api.WebVotingSession(wallet, metadata.accountId, undefined, () => Promise.resolve());
    let key;
    try {
        key = probe.storageKey;
    }
    finally {
        probe.free();
    }
    const saved = await storage.read(key);
    const checkpoint = await storage.bindSession(key);
    session = new api.WebVotingSession(wallet, metadata.accountId, saved ? new Uint8Array(saved.bytes) : undefined, checkpoint);
}
async function restore() {
    await storage.claim();
    const saved = await storage.read('wallet');
    if (!saved) {
        await storage.close();
        $('wallet-status').textContent = 'No saved wallet';
        return;
    }
    metadata = { accountId: saved.accountId, accountIndex: saved.accountIndex, birthday: saved.birthday, lwd: saved.lwd };
    $('lwd').value = metadata.lwd;
    $('birthday').value = metadata.birthday;
    $('account-index').value = metadata.accountIndex;
    wallet = new api.WebWallet('test', metadata.lwd, new Uint8Array(saved.bytes));
    await attachSession();
    await refreshWallet('Wallet restored');
}
bind('initialize', async () => {
    if (!$('mnemonic').value.trim())
        throw new Error('MNEMONIC_REQUIRED');
    if (!['lwd', 'birthday', 'account-index'].every(id => $(id).reportValidity()))
        return;
    const url = new URL($('lwd').value);
    if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash)
        throw new Error('INVALID_LWD_URL');
    await storage.claim();
    wallet = new api.WebWallet('test', url.href.replace(/\/$/, ''), undefined);
    metadata = { accountIndex: Number($('account-index').value), birthday: Number($('birthday').value), lwd: url.href.replace(/\/$/, '') };
    try {
        const account = wallet.create_account('Voting account', $('mnemonic').value, metadata.accountIndex, metadata.birthday, $('passphrase').value);
        $('mnemonic').value = '';
        $('passphrase').value = '';
        metadata.accountId = await account;
        await persistWallet();
        await attachSession();
        await refreshWallet('Wallet ready');
    }
    catch (error) {
        failed = true;
        throw error;
    }
});
bind('restore', restore);
bind('sync', async () => {
    let count = 0, more;
    do {
        more = await wallet.sync_next_batch(100);
        await persistWallet();
        count++;
        $('wallet-status').textContent = `Sync: ${count} batches saved`;
    } while (more);
    await refreshWallet(`Sync complete: ${count} batches saved`);
});
async function refreshWallet(label = 'Wallet status') {
    let summary;
    const lines = [label];
    try {
        summary = await wallet.get_wallet_summary();
        const balance = summary?.account_balances.find(([account]) => account === metadata.accountId)?.[1];
        lines.push(`Scanned height: ${summary?.fully_scanned_height ?? 'unavailable (unsynced)'}`);
        lines.push(balance ? `Balance: ${balance.total_balance} zats | Spendable: ${balance.spendable_balance} zats | Pending: ${balance.pending_balance} zats` : 'Balance: unavailable (unsynced)');
    }
    catch {
        lines.push('Scanned height: unavailable', 'Balance: unavailable');
    }
    finally {
        summary?.free();
    }
    try {
        lines.push(`Chain tip: ${await wallet.get_latest_height()}`);
    }
    catch {
        lines.push('Chain tip: unavailable');
    }
    $('wallet-status').textContent = lines.join('\n');
}
bind('wallet-refresh', () => refreshWallet());
function text(tag, content, parent) { const node = document.createElement(tag); node.textContent = content; parent.append(node); return node; }
function resetSelection() { selected = undefined; roster = []; eligible = false; phase = 'uninitialized'; nextAction = 'none'; $('selected-round').textContent = 'No round selected'; $('proposals').replaceChildren(); $('eligibility-status').textContent = 'Eligibility pending'; $('progress').textContent = 'No active round'; }
async function refreshRound() {
    const status = await session.roundStatus(selected.roundId);
    phase = status.phase;
    nextAction = status.nextAction;
    $('progress').textContent = `${status.phase}\nBundles ${status.completedBundles}/${status.totalBundles} | Votes ${status.confirmedVotes}/${status.totalVotes} | Helpers ${status.confirmedHelpers}/${status.requiredHelpers}\nNext: ${status.nextAction}`;
}
async function selectRound(round) {
    resetSelection();
    const proposals = await session.roundProposals(round.roundId);
    selected = round;
    roster = proposals;
    $('selected-round').textContent = round.name;
    for (const proposal of proposals) {
        const field = document.createElement('fieldset');
        text('legend', proposal.title, field);
        const choices = document.createElement('div');
        choices.className = 'choices';
        field.append(choices);
        [...proposal.options, 'Skip'].forEach((name, index) => {
            const label = document.createElement('label'), input = document.createElement('input');
            input.type = 'radio';
            input.name = `proposal-${proposal.proposalId}`;
            input.value = index === proposal.options.length ? 'skip' : String(index);
            label.append(input, document.createTextNode(name));
            choices.append(label);
        });
        $('proposals').append(field);
    }
    await refreshRound();
}
bind('discover', async () => {
    const discovery = await session.discoverRounds();
    resetSelection();
    for (const id of ['accepted', 'rejected'])
        $(id).replaceChildren();
    for (const round of discovery.accepted) {
        const row = document.createElement('div');
        row.className = 'round';
        const button = text('button', round.name, row);
        button.dataset.mutation = '';
        button.disabled = true;
        button.onclick = () => operation(() => selectRound(round));
        text('p', `Snapshot ${round.snapshotHeight} | ${round.proposalCount} proposals`, row);
        text('p', `Ends ${round.endsAt}`, row);
        $('accepted').append(row);
    }
    for (const round of discovery.rejected) {
        const row = document.createElement('div');
        row.className = 'round rejected';
        text('p', round.roundId, row);
        text('p', round.code, row);
        $('rejected').append(row);
    }
    if (!discovery.accepted.length)
        text('p', 'No authenticated rounds', $('accepted')).className = 'empty';
    if (!discovery.rejected.length)
        text('p', 'No rejected rounds', $('rejected')).className = 'empty';
});
bind('eligibility', async () => {
    const result = await session.checkEligibility(selected.roundId);
    eligible = result.eligible;
    $('eligibility-status').textContent = `${result.eligible ? 'Eligible' : 'Not eligible'} | Selected ${result.selectedValueZats} zats | Power ${result.votingPower} | Notes ${result.selectedNoteCount} | Bundles ${result.bundleCount} | Trimmed ${result.trimmedValueZats} zats`;
});
bind('prepare', async () => { $('progress').textContent = 'Preparing round...'; await session.prepareRound(selected.roundId); await refreshRound(); });
bind('delegate', async () => {
    if (!$('mnemonic').value.trim())
        throw new Error('MNEMONIC_REQUIRED');
    if (!confirm(`Submit delegation for ${selected.name}?`))
        return;
    let usk, fingerprint;
    try {
        usk = api.mnemonic_to_spending_key('test', $('mnemonic').value, $('passphrase').value, metadata.accountIndex);
        fingerprint = api.mnemonic_to_seed_fingerprint($('mnemonic').value, $('passphrase').value);
    }
    catch (error) {
        usk?.free();
        fingerprint?.free();
        throw error;
    }
    finally {
        $('mnemonic').value = '';
        $('passphrase').value = '';
    }
    try {
        $('progress').textContent = 'Proving and submitting delegation...';
        const submission = session.submitDelegation(selected.roundId, usk, fingerprint);
        usk = fingerprint = undefined;
        try {
            await submission;
        }
        finally {
            await refreshRound();
        }
    }
    finally {
        usk?.free();
        fingerprint?.free();
    }
});
bind('vote', async () => {
    const decisions = roster.map(proposal => {
        const choice = document.querySelector(`input[name="proposal-${proposal.proposalId}"]:checked`);
        if (!choice)
            throw new Error('BALLOT_INCOMPLETE');
        return choice.value === 'skip' ? { kind: 'skip', proposalId: proposal.proposalId } : { kind: 'choice', proposalId: proposal.proposalId, option: Number(choice.value) };
    });
    const summary = roster.map((proposal, index) => `${proposal.title}: ${decisions[index].kind === 'skip' ? 'Skip' : proposal.options[decisions[index].option]}`).join('\n');
    if (!confirm(`Submit ballot for ${selected.name}?\n\n${summary}`))
        return;
    $('progress').textContent = 'Proving and submitting ballot...';
    await session.submitBallot(selected.roundId, decisions);
    await refreshRound();
});
bind('resume', async () => { $('progress').textContent = 'Resuming round...'; await session.resumeRound(selected.roundId); await refreshRound(); });
bind('round-refresh', refreshRound);
bind('clear-round', async () => { if (confirm(`Clear local recovery state for ${selected.name}?`)) {
    await session.clearRound(selected.roundId);
    resetSelection();
} });
bind('clear', async () => {
    if (!confirm('Clear all local wallet and voting recovery state?'))
        return;
    await storage.claim();
    await storage.clear();
    session?.free();
    wallet?.free();
    session = wallet = metadata = undefined;
    resetSelection();
    for (const id of ['accepted', 'rejected'])
        $(id).replaceChildren();
    $('mnemonic').value = '';
    $('passphrase').value = '';
    $('wallet-status').textContent = 'Local state cleared';
});
$('reload').onclick = () => location.reload();
addEventListener('pagehide', () => { storage?.close(); $('mnemonic').value = ''; $('passphrase').value = ''; });
try {
    api = await import('./pkg/zcash_wallet_wasm.js');
    await api.default();
    if (api.initThreadPool)
        await api.initThreadPool(Math.max(1, Math.min(navigator.hardwareConcurrency || 2, 4)));
    storage = await VotingStorage.open();
    if (await storage.read('wallet'))
        await restore();
    else
        $('wallet-status').textContent = 'Wallet not initialized';
}
catch {
    failed = true;
    $('wallet-status').textContent = 'Module or local state unavailable; reload required';
}
finally {
    busy = false;
    controls();
    globalThis.lucide?.createIcons();
}
