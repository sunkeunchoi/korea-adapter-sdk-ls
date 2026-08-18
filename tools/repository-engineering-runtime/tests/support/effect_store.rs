use std::collections::BTreeSet;

use repository_engineering_runtime::model::{EffectApplyOutcome, EffectEntry};
use repository_engineering_runtime::ports::EffectApplier;

#[derive(Debug, Clone)]
pub struct MemoryEffects {
    base_ledger_digest: String,
    applied: BTreeSet<String>,
}

impl MemoryEffects {
    pub fn new(base_ledger_digest: String) -> Self {
        Self {
            base_ledger_digest,
            applied: BTreeSet::new(),
        }
    }
}

impl EffectApplier for MemoryEffects {
    type Error = ();

    fn observed_base_ledger_digest(&self) -> Result<String, Self::Error> {
        Ok(self.base_ledger_digest.clone())
    }

    fn validate_plan(&self, entries: &[EffectEntry]) -> Result<(), Self::Error> {
        let unique_ids = entries
            .iter()
            .map(|entry| &entry.effect_id)
            .collect::<BTreeSet<_>>();
        let unique_targets = entries
            .iter()
            .map(|entry| &entry.relative_target)
            .collect::<BTreeSet<_>>();
        if entries.is_empty()
            || unique_ids.len() != entries.len()
            || unique_targets.len() != entries.len()
            || entries
                .iter()
                .any(|entry| entry.base_ledger_digest != self.base_ledger_digest)
        {
            return Err(());
        }
        Ok(())
    }

    fn apply(&mut self, entry: &EffectEntry) -> Result<EffectApplyOutcome, Self::Error> {
        if self.applied.insert(entry.effect_id.clone()) {
            Ok(EffectApplyOutcome::Applied)
        } else {
            Ok(EffectApplyOutcome::AlreadyApplied)
        }
    }
}
