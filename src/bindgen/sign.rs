use crate::bindgen::{SeedFingerprint, UnifiedSpendingKey};
use crate::error::Error;
use crate::{Network, bindgen::Pczt};
use pczt::roles::signer::Signer;
use pczt::roles::verifier::Verifier;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::str::FromStr;
use wasm_bindgen::prelude::wasm_bindgen;
use zcash_primitives::consensus::{NetworkConstants, Parameters};
use zcash_primitives::legacy::keys::{NonHardenedChildIndex, TransparentKeyScope};

#[wasm_bindgen]
pub async fn pczt_sign(
    network: &str,
    pczt: Pczt,
    usk: UnifiedSpendingKey,
    seed_fp: SeedFingerprint,
) -> Result<Pczt, Error> {
    pczt_sign_inner(
        Network::from_str(network)?,
        pczt.into(),
        usk.into(),
        seed_fp.into(),
    )
    .await
    .map(Into::into)
}

pub async fn pczt_sign_inner(
    network: Network,
    pczt: pczt::Pczt,
    usk: zcash_keys::keys::UnifiedSpendingKey,
    seed_fp: zip32::fingerprint::SeedFingerprint,
) -> Result<pczt::Pczt, Error> {
    // Find all the spends matching our seed.
    #[derive(Debug)]
    enum KeyRef {
        Orchard {
            index: usize,
        },
        Sapling {
            index: usize,
        },
        Transparent {
            index: usize,
            scope: TransparentKeyScope,
            address_index: NonHardenedChildIndex,
        },
    }
    let mut keys = BTreeMap::<zcash_primitives::zip32::AccountId, Vec<KeyRef>>::new();
    let pczt = Verifier::new(pczt)
        .with_orchard::<Infallible, _>(|bundle| {
            for (index, action) in bundle.actions().iter().enumerate() {
                if let Some(account_index) =
                    action
                        .spend()
                        .zip32_derivation()
                        .as_ref()
                        .and_then(|derivation| {
                            derivation.extract_account_index(
                                &seed_fp,
                                zcash_primitives::zip32::ChildIndex::hardened(
                                    network.network_type().coin_type(),
                                ),
                            )
                        })
                {
                    keys.entry(account_index)
                        .or_default()
                        .push(KeyRef::Orchard { index });
                }
            }
            Ok(())
        })
        .map_err(|e| Error::PcztSign(format!("Invalid PCZT: {:?}", e)))?
        .with_sapling::<Infallible, _>(|bundle| {
            for (index, spend) in bundle.spends().iter().enumerate() {
                if let Some(account_index) =
                    spend.zip32_derivation().as_ref().and_then(|derivation| {
                        derivation.extract_account_index(
                            &seed_fp,
                            zcash_primitives::zip32::ChildIndex::hardened(
                                network.network_type().coin_type(),
                            ),
                        )
                    })
                {
                    keys.entry(account_index)
                        .or_default()
                        .push(KeyRef::Sapling { index });
                }
            }
            Ok(())
        })
        .map_err(|e| Error::PcztSign(format!("Invalid PCZT: {:?}", e)))?
        .with_transparent::<Infallible, _>(|bundle| {
            for (index, input) in bundle.inputs().iter().enumerate() {
                for derivation in input.bip32_derivation().values() {
                    // The HARDENED_FLAG constant (2^31)
                    const HARDENED_FLAG: u32 = 1 << 31;

                    // Create ChildNumber directly from u32 using the internal representation
                    // This matches the coin_type with hardened flag set
                    let coin_type_value = network.network_type().coin_type() | HARDENED_FLAG;

                    if let Some((account_index, scope, address_index)) =
                        derivation.extract_bip_44_fields(&seed_fp, coin_type_value.into())
                    {
                        keys.entry(account_index)
                            .or_default()
                            .push(KeyRef::Transparent {
                                index,
                                scope,
                                address_index,
                            });
                    }
                }
            }
            Ok(())
        })
        .map_err(|e| Error::PcztSign(format!("Invalid PCZT: {:?}", e)))?
        .finish();

    // Add proof generation key for Sapling spends before signing
    // This is required for sapling-crypto 0.5+ / pczt 0.5+
    let pczt = {
        use pczt::roles::updater::Updater;

        let sapling_proof_gen_key = usk.sapling().expsk.proof_generation_key();

        Updater::new(pczt)
            .update_sapling_with(|mut updater| {
                // Find all spends that need proof generation key
                let spends_needing_key: Vec<usize> = updater
                    .bundle()
                    .spends()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, spend)| {
                        // Only add if not already set
                        spend.proof_generation_key().is_none().then_some(index)
                    })
                    .collect();

                // Add proof generation key to each spend
                for index in spends_needing_key {
                    updater.update_spend_with(index, |mut spend_updater| {
                        spend_updater.set_proof_generation_key(sapling_proof_gen_key.clone())
                    })?;
                }

                Ok(())
            })
            .map_err(|e| Error::PcztSign(format!("Failed to add proof generation key: {:?}", e)))?
            .finish()
    };

    let mut signer = Signer::new(pczt).unwrap();
    //.map_err(|e| anyhow!("Failed to initialize Signer: {:?}", e))?;
    for (_, spends) in keys {
        // let usk = UnifiedSpendingKey::from_seed(&params, seed, account_index)?;
        for keyref in spends {
            match keyref {
                KeyRef::Orchard { index } => {
                    signer
                        .sign_orchard(
                            index,
                            &orchard::keys::SpendAuthorizingKey::from(usk.orchard()),
                        )
                        .map_err(|e| {
                            Error::PcztSign(format!(
                                "Failed to sign Orchard spend {index}: {:?}",
                                e
                            ))
                        })?;
                }
                KeyRef::Sapling { index } => {
                    signer
                        .sign_sapling(index, &usk.sapling().expsk.ask)
                        .map_err(|e| {
                            Error::PcztSign(format!(
                                "Failed to sign Sapling spend {index}: {:?}",
                                e
                            ))
                        })?;
                }
                KeyRef::Transparent {
                    index,
                    scope,
                    address_index,
                } => {
                    signer
                        .sign_transparent(
                            index,
                            &usk.transparent()
                                .derive_secret_key(scope, address_index)
                                .map_err(|e| {
                                    Error::PcztSign(format!(
                                        "Failed to derive transparent key at .../{:?}/{:?}: {:?}",
                                        scope, address_index, e,
                                    ))
                                })?,
                        )
                        .map_err(|e| {
                            Error::PcztSign(format!(
                                "Failed to sign transparent input {index}: {:?}",
                                e
                            ))
                        })?;
                }
            }
        }
    }

    Ok(signer.finish())
}

