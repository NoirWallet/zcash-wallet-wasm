//! Implementation of the synchronization flow described in the crate root.
//!
//! This is currently a simple implementation that does not yet implement a few features:
//!
//! - Block batches are not downloaded in parallel with scanning.
//! - Transactions are not enhanced once detected (that is, after an output is detected in
//!   a transaction, the full transaction is not downloaded and scanned).
//! - There is no mechanism for notifying the caller of progress updates.
//! - There is no mechanism for interrupting the synchronization flow, other than ending
//!   the process.

#[cfg(feature = "sync")]
use {
    crate::{
        data_api::{
            WalletCommitmentTrees, WalletRead, WalletWrite,
            chain::{
                BlockCache, ChainState, CommitmentTreeRoot, error::Error as ChainError,
                scan_cached_blocks, scan_cached_blocks_optimized,
            },
            scanning::{ScanPriority, ScanRange},
        },
        proto::{
            compact_formats::CompactBlock,
            service::{self, BlockId, compact_tx_streamer_client::CompactTxStreamerClient},
        },
        scanning::ScanError,
    },
    futures_util::TryStreamExt,
    shardtree::error::ShardTreeError,
    std::fmt,
    subtle::ConditionallySelectable,
    tonic::{
        body::Body as TonicBody,
        client::GrpcService,
        codegen::{Body, Bytes, StdError},
    },
    tracing::{debug, info},
    zcash_primitives::{block::BlockHash, merkle_tree::HashSer},
    zcash_protocol::consensus::{BlockHeight, NetworkUpgrade, Parameters},
};

#[cfg(all(feature = "sync", feature = "orchard"))]
use orchard::tree::MerkleHashOrchard;

#[cfg(all(feature = "sync", feature = "transparent-inputs"))]
use {
    crate::wallet::WalletTransparentOutput,
    ::transparent::{
        address::Script,
        bundle::{OutPoint, TxOut},
    },
    zcash_keys::encoding::AddressCodec as _,
    zcash_protocol::value::Zatoshis,
    zcash_script::script,
};

#[cfg(feature = "sync-decryptor")]
pub mod decryptor;

/// Retrieves compact blocks for synchronization.
///
/// The lightwalletd client is passed to the source so alternative sources can
/// fall back without duplicating the remaining synchronization transport.
#[cfg(feature = "sync")]
#[async_trait::async_trait(?Send)]
pub trait CompactBlockSource<ChT> {
    async fn get_block_range(
        &mut self,
        client: &mut CompactTxStreamerClient<ChT>,
        range: service::BlockRange,
        expected_prev_hash: Option<BlockHash>,
    ) -> Result<Vec<CompactBlock>, tonic::Status>;
}

/// A compact block source that always uses lightwalletd.
#[cfg(feature = "sync")]
pub struct LightwalletdBlockSource;

#[cfg(feature = "sync")]
#[async_trait::async_trait(?Send)]
impl<ChT> CompactBlockSource<ChT> for LightwalletdBlockSource
where
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
{
    async fn get_block_range(
        &mut self,
        client: &mut CompactTxStreamerClient<ChT>,
        range: service::BlockRange,
        _expected_prev_hash: Option<BlockHash>,
    ) -> Result<Vec<CompactBlock>, tonic::Status> {
        client
            .get_block_range(range)
            .await?
            .into_inner()
            .try_collect()
            .await
    }
}

/// Scans the chain until the wallet is up-to-date.
#[cfg(feature = "sync")]
pub async fn run<P, ChT, CaT, DbT>(
    client: &mut CompactTxStreamerClient<ChT>,
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    batch_size: u32,
) -> Result<(), Error<CaT::Error, <DbT as WalletRead>::Error, <DbT as WalletCommitmentTrees>::Error>>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite + WalletCommitmentTrees,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    <DbT as WalletCommitmentTrees>::Error: std::error::Error + Send + Sync + 'static,
{
    run_with_block_source(
        client,
        &mut LightwalletdBlockSource,
        params,
        db_cache,
        db_data,
        batch_size,
    )
    .await
}

