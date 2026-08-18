use crate::model::{DispatchIntent, TerminalRow, WorkerResult};

pub trait WorkerHost {
    type Error;

    fn invoke(&mut self, intent: &DispatchIntent) -> Result<WorkerResult, Self::Error>;
    fn cancel_and_reap(&mut self, invocation_id: &str) -> Result<(), Self::Error>;
}

pub trait ArtifactStore {
    type Error;

    fn create(&mut self, artifact_id: &str, bytes: &[u8]) -> Result<(), Self::Error>;
    fn read(&self, artifact_id: &str) -> Result<Vec<u8>, Self::Error>;
}

pub trait EffectApplier {
    type Error;

    fn prepare(&mut self, rows: &[TerminalRow]) -> Result<Vec<String>, Self::Error>;
    fn apply(&mut self, effect_id: &str) -> Result<(), Self::Error>;
}