fn encode_compact_size(size: usize) -> Vec<u8> {
    let mut result = Vec::new();
    match size {
        0x00..=0xfc => {
            result.push(size as u8);
        }
        0x00fd..=0xffff => {
            result.push(0xfd);
            result.extend_from_slice(&(size as u16).to_le_bytes());
        }
        0x0001_0000..=0xffff_ffff => {
            result.push(0xfe);
            result.extend_from_slice(&(size as u32).to_le_bytes());
        }
        _ => {
            result.push(0xff);
            result.extend_from_slice(&(size as u64).to_le_bytes());
        }
    }
    result
}

/// Sign a message with the transparent address private key
///
/// This implements the Bitcoin/Zcash message signing convention compatible with
/// the `signmessage` RPC call.
///
/// # Arguments
///
/// * `network` - Network type ("main" or "test")
/// * `usk` - The Unified Spending Key
/// * `message` - The message to sign
///
/// # Returns
///
/// Returns a JSON object with:
/// - signature: Hex-encoded signature (65 bytes: recovery_id + r + s)
/// - pubkey: Hex-encoded uncompressed public key (65 bytes: 04 + x + y)
/// - address: The transparent address used for signing
///
/// # Example
///
/// ```javascript
/// const result = await sign_message_with_transparent("main", usk, "Hello Zcash!");
/// console.log("Signature:", result.signature);  // hex format
/// console.log("Public Key:", result.pubkey);     // uncompressed hex (65 bytes)
/// console.log("Address:", result.address);       // t-address
/// ```
#[wasm_bindgen]
pub fn sign_message_with_transparent(
    network: &str,
    usk: UnifiedSpendingKey,
    message: &str,
) -> Result<wasm_bindgen::JsValue, Error> {
    use bitcoin_hashes::{Hash, sha256d};
    use secp256k1::{Message as Secp256k1Message, Secp256k1, SecretKey};
    use serde::{Deserialize, Serialize};
    use zcash_client_backend::encoding::AddressCodec;
    use zcash_primitives::legacy::keys::{IncomingViewingKey, NonHardenedChildIndex};

    #[derive(Serialize, Deserialize)]
    struct SignatureResult {
        signature: String,
        pubkey: String,
        address: String,
    }

    let net = Network::from_str(network)?;
    let usk_inner: zcash_keys::keys::UnifiedSpendingKey = usk.into();

    // Get transparent private key
    let transparent_key = usk_inner.transparent();

    // Derive the external address at index 0 (default receiving address)
    let address_index = NonHardenedChildIndex::ZERO;
    let privkey = transparent_key
        .derive_external_secret_key(address_index)
        .map_err(|e| Error::Generic(format!("Failed to derive private key: {:?}", e)))?;

    // Get the address
    let address_obj = transparent_key
        .to_account_pubkey()
        .derive_external_ivk()
        .map_err(|e| Error::Generic(format!("Failed to derive IVK: {:?}", e)))?
        .derive_address(address_index)
        .map_err(|e| Error::Generic(format!("Failed to derive address: {:?}", e)))?;

    let address_str = address_obj.encode(&net);

    // Convert to secp256k1 SecretKey
    let secret_key = SecretKey::from_slice(privkey.as_ref())
        .map_err(|e| Error::Generic(format!("Invalid private key: {}", e)))?;

    let secp = Secp256k1::new();

    // Get the uncompressed public key (65 bytes: 04 + x + y)
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
    let pubkey_uncompressed = public_key.serialize_uncompressed();
    let pubkey_hex = hex::encode(pubkey_uncompressed);

    // Create message hash following Zcash message signing convention
    // "Zcash Signed Message:\n" + message
    let mut msg_bytes = Vec::new();
    let prefix = "Zcash Signed Message:\n";
    msg_bytes.extend_from_slice(&encode_compact_size(prefix.len()));
    msg_bytes.extend_from_slice(prefix.as_bytes());
    msg_bytes.extend_from_slice(&encode_compact_size(message.len()));
    msg_bytes.extend_from_slice(message.as_bytes());

    // Double SHA256
    let hash = sha256d::Hash::hash(&msg_bytes);
    let message_hash = Secp256k1Message::from_slice(hash.as_ref())
        .map_err(|e| Error::Generic(format!("Failed to create message: {}", e)))?;

    // Sign with recovery ID
    let signature = secp.sign_ecdsa_recoverable(&message_hash, &secret_key);
    let (recovery_id, sig_bytes) = signature.serialize_compact();

    // For Zcash message signing:
    // - Uncompressed pubkey: recovery_id + 27 (range 27-30)
    // - Compressed pubkey: recovery_id + 31 (range 31-34)
    let recovery_byte = 27 + recovery_id.to_i32() as u8;

    // Combine recovery_id + signature bytes (65 bytes total)
    let mut full_sig = Vec::with_capacity(65);
    full_sig.push(recovery_byte);
    full_sig.extend_from_slice(&sig_bytes);
    let signature_hex = hex::encode(&full_sig);

    let result = SignatureResult {
        signature: signature_hex,
        pubkey: pubkey_hex,
        address: address_str,
    };

    Ok(serde_wasm_bindgen::to_value(&result)?)
}
