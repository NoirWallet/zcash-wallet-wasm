//! Synthetic vault transactions for tests.
//!
//! Builds an unproven, unsigned Ironwood PCZT whose spend authority is a given `ak`, using the stock
//! `zcash_primitives` builder. Real note selection, fee policy and change management belong to the
//! wallet; this exists so the coordinator and signer tiers — in Rust and in JavaScript — can be driven
//! against a genuine transaction.
//!
//! No prover: `circuits` is deliberately not enabled, so this crate stays light enough to compile into
//! a JS test harness.

use orchard::keys::{FullViewingKey, Scope};
use orchard::note::{NoteVersion, RandomSeed, Rho};
use orchard::tree::{MerkleHashOrchard, MerklePath};
use orchard::{Note, ValuePool};
use pczt::roles::creator::Creator;
use pczt::roles::io_finalizer::IoFinalizer;
use pczt::Pczt;
use zcash_primitives::transaction::builder::{BuildConfig, Builder, BundlePadding};
use zcash_primitives::transaction::fees::zip317;
use zcash_protocol::consensus::{Network, NetworkUpgrade, Parameters};
use zcash_protocol::memo::MemoBytes;
use zcash_protocol::value::Zatoshis;

/// Build one Ironwood spend of `spend_value` paying `pay_value` back to the same address.
///
/// Unpadded, so the transaction holds exactly one action and the arithmetic in a test stays obvious.
pub fn ironwood_pczt<R: rand::Rng + rand::CryptoRng>(
    fvk_bytes: &[u8; 96],
    pool: ValuePool,
    spend_value: u64,
    pay_value: u64,
    rng: R,
) -> Option<Pczt> {
    let network = Network::TestNetwork;
    let fvk = FullViewingKey::from_bytes(fvk_bytes)?;
    let recipient = fvk.address_at(0u32, Scope::External);
    let target_height = network.activation_height(NetworkUpgrade::Nu6_3)? + 1;

    let rho = Rho::from_bytes(&[1u8; 32]).into_option()?;
    let rseed =
        (0u8..=255).find_map(|byte| RandomSeed::from_bytes([byte; 32], &rho).into_option())?;
    let note = Note::from_parts(
        recipient,
        orchard::value::NoteValue::from_raw(spend_value),
        rho,
        rseed,
        NoteVersion::V3,
    )
    .into_option()?;

    let zero = MerkleHashOrchard::from_bytes(&[0; 32]).into_option()?;
    let merkle_path = MerklePath::from_parts(0, [zero; 32]);
    let anchor = merkle_path.root(note.commitment().into());

    let mut builder = Builder::new(
        network,
        target_height,
        BuildConfig::Standard {
            sapling_anchor: None,
            orchard_anchor: (pool == ValuePool::Orchard).then_some(anchor),
            ironwood_anchor: Some(anchor),
            orchard_padding: BundlePadding::DEFAULT,
            ironwood_padding: BundlePadding {
                bundle_required: true,
                pad_to_minimum: Some(1),
            },
        },
    );
    match pool {
        ValuePool::Orchard => {
            builder
                .add_orchard_spend::<zip317::FeeRule>(fvk.clone(), note, merkle_path)
                .ok()?;
        }
        ValuePool::Ironwood => {
            builder
                .add_ironwood_spend::<zip317::FeeRule>(fvk.clone(), note, merkle_path)
                .ok()?;
        }
    }
    builder
        .add_ironwood_output::<zip317::FeeRule>(
            Some(fvk.to_ovk(Scope::External)),
            recipient,
            Zatoshis::const_from_u64(pay_value),
            MemoBytes::empty(),
        )
        .ok()?;

    let parts = builder
        .build_for_pczt(rng, &zip317::FeeRule::standard())
        .ok()?;
    let creator = Creator::build_from_parts(parts.pczt_parts)?;
    IoFinalizer::new(creator).finalize_io(rand::rng()).ok()
}
