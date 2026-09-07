use zcash_client_backend::{
    data_api::{
        OutputLockStore, WalletRead,
        locking::{LockError, LockFilter, LockOwner},
    },
    wallet::OutputRef,
};
use zcash_protocol::{
    PoolType,
    consensus::{self, BlockHeight},
};

use crate::{AccountId, Error, MemoryWalletDb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputLock {
    pub(crate) owner: LockOwner,
    pub(crate) expiry_height: BlockHeight,
}

impl<P: consensus::Parameters> MemoryWalletDb<P> {
    pub(crate) fn output_account(&self, output: &OutputRef) -> Option<AccountId> {
        match output.pool() {
            PoolType::Transparent => self
                .transparent_received_outputs
                .get(&transparent::bundle::OutPoint::new(
                    (*output.txid()).into(),
                    output.output_index(),
                ))
                .map(|output| output.account_id),
            PoolType::Shielded(_) => self
                .received_notes
                .iter()
                .find(|note| OutputRef::from(note.note_id) == *output)
                .map(|note| note.account_id),
        }
    }

    pub(crate) fn active_output_lock(
        &self,
        output: &OutputRef,
        height: BlockHeight,
    ) -> Option<&OutputLock> {
        self.output_locks
            .get(output)
            .filter(|lock| lock.expiry_height >= height)
    }

    pub(crate) fn lock_allows(
        &self,
        output: &OutputRef,
        height: BlockHeight,
        filter: LockFilter<'_>,
    ) -> bool {
        match (filter, self.active_output_lock(output, height)) {
            (LockFilter::Policy(policy), Some(lock)) => {
                policy.overridable_owners().contains(&lock.owner)
            }
            _ => true,
        }
    }

    pub(crate) fn lock_order(
        &self,
        output: &OutputRef,
        height: BlockHeight,
        filter: LockFilter<'_>,
    ) -> bool {
        match filter {
            LockFilter::Policy(policy) => {
                self.active_output_lock(output, height).is_some() != policy.prefers_locked()
            }
            LockFilter::Unfiltered => false,
        }
    }
}

impl<P: consensus::Parameters> OutputLockStore for MemoryWalletDb<P> {
    type Error = Error;
    type AccountId = AccountId;

    fn lock_outputs(
        &mut self,
        outputs: &[OutputRef],
        owner: LockOwner,
        expiry_height: BlockHeight,
    ) -> Result<usize, LockError<Error>> {
        let tip = self.chain_height().map_err(LockError::Storage)?;
        for output in outputs {
            if self.output_account(output).is_none()
                || self.output_locks.get(output).is_some_and(|lock| {
                    lock.owner != owner && tip.is_none_or(|tip| lock.expiry_height > tip)
                })
            {
                return Err(LockError::LockFailure(*output));
            }
        }
        for output in outputs {
            self.output_locks.insert(
                *output,
                OutputLock {
                    owner,
                    expiry_height,
                },
            );
        }
        Ok(outputs.len())
    }

    fn unlock_output(&mut self, output: &OutputRef, owner: LockOwner) -> Result<bool, Error> {
        if self
            .output_locks
            .get(output)
            .is_some_and(|lock| lock.owner == owner)
        {
            self.output_locks.remove(output);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn clear_locked_outputs(&mut self, account: AccountId) -> Result<usize, Error> {
        let outputs: Vec<_> = self
            .output_locks
            .keys()
            .filter(|output| self.output_account(output) == Some(account))
            .copied()
            .collect();
        for output in &outputs {
            self.output_locks.remove(output);
        }
        Ok(outputs.len())
    }

    fn get_locked_outputs(&self, account: AccountId) -> Result<Vec<OutputRef>, Error> {
        let tip = self.chain_height()?.ok_or(Error::ChainHeightUnknown)?;
        Ok(self
            .output_locks
            .iter()
            .filter(|(output, lock)| {
                lock.expiry_height > tip && self.output_account(output) == Some(account)
            })
            .map(|(output, _)| *output)
            .collect())
    }
}

#[cfg(all(test, feature = "transparent-inputs"))]
mod tests {
    use super::*;
    use crate::types::transparent::ReceivedTransparentOutput;
    use transparent::{
        address::TransparentAddress,
        bundle::{OutPoint, TxOut},
    };
    use zcash_protocol::{TxId, consensus::Network, value::Zatoshis};

    fn wallet() -> (MemoryWalletDb<Network>, AccountId, OutputRef, OutputRef) {
        let mut wallet = MemoryWalletDb::new(Network::TestNetwork, 100);
        wallet.scan_queue.0.push((
            1.into(),
            2.into(),
            zcash_client_backend::data_api::scanning::ScanPriority::Historic,
        ));
        let account = AccountId::from(0);
        let outputs = [
            OutputRef::new(TxId::from_bytes([1; 32]), PoolType::Transparent, 0),
            OutputRef::new(TxId::from_bytes([2; 32]), PoolType::Transparent, 0),
        ];
        for output in outputs {
            let address = TransparentAddress::PublicKeyHash([3; 20]);
            wallet.transparent_received_outputs.insert(
                OutPoint::new((*output.txid()).into(), 0),
                ReceivedTransparentOutput {
                    account_id: account,
                    transaction_id: *output.txid(),
                    address,
                    key_scope: transparent::keys::TransparentKeyScope::EXTERNAL,
                    txout: TxOut::new(Zatoshis::const_from_u64(50_000), address.script().into()),
                    max_observed_unspent_height: None,
                },
            );
        }
        (wallet, account, outputs[0], outputs[1])
    }

    #[test]
    fn locks_are_atomic_owned_and_persisted() {
        let (mut wallet, account, first, second) = wallet();
        let owner = LockOwner::new([4; 32]);
        let other = LockOwner::new([5; 32]);
        assert_eq!(
            wallet
                .lock_outputs(&[first, first], owner, 10.into())
                .unwrap(),
            2
        );
        assert!(
            matches!(wallet.lock_outputs(&[second, first], other, 20.into()), Err(LockError::LockFailure(output)) if output == first)
        );
        assert_eq!(wallet.get_locked_outputs(account).unwrap(), vec![first]);
        assert!(!wallet.unlock_output(&first, other).unwrap());
        assert_eq!(wallet.lock_outputs(&[first], owner, 20.into()).unwrap(), 1);
        let mut encoded = Vec::new();
        wallet.encode(&mut encoded).unwrap();
        assert_eq!(encoded.len(), wallet.encoded_len().unwrap());
        let mut restored =
            MemoryWalletDb::decode_new(encoded.as_slice(), Network::TestNetwork, 100).unwrap();
        assert_eq!(restored.output_locks, wallet.output_locks);
        assert!(restored.active_output_lock(&first, 20.into()).is_some());
        assert!(restored.active_output_lock(&first, 21.into()).is_none());
        restored
            .mark_transparent_output_spent(
                &TxId::from_bytes([8; 32]),
                &OutPoint::new((*first.txid()).into(), 0),
            )
            .unwrap();
        assert!(restored.get_locked_outputs(account).unwrap().is_empty());
        assert!(wallet.unlock_output(&first, owner).unwrap());
    }

    #[test]
    fn legacy_wallet_defaults_to_no_locks_and_rejects_bad_lock_records() {
        let (wallet, _, first, _) = wallet();
        let proto = crate::proto::memwallet::MemoryWallet::from(&wallet);
        assert!(proto.output_locks.is_empty());
        let legacy_bytes = prost::Message::encode_to_vec(&proto);
        let decoded =
            MemoryWalletDb::decode_new(legacy_bytes.as_slice(), Network::TestNetwork, 100).unwrap();
        assert!(decoded.output_locks.is_empty());
        for (pool, owner) in [(99, vec![0; 32]), (0, vec![0; 31])] {
            let mut malformed = proto.clone();
            malformed
                .output_locks
                .push(crate::proto::memwallet::OutputLockRecord {
                    tx_id: first.txid().as_ref().to_vec(),
                    pool,
                    output_index: 0,
                    owner,
                    expiry_height: 10,
                });
            assert!(MemoryWalletDb::new_from_proto(malformed, Network::TestNetwork, 100).is_err());
        }
    }
}
