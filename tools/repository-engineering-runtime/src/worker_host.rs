use std::future::Future;

use crate::model::{AcceptedResultCapsule, DispatchIntent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRecovery {
    NeverStarted,
    Running,
    Terminal(Box<AcceptedResultCapsule>),
    Unknown,
}

pub trait WorkerHost: Clone + Send + Sync + 'static {
    type Error: Send + 'static;

    fn invoke(
        &self,
        intent: DispatchIntent,
    ) -> impl Future<Output = Result<AcceptedResultCapsule, Self::Error>> + Send;
    fn recover(
        &self,
        intent: DispatchIntent,
    ) -> impl Future<Output = Result<HostRecovery, Self::Error>> + Send;
    fn await_terminal(
        &self,
        intent: DispatchIntent,
    ) -> impl Future<Output = Result<AcceptedResultCapsule, Self::Error>> + Send;
    fn cancel_and_reap(
        &self,
        invocation_id: String,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
