//! Owned proof inputs are evaluated without borrowing wallet or database state.
use crate::VotingError;
use futures_util::future::LocalBoxFuture;

pub trait ProofExecutor {
    fn prove<T: Send + 'static, R: Send + 'static>(
        &self,
        input: T,
        prove: impl FnOnce(T) -> Result<R, VotingError> + Send + 'static,
    ) -> LocalBoxFuture<'static, Result<R, VotingError>>;
}

pub struct SequentialProofExecutor;
impl ProofExecutor for SequentialProofExecutor {
    fn prove<T: Send + 'static, R: Send + 'static>(
        &self,
        input: T,
        prove: impl FnOnce(T) -> Result<R, VotingError> + Send + 'static,
    ) -> LocalBoxFuture<'static, Result<R, VotingError>> {
        Box::pin(async move { prove(input) })
    }
}
