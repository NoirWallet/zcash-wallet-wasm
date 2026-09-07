use wasm_bindgen::prelude::*;

pub mod bindgen;
mod error;
mod init;
mod network;
mod wallet;

pub use error::*;
pub use network::*;
pub use wallet::*;

#[cfg(feature = "voting")]
pub use zcash_voting;

/// The maximum number of checkpoints to store in each shard-tree
pub const PRUNING_DEPTH: usize = 100;

// Requires atomics and SharedArrayBuffer support - see .cargo/config.toml
// Enable with feature: wasm-parallel
#[cfg(feature = "wasm-parallel")]
pub use wasm_bindgen_rayon::init_thread_pool;

#[wasm_bindgen]
pub struct BlockRange(pub u32, pub u32);