/// Scans the chain using an injectable compact block source.
#[cfg(feature = "sync")]
pub async fn run_with_block_source<P, ChT, CaT, DbT, BlT>(
    client: &mut CompactTxStreamerClient<ChT>,
    block_source: &mut BlT,
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    batch_size: u32,
) -> Result<(), Error<CaT::Error, <DbT as WalletRead>::Error, <DbT as WalletCommitmentTrees>::Error>>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite + WalletCommitmentTrees,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    <DbT as WalletCommitmentTrees>::Error: std::error::Error + Send + Sync + 'static,
    BlT: CompactBlockSource<ChT>,
{
    let mut tip_height = update_chain_tip(client, db_data).await?;
    let mut ironwood_active = params.is_nu_active(NetworkUpgrade::Nu6_3, tip_height);

    // 1) Download note commitment tree data from lightwalletd
    // 2) Pass the commitment tree data to the database.
    update_subtree_roots(client, params, tip_height, db_data).await?;

    while running(client, block_source, params, db_cache, db_data, batch_size).await? {
        tip_height = update_chain_tip(client, db_data).await?;
        let next_ironwood_active = params.is_nu_active(NetworkUpgrade::Nu6_3, tip_height);
        if next_ironwood_active && !ironwood_active {
            update_subtree_roots(client, params, tip_height, db_data).await?;
        }
        ironwood_active = next_ironwood_active;
    }

    Ok(())
}

/// Syncs a single batch of blocks and returns whether any work was performed.
///
/// Unlike [`run`], which loops until the wallet is fully synced, this function
/// processes at most `batch_size` blocks after handling any Verify-priority
/// ranges needed for chain continuity.
#[cfg(feature = "sync")]
pub async fn run_next_batch<P, ChT, CaT, DbT>(
    client: &mut CompactTxStreamerClient<ChT>,
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    batch_size: u32,
) -> Result<
    bool,
    Error<CaT::Error, <DbT as WalletRead>::Error, <DbT as WalletCommitmentTrees>::Error>,
>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite + WalletCommitmentTrees,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    <DbT as WalletCommitmentTrees>::Error: std::error::Error + Send + Sync + 'static,
{
    run_next_batch_with_block_source(
        client,
        &mut LightwalletdBlockSource,
        params,
        db_cache,
        db_data,
        batch_size,
    )
    .await
}

/// Syncs a single batch using an injectable compact block source.
#[cfg(feature = "sync")]
pub async fn run_next_batch_with_block_source<P, ChT, CaT, DbT, BlT>(
    client: &mut CompactTxStreamerClient<ChT>,
    block_source: &mut BlT,
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    batch_size: u32,
) -> Result<
    bool,
    Error<CaT::Error, <DbT as WalletRead>::Error, <DbT as WalletCommitmentTrees>::Error>,
>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite + WalletCommitmentTrees,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    <DbT as WalletCommitmentTrees>::Error: std::error::Error + Send + Sync + 'static,
    BlT: CompactBlockSource<ChT>,
{
    let tip_height = update_chain_tip(client, db_data).await?;
    update_subtree_roots(client, params, tip_height, db_data).await?;
    running_single_batch(client, block_source, params, db_cache, db_data, batch_size).await
}

#[cfg(feature = "sync")]
/// Syncs a single batch using the optimized scanner.
///
/// This has the same return semantics as [`run_next_batch`], but uses
/// External-only batch trial decryption with targeted Internal IVK fallback for
/// contiguous scan ranges.
pub async fn run_next_batch_optimized<P, ChT, CaT, DbT>(
    client: &mut CompactTxStreamerClient<ChT>,
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    batch_size: u32,
) -> Result<
    bool,
    Error<CaT::Error, <DbT as WalletRead>::Error, <DbT as WalletCommitmentTrees>::Error>,
>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite + WalletCommitmentTrees,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    <DbT as WalletCommitmentTrees>::Error: std::error::Error + Send + Sync + 'static,
{
    run_next_batch_optimized_with_block_source(
        client,
        &mut LightwalletdBlockSource,
        params,
        db_cache,
        db_data,
        batch_size,
    )
    .await
}

/// Syncs a single optimized batch using an injectable compact block source.
#[cfg(feature = "sync")]
pub async fn run_next_batch_optimized_with_block_source<P, ChT, CaT, DbT, BlT>(
    client: &mut CompactTxStreamerClient<ChT>,
    block_source: &mut BlT,
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    batch_size: u32,
) -> Result<
    bool,
    Error<CaT::Error, <DbT as WalletRead>::Error, <DbT as WalletCommitmentTrees>::Error>,
>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite + WalletCommitmentTrees,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    <DbT as WalletCommitmentTrees>::Error: std::error::Error + Send + Sync + 'static,
    BlT: CompactBlockSource<ChT>,
{
    let tip_height = update_chain_tip(client, db_data).await?;
    update_subtree_roots(client, params, tip_height, db_data).await?;
    running_single_batch_optimized(client, block_source, params, db_cache, db_data, batch_size)
        .await
}

