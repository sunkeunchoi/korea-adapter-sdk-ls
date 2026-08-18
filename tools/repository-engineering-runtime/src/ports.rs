use crate::model::{
    CheckpointGeneration, EffectApplyOutcome, EffectEntry, PublishedHead, RecoveredCheckpoint,
};

pub trait ArtifactStore {
    type Error;

    fn create(&mut self, artifact_id: &str, bytes: &[u8]) -> Result<(), Self::Error>;
    fn read(&self, artifact_id: &str) -> Result<Vec<u8>, Self::Error>;
}

pub trait CheckpointStore {
    type Error;

    fn create(&mut self, generation: CheckpointGeneration) -> Result<PublishedHead, Self::Error>;
    fn publish(
        &mut self,
        observed_generation_digest: &str,
        generation: CheckpointGeneration,
    ) -> Result<PublishedHead, Self::Error>;
    fn recover(&mut self, caller_pin: &str) -> Result<RecoveredCheckpoint, Self::Error>;
}

pub trait EffectApplier {
    type Error;

    fn observed_base_ledger_digest(&self) -> Result<String, Self::Error>;
    fn validate_plan(&self, entries: &[EffectEntry]) -> Result<(), Self::Error>;
    fn apply(&mut self, entry: &EffectEntry) -> Result<EffectApplyOutcome, Self::Error>;
}