async fn running_single_batch<P, ChT, CaT, DbT, TrErr, BlT>(
    client: &mut CompactTxStreamerClient<ChT>,
    block_source: &mut BlT,
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    batch_size: u32,
) -> Result<bool, Error<CaT::Error, <DbT as WalletRead>::Error, TrErr>>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    BlT: CompactBlockSource<ChT>,
{
    #[cfg(feature = "transparent-inputs")]
    for account_id in db_data.get_account_ids().map_err(Error::Wallet)? {
        let start_height = db_data
            .utxo_query_height(account_id)
            .map_err(Error::Wallet)?;
        info!(
            "Refreshing UTXOs for {:?} from height {}",
            account_id, start_height,
        );
        refresh_utxos(params, client, db_data, account_id, start_height).await?;
    }

    let mut did_work = false;
    let mut scan_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;

    loop {
        match scan_ranges.first() {
            Some(scan_range) if scan_range.priority() == ScanPriority::Verify => {
                did_work = true;
                let chain_state =
                    download_chain_state(client, scan_range.block_range().start - 1).await?;
                download_blocks(
                    client,
                    block_source,
                    db_cache,
                    scan_range,
                    Some(chain_state.block_hash()),
                )
                .await?;
                let scan_ranges_updated =
                    scan_blocks(params, db_cache, db_data, &chain_state, scan_range).await?;
                db_cache
                    .delete(scan_range.clone())
                    .await
                    .map_err(Error::Cache)?;

                if scan_ranges_updated {
                    scan_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    let scan_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;
    debug!("Suggested ranges: {:?}", scan_ranges);

    let first_batch = scan_ranges
        .into_iter()
        .flat_map(|r| {
            (0..).scan(r, |acc, _| {
                if acc.is_empty() {
                    None
                } else if let Some((cur, next)) = acc.split_at(acc.block_range().start + batch_size)
                {
                    *acc = next;
                    Some(cur)
                } else {
                    let cur = acc.clone();
                    let end = acc.block_range().end;
                    *acc = ScanRange::from_parts(end..end, acc.priority());
                    Some(cur)
                }
            })
        })
        .next();

    match first_batch {
        Some(scan_range) => {
            let chain_state =
                download_chain_state(client, scan_range.block_range().start - 1).await?;
            download_blocks(
                client,
                block_source,
                db_cache,
                &scan_range,
                Some(chain_state.block_hash()),
            )
            .await?;
            scan_blocks(params, db_cache, db_data, &chain_state, &scan_range).await?;
            db_cache.delete(scan_range).await.map_err(Error::Cache)?;
            Ok(true)
        }
        None => Ok(did_work),
    }
}

#[cfg(feature = "sync")]
async fn running_single_batch_optimized<P, ChT, CaT, DbT, TrErr, BlT>(
    client: &mut CompactTxStreamerClient<ChT>,
    block_source: &mut BlT,
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    batch_size: u32,
) -> Result<bool, Error<CaT::Error, <DbT as WalletRead>::Error, TrErr>>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    BlT: CompactBlockSource<ChT>,
{
    #[cfg(feature = "transparent-inputs")]
    for account_id in db_data.get_account_ids().map_err(Error::Wallet)? {
        let start_height = db_data
            .utxo_query_height(account_id)
            .map_err(Error::Wallet)?;
        info!(
            "Refreshing UTXOs for {:?} from height {}",
            account_id, start_height,
        );
        refresh_utxos(params, client, db_data, account_id, start_height).await?;
    }

    let mut did_work = false;
    let mut scan_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;

    loop {
        match scan_ranges.first() {
            Some(scan_range) if scan_range.priority() == ScanPriority::Verify => {
                did_work = true;
                let chain_state =
                    download_chain_state(client, scan_range.block_range().start - 1).await?;
                download_blocks(
                    client,
                    block_source,
                    db_cache,
                    scan_range,
                    Some(chain_state.block_hash()),
                )
                .await?;
                let scan_ranges_updated =
                    scan_blocks_optimized(params, db_cache, db_data, &chain_state, scan_range)
                        .await?;
                db_cache
                    .delete(scan_range.clone())
                    .await
                    .map_err(Error::Cache)?;

                if scan_ranges_updated {
                    scan_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    let scan_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;
    debug!("Suggested ranges: {:?}", scan_ranges);

    let first_batch = scan_ranges
        .into_iter()
        .flat_map(|r| {
            (0..).scan(r, |acc, _| {
                if acc.is_empty() {
                    None
                } else if let Some((cur, next)) = acc.split_at(acc.block_range().start + batch_size)
                {
                    *acc = next;
                    Some(cur)
                } else {
                    let cur = acc.clone();
                    let end = acc.block_range().end;
                    *acc = ScanRange::from_parts(end..end, acc.priority());
                    Some(cur)
                }
            })
        })
        .next();

    match first_batch {
        Some(scan_range) => {
            let chain_state =
                download_chain_state(client, scan_range.block_range().start - 1).await?;
            download_blocks(
                client,
                block_source,
                db_cache,
                &scan_range,
                Some(chain_state.block_hash()),
            )
            .await?;
            scan_blocks_optimized(params, db_cache, db_data, &chain_state, &scan_range).await?;
            db_cache.delete(scan_range).await.map_err(Error::Cache)?;
            Ok(true)
        }
        None => Ok(did_work),
    }
}

async fn running<P, ChT, CaT, DbT, TrErr, BlT>(
    client: &mut CompactTxStreamerClient<ChT>,
    block_source: &mut BlT,
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    batch_size: u32,
) -> Result<bool, Error<CaT::Error, <DbT as WalletRead>::Error, TrErr>>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    BlT: CompactBlockSource<ChT>,
{
    // 3) Download chain tip metadata from lightwalletd
    // 4) Notify the wallet of the updated chain tip.
    // Refresh UTXOs for the accounts in the wallet. We do this before we perform
    // any shielded scanning, to ensure that we discover any UTXOs between the old
    // fully-scanned height and the current chain tip.
    #[cfg(feature = "transparent-inputs")]
    for account_id in db_data.get_account_ids().map_err(Error::Wallet)? {
        let start_height = db_data
            .utxo_query_height(account_id)
            .map_err(Error::Wallet)?;
        info!(
            "Refreshing UTXOs for {:?} from height {}",
            account_id, start_height,
        );
        refresh_utxos(params, client, db_data, account_id, start_height).await?;
    }

    // 5) Get the suggested scan ranges from the wallet database
    let mut scan_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;

    // Store the handles to cached block deletions (which we spawn into separate
    // tasks to allow us to continue downloading and scanning other ranges).
    let mut block_deletions = vec![];

    // 6) Run the following loop until the wallet's view of the chain tip as of
    //    the previous wallet session is valid.
    loop {
        // If there is a range of blocks that needs to be verified, it will always
        // be returned as the first element of the vector of suggested ranges.
        match scan_ranges.first() {
            Some(scan_range) if scan_range.priority() == ScanPriority::Verify => {
                // Download the blocks in `scan_range` into the block source,
                // overwriting any existing blocks in this range.
                let chain_state =
                    download_chain_state(client, scan_range.block_range().start - 1).await?;
                download_blocks(
                    client,
                    block_source,
                    db_cache,
                    scan_range,
                    Some(chain_state.block_hash()),
                )
                .await?;

                // Scan the downloaded blocks and check for scanning errors that
                // indicate the wallet's chain tip is out of sync with blockchain
                // history.
                let scan_ranges_updated =
                    scan_blocks(params, db_cache, db_data, &chain_state, scan_range).await?;

                // Delete the now-scanned blocks, because keeping the entire chain
                // in CompactBlock files on disk is horrendous for the filesystem.
                block_deletions.push(db_cache.delete(scan_range.clone()));

                if scan_ranges_updated {
                    // The suggested scan ranges have been updated, so we re-request.
                    scan_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;
                } else {
                    // At this point, the cache and scanned data are locally
                    // consistent (though not necessarily consistent with the
                    // latest chain tip - this would be discovered the next time
                    // this codepath is executed after new blocks are received) so
                    // we can break out of the loop.
                    break;
                }
            }
            _ => {
                // Nothing to verify; break out of the loop
                break;
            }
        }
    }

    // 7) Loop over the remaining suggested scan ranges, retrieving the requested data
    //    and calling `scan_cached_blocks` on each range.
    let scan_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;
    debug!("Suggested ranges: {:?}", scan_ranges);
    for scan_range in scan_ranges.into_iter().flat_map(|r| {
        // Limit the number of blocks we download and scan at any one time.
        (0..).scan(r, |acc, _| {
            if acc.is_empty() {
                None
            } else if let Some((cur, next)) = acc.split_at(acc.block_range().start + batch_size) {
                *acc = next;
                Some(cur)
            } else {
                let cur = acc.clone();
                let end = acc.block_range().end;
                *acc = ScanRange::from_parts(end..end, acc.priority());
                Some(cur)
            }
        })
    }) {
        // Download the blocks in `scan_range` into the block source.
        let chain_state = download_chain_state(client, scan_range.block_range().start - 1).await?;
        download_blocks(
            client,
            block_source,
            db_cache,
            &scan_range,
            Some(chain_state.block_hash()),
        )
        .await?;

        // Scan the downloaded blocks.
        let scan_ranges_updated =
            scan_blocks(params, db_cache, db_data, &chain_state, &scan_range).await?;

        // Delete the now-scanned blocks.
        block_deletions.push(db_cache.delete(scan_range));

        if scan_ranges_updated {
            // The suggested scan ranges have been updated (either due to a continuity
            // error or because a higher priority range has been added).
            info!("Waiting for cached blocks to be deleted...");
            for deletion in block_deletions {
                deletion.await.map_err(Error::Cache)?;
            }
            return Ok(true);
        }
    }

    info!("Waiting for cached blocks to be deleted...");
    for deletion in block_deletions {
        deletion.await.map_err(Error::Cache)?;
    }
    Ok(false)
}

#[cfg(feature = "sync")]
async fn download_subtree_roots<ChT, H>(
    client: &mut CompactTxStreamerClient<ChT>,
    protocol: service::ShieldedProtocol,
) -> Result<Vec<CommitmentTreeRoot<H>>, tonic::Status>
where
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    H: HashSer,
{
    let mut request = service::GetSubtreeRootsArg::default();
    request.set_shielded_protocol(protocol);

    client
        .get_subtree_roots(request)
        .await?
        .into_inner()
        .and_then(|root| async move {
            let root_hash = H::read(&root.root_hash[..])?;
            Ok(CommitmentTreeRoot::from_parts(
                BlockHeight::from_u32(root.completing_block_height as u32),
                root_hash,
            ))
        })
        .try_collect()
        .await
}

#[cfg(feature = "sync")]
async fn update_subtree_roots<P, ChT, DbT, CaErr, DbErr>(
    client: &mut CompactTxStreamerClient<ChT>,
    params: &P,
    tip_height: BlockHeight,
    db_data: &mut DbT,
) -> Result<(), Error<CaErr, DbErr, <DbT as WalletCommitmentTrees>::Error>>
where
    P: Parameters,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    DbT: WalletCommitmentTrees,
    <DbT as WalletCommitmentTrees>::Error: std::error::Error + Send + Sync + 'static,
{
    let sapling_roots: Vec<CommitmentTreeRoot<sapling::Node>> =
        download_subtree_roots(client, service::ShieldedProtocol::Sapling).await?;

    info!("Sapling tree has {} subtrees", sapling_roots.len());
    db_data
        .put_sapling_subtree_roots(0, &sapling_roots)
        .map_err(Error::WalletTrees)?;

    #[cfg(feature = "orchard")]
    {
        let orchard_roots: Vec<CommitmentTreeRoot<MerkleHashOrchard>> =
            download_subtree_roots(client, service::ShieldedProtocol::Orchard).await?;

        info!("Orchard tree has {} subtrees", orchard_roots.len());
        db_data
            .put_orchard_subtree_roots(0, &orchard_roots)
            .map_err(Error::WalletTrees)?;

        if params.is_nu_active(NetworkUpgrade::Nu6_3, tip_height) {
            let ironwood_roots: Vec<CommitmentTreeRoot<MerkleHashOrchard>> =
                download_subtree_roots(client, service::ShieldedProtocol::Ironwood).await?;

            info!("Ironwood tree has {} subtrees", ironwood_roots.len());
            db_data
                .put_ironwood_subtree_roots(0, &ironwood_roots)
                .map_err(Error::WalletTrees)?;
        }
    }

    Ok(())
}

#[cfg(feature = "sync")]
async fn update_chain_tip<ChT, DbT, CaErr, TrErr>(
    client: &mut CompactTxStreamerClient<ChT>,
    db_data: &mut DbT,
) -> Result<BlockHeight, Error<CaErr, <DbT as WalletRead>::Error, TrErr>>
where
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    DbT: WalletWrite,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
{
    let tip_height: BlockHeight = client
        .get_latest_block(service::ChainSpec::default())
        .await?
        .get_ref()
        .height
        .try_into()
        .map_err(|_| Error::MisbehavingServer)?;

    info!("Latest block height is {}", tip_height);
    db_data
        .update_chain_tip(tip_height)
        .map_err(Error::Wallet)?;

    Ok(tip_height)
}

#[cfg(feature = "sync")]
async fn download_blocks<ChT, CaT, DbErr, TrErr, BlT>(
    client: &mut CompactTxStreamerClient<ChT>,
    block_source: &mut BlT,
    db_cache: &CaT,
    scan_range: &ScanRange,
    expected_prev_hash: Option<BlockHash>,
) -> Result<(), Error<CaT::Error, DbErr, TrErr>>
where
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    BlT: CompactBlockSource<ChT>,
{
    info!("Fetching {}", scan_range);
    let mut start = service::BlockId::default();
    start.height = scan_range.block_range().start.into();
    let mut end = service::BlockId::default();
    end.height = (scan_range.block_range().end - 1).into();
    let range = service::BlockRange {
        start: Some(start),
        end: Some(end),
        pool_types: vec![],
    };
    let compact_blocks = block_source
        .get_block_range(client, range, expected_prev_hash)
        .await?;

    db_cache
        .insert(compact_blocks)
        .await
        .map_err(Error::Cache)?;

    Ok(())
}

#[cfg(feature = "sync")]
async fn download_chain_state<ChT, CaErr, DbErr, TrErr>(
    client: &mut CompactTxStreamerClient<ChT>,
    block_height: BlockHeight,
) -> Result<ChainState, Error<CaErr, DbErr, TrErr>>
where
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
{
    let tree_state = client
        .get_tree_state(BlockId {
            height: block_height.into(),
            hash: vec![],
        })
        .await?;

    tree_state
        .into_inner()
        .to_chain_state()
        .map_err(|_| Error::MisbehavingServer)
}

/// Scans the given block range and checks for scanning errors that indicate the wallet's
/// chain tip is out of sync with blockchain history.
///
/// Returns `true` if scanning these blocks materially changed the suggested scan ranges.
#[cfg(feature = "sync")]
async fn scan_blocks<P, CaT, DbT, TrErr>(
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    initial_chain_state: &ChainState,
    scan_range: &ScanRange,
) -> Result<bool, Error<CaT::Error, <DbT as WalletRead>::Error, TrErr>>
where
    P: Parameters + Send + 'static,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
{
    info!("Scanning {}", scan_range);
    let scan_result = scan_cached_blocks(
        params,
        db_cache,
        db_data,
        scan_range.block_range().start,
        initial_chain_state,
        scan_range.len(),
    );

    match scan_result {
        Err(ChainError::Scan(err)) if err.is_continuity_error() => {
            // Pick a height to rewind to, which must be at least one block before the
            // height at which the error occurred, but may be an earlier height determined
            // based on heuristics such as the platform, available bandwidth, size of
            // recent CompactBlocks, etc.
            let rewind_height = err.at_height().saturating_sub(10);
            info!(
                "Chain reorg detected at {}, rewinding to {}",
                err.at_height(),
                rewind_height,
            );

            // Rewind to the chosen height.
            db_data
                .truncate_to_height(rewind_height)
                .map_err(Error::Wallet)?;

            // Delete cached blocks from rewind_height onwards.
            //
            // This does imply that assumed-valid blocks will be re-downloaded, but it is
            // also possible that in the intervening time, a chain reorg has occurred that
            // orphaned some of those blocks.
            db_cache
                .truncate(rewind_height)
                .await
                .map_err(Error::Cache)?;

            // The database was truncated, invalidating prior suggested ranges.
            Ok(true)
        }
        Ok(_) => {
            // If scanning these blocks caused a suggested range to be added that has a
            // higher priority than the current range, invalidate the current ranges.
            let latest_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;

            Ok(if let Some(range) = latest_ranges.first() {
                range.priority() > scan_range.priority()
            } else {
                false
            })
        }
        Err(e) => Err(e.into()),
    }
}

async fn scan_blocks_optimized<P, CaT, DbT, TrErr>(
    params: &P,
    db_cache: &CaT,
    db_data: &mut DbT,
    initial_chain_state: &ChainState,
    scan_range: &ScanRange,
) -> Result<bool, Error<CaT::Error, <DbT as WalletRead>::Error, TrErr>>
where
    P: Parameters + Send + 'static,
    CaT: BlockCache,
    CaT::Error: std::error::Error + Send + Sync + 'static,
    DbT: WalletWrite,
    <DbT as WalletRead>::AccountId: ConditionallySelectable + Default + Send + Sync + 'static,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
{
    info!("Scanning {}", scan_range);
    let scan_result = scan_cached_blocks_optimized(
        params,
        db_cache,
        db_data,
        scan_range.block_range().start,
        initial_chain_state,
        scan_range.len(),
    );

    match scan_result {
        Err(ChainError::Scan(err)) if err.is_continuity_error() => {
            // Pick a height to rewind to, which must be at least one block before the
            // height at which the error occurred, but may be an earlier height determined
            // based on heuristics such as the platform, available bandwidth, size of
            // recent CompactBlocks, etc.
            let rewind_height = err.at_height().saturating_sub(10);
            info!(
                "Chain reorg detected at {}, rewinding to {}",
                err.at_height(),
                rewind_height,
            );

            // Rewind to the chosen height.
            db_data
                .truncate_to_height(rewind_height)
                .map_err(Error::Wallet)?;

            // Delete cached blocks from rewind_height onwards.
            //
            // This does imply that assumed-valid blocks will be re-downloaded, but it is
            // also possible that in the intervening time, a chain reorg has occurred that
            // orphaned some of those blocks.
            db_cache
                .truncate(rewind_height)
                .await
                .map_err(Error::Cache)?;

            // The database was truncated, invalidating prior suggested ranges.
            Ok(true)
        }
        Ok(_) => {
            // If scanning these blocks caused a suggested range to be added that has a
            // higher priority than the current range, invalidate the current ranges.
            let latest_ranges = db_data.suggest_scan_ranges().map_err(Error::Wallet)?;

            Ok(if let Some(range) = latest_ranges.first() {
                range.priority() > scan_range.priority()
            } else {
                false
            })
        }
        Err(e) => Err(e.into()),
    }
}

/// Refreshes the given account's view of UTXOs that exist starting at the given height.
///
/// ## Note about UTXO tracking
///
/// (Extracted from [a comment in the Android SDK].)
///
/// We no longer clear UTXOs here, as `WalletDb::put_received_transparent_utxo` now uses
/// an upsert instead of an insert. This means that now-spent UTXOs would previously have
/// been deleted, but now are left in the database (like shielded notes).
///
/// Due to the fact that the `lightwalletd` query only returns _current_ UTXOs, we don't
/// learn about recently-spent UTXOs here, so the transparent balance does not get updated
/// here.
///
/// Instead, when a received shielded note is "enhanced" by downloading the full
/// transaction, we mark any UTXOs spent in that transaction as spent in the database.
/// This relies on two current properties:
/// - UTXOs are only ever spent in shielding transactions.
/// - At least one shielded note from each shielding transaction is always enhanced.
///
/// However, for greater reliability, we may want to alter the Data Access API to support
/// "inferring spentness" from what is _not_ returned as a UTXO, or alternatively fetch
/// TXOs from `lightwalletd` instead of just UTXOs.
///
/// [a comment in the Android SDK]: https://github.com/Electric-Coin-Company/zcash-android-wallet-sdk/blob/855204fc8ae4057fdac939f98df4aa38c8e662f1/sdk-lib/src/main/java/cash/z/ecc/android/sdk/block/processor/CompactBlockProcessor.kt#L979-L991
#[cfg(all(feature = "sync", feature = "transparent-inputs"))]
pub async fn refresh_utxos<P, ChT, DbT, CaErr, TrErr>(
    params: &P,
    client: &mut CompactTxStreamerClient<ChT>,
    db_data: &mut DbT,
    account_id: <DbT as WalletRead>::AccountId,
    start_height: BlockHeight,
) -> Result<(), Error<CaErr, <DbT as WalletRead>::Error, TrErr>>
where
    P: Parameters + Send + 'static,
    ChT: GrpcService<TonicBody>,
    ChT::Error: Into<StdError>,
    ChT::ResponseBody: Body<Data = Bytes> + Send + 'static,
    <ChT::ResponseBody as Body>::Error: Into<StdError> + Send,
    DbT: WalletWrite,
    <DbT as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
{
    let request = service::GetAddressUtxosArg {
        addresses: db_data
            .get_transparent_receivers(account_id, true, true)
            .map_err(Error::Wallet)?
            .into_keys()
            .map(|addr| addr.encode(params))
            .collect(),
        start_height: start_height.into(),
        max_entries: 0,
    };

    if request.addresses.is_empty() {
        info!("{:?} has no transparent receivers", account_id);
    } else {
        client
            .get_address_utxos_stream(request)
            .await?
            .into_inner()
            .map_err(Error::Server)
            .and_then(|reply| async move {
                WalletTransparentOutput::from_parts(
                    OutPoint::new(
                        reply.txid[..]
                            .try_into()
                            .map_err(|_| Error::MisbehavingServer)?,
                        reply
                            .index
                            .try_into()
                            .map_err(|_| Error::MisbehavingServer)?,
                    ),
                    TxOut::new(
                        Zatoshis::from_nonnegative_i64(reply.value_zat)
                            .map_err(|_| Error::MisbehavingServer)?,
                        Script(script::Code(reply.script)),
                    ),
                    Some(
                        BlockHeight::try_from(reply.height)
                            .map_err(|_| Error::MisbehavingServer)?,
                    ),
                    Some(account_id),
                    None,
                    None,
                )
                .ok_or(Error::MisbehavingServer)
            })
            .try_for_each(|output| {
                let res = db_data.put_received_transparent_utxo(&output).map(|_| ());
                async move { res.map_err(Error::Wallet) }
            })
            .await?;
    }

    Ok(())
}

/// Errors that can occur while syncing.
#[cfg(feature = "sync")]
#[derive(Debug)]
#[non_exhaustive]
pub enum Error<CaErr, DbErr, TrErr> {
    /// An error while interacting with a [`BlockCache`].
    Cache(CaErr),
    /// The lightwalletd server returned invalid information, and is misbehaving.
    MisbehavingServer,
    /// An error while scanning blocks.
    Scan(ScanError),
    /// An error while communicating with the lightwalletd server.
    Server(tonic::Status),
    /// An error while interacting with a wallet database via [`WalletRead`] or
    /// [`WalletWrite`].
    Wallet(DbErr),
    /// An error while interacting with a wallet database via [`WalletCommitmentTrees`].
    WalletTrees(ShardTreeError<TrErr>),
}

#[cfg(feature = "sync")]
impl<CaErr, DbErr, TrErr> fmt::Display for Error<CaErr, DbErr, TrErr>
where
    CaErr: fmt::Display,
    DbErr: fmt::Display,
    TrErr: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Cache(e) => write!(f, "Error while interacting with block cache: {e}"),
            Error::MisbehavingServer => write!(f, "lightwalletd server is misbehaving"),
            Error::Scan(e) => write!(f, "Error while scanning blocks: {e}"),
            Error::Server(e) => {
                write!(f, "Error while communicating with lightwalletd server: {e}")
            }
            Error::Wallet(e) => write!(f, "Error while interacting with wallet database: {e}"),
            Error::WalletTrees(e) => write!(
                f,
                "Error while interacting with wallet commitment trees: {e}"
            ),
        }
    }
}

#[cfg(feature = "sync")]
impl<CaErr, DbErr, TrErr> std::error::Error for Error<CaErr, DbErr, TrErr>
where
    CaErr: std::error::Error,
    DbErr: std::error::Error,
    TrErr: std::error::Error,
{
}

#[cfg(feature = "sync")]
impl<CaErr, DbErr, TrErr> From<ChainError<DbErr, CaErr>> for Error<CaErr, DbErr, TrErr> {
    fn from(e: ChainError<DbErr, CaErr>) -> Self {
        match e {
            ChainError::Wallet(e) => Error::Wallet(e),
            ChainError::BlockSource(e) => Error::Cache(e),
            ChainError::Scan(e) => Error::Scan(e),
        }
    }
}

#[cfg(feature = "sync")]
impl<CaErr, DbErr, TrErr> From<tonic::Status> for Error<CaErr, DbErr, TrErr> {
    fn from(status: tonic::Status) -> Self {
        Error::Server(status)
    }
}

#[cfg(all(test, feature = "sync", feature = "orchard"))]
mod tests {
    use std::{
        convert::Infallible,
        future::{Ready, ready},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll},
    };

    use tonic::{
        body::Body as TonicBody,
        codegen::{Service, http::Request, http::Response},
    };
    use zcash_protocol::consensus::{BlockHeight, Network};

    use crate::{
        data_api::testing::MockWalletDb,
        proto::service::compact_tx_streamer_client::CompactTxStreamerClient,
    };

    use super::update_subtree_roots;

    #[derive(Clone)]
    struct CountingGrpcService(Arc<AtomicUsize>);

    impl Service<Request<TonicBody>> for CountingGrpcService {
        type Response = Response<TonicBody>;
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _request: Request<TonicBody>) -> Self::Future {
            self.0.fetch_add(1, Ordering::Relaxed);
            ready(Ok(Response::builder()
                .header("content-type", "application/grpc")
                .header("grpc-status", "0")
                .body(TonicBody::empty())
                .unwrap()))
        }
    }

    fn subtree_root_request_count(tip_height: u32) -> usize {
        let request_count = Arc::new(AtomicUsize::new(0));
        let mut client =
            CompactTxStreamerClient::new(CountingGrpcService(Arc::clone(&request_count)));
        let network = Network::MainNetwork;
        let mut wallet = MockWalletDb::new(network);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            update_subtree_roots::<_, _, _, Infallible, Infallible>(
                &mut client,
                &network,
                BlockHeight::from_u32(tip_height),
                &mut wallet,
            )
            .await
            .unwrap();
        });

        request_count.load(Ordering::Relaxed)
    }

    #[test]
    fn subtree_root_sync_skips_ironwood_before_activation() {
        assert_eq!(subtree_root_request_count(3_428_142), 2);
    }

    #[test]
    fn subtree_root_sync_requests_ironwood_at_activation() {
        assert_eq!(subtree_root_request_count(3_428_143), 3);
    }
}
