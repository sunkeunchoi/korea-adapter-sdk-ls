use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use tokio::sync::watch;
use tokio::task::JoinSet;

use crate::machine::SweepMachine;
use crate::model::{
    AcceptedResultCapsule, ArtifactReference, CheckpointGeneration, DispatchIntent,
    EffectApplyOutcome, EffectEntry, Phase, PublishedHead, RunRequest, TerminalRecord,
};
use crate::ports::{ArtifactStore, CheckpointStore, EffectApplier};
use crate::worker_host::{HostRecovery, WorkerHost};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    Machine,
    Artifact,
    Checkpoint,
    Effect,
    Host,
    Join,
    RecoveryRequired,
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DriverError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub terminal: TerminalRecord,
    pub head: PublishedHead,
}

#[derive(Debug, Default)]
struct EffectProgress {
    prepared: Vec<EffectEntry>,
    applied_ids: Vec<String>,
}

#[derive(Debug)]
struct RunState {
    machine: SweepMachine,
    capsules: BTreeMap<String, ArtifactReference>,
    sequence: u64,
    head: PublishedHead,
    effects: EffectProgress,
}

pub struct Driver<H, A, C, E> {
    host: H,
    artifacts: A,
    checkpoints: C,
    effects: E,
}

impl<H, A, C, E> Driver<H, A, C, E>
where
    H: WorkerHost,
    A: ArtifactStore,
    C: CheckpointStore,
    E: EffectApplier,
{
    pub fn new(host: H, artifacts: A, checkpoints: C, effects: E) -> Self {
        Self {
            host,
            artifacts,
            checkpoints,
            effects,
        }
    }

    pub async fn run(
        &mut self,
        request: RunRequest,
        cancellation: watch::Receiver<bool>,
    ) -> Result<RunResult, DriverError> {
        let mut machine = SweepMachine::new(request).map_err(|_| DriverError::Machine)?;
        let capsules = BTreeMap::new();
        let sequence = 0;
        let effects = EffectProgress::default();
        let head = self.create_checkpoint(&machine, sequence, None, None, &capsules, &effects)?;

        machine.begin_dispatch().map_err(|_| DriverError::Machine)?;
        let mut state = RunState {
            machine,
            capsules,
            sequence,
            head,
            effects,
        };
        state.sequence += 1;
        self.publish_state(&mut state, None)?;

        self.drive(state, cancellation, Vec::new()).await
    }

    pub async fn resume(
        &mut self,
        request: RunRequest,
        caller_pin: &str,
        cancellation: watch::Receiver<bool>,
    ) -> Result<RunResult, DriverError> {
        let recovered = self
            .checkpoints
            .recover(caller_pin)
            .map_err(|_| DriverError::Checkpoint)?;
        let loaded_capsules = self.load_capsules(&recovered.generation)?;
        let machine = SweepMachine::restore(request, &recovered.generation, &loaded_capsules)
            .map_err(|_| DriverError::RecoveryRequired)?;
        let capsules = recovered
            .generation
            .rows
            .iter()
            .filter_map(|row| {
                row.result_capsule
                    .clone()
                    .map(|reference| (row.row_id.clone(), reference))
            })
            .collect::<BTreeMap<_, _>>();
        let mut state = RunState {
            machine,
            capsules,
            sequence: recovered.generation.sequence,
            head: recovered.head,
            effects: EffectProgress {
                prepared: recovered.generation.prepared_effects.clone(),
                applied_ids: recovered.generation.applied_effect_ids.clone(),
            },
        };

        self.reconcile_effects(&mut state, recovered.generation.cancellation_fence)?;

        if let Some(terminal) = state.machine.current_terminal_record() {
            return Ok(RunResult {
                terminal,
                head: state.head,
            });
        }
        if state.machine.phase() == Phase::RecoveryRequired {
            return Err(DriverError::RecoveryRequired);
        }
        if state.machine.phase() == Phase::Discovering {
            state
                .machine
                .begin_dispatch()
                .map_err(|_| DriverError::Machine)?;
            state.sequence += 1;
            self.publish_state(&mut state, None)?;
        }
        if state.machine.phase() == Phase::Cancelling {
            let mut host_failed = false;
            for intent in state.machine.running_intents() {
                if self
                    .host
                    .cancel_and_reap(intent.invocation_id)
                    .await
                    .is_err()
                {
                    host_failed = true;
                }
            }
            if host_failed {
                return Err(DriverError::Host);
            }
            let terminal = state
                .machine
                .finish_cancel()
                .map_err(|_| DriverError::Machine)?;
            state.sequence += 1;
            self.publish_state(&mut state, recovered.generation.cancellation_fence)?;
            return Ok(RunResult {
                terminal,
                head: state.head,
            });
        }

        let mut initial = Vec::new();
        for intent in state.machine.running_intents() {
            match self
                .host
                .recover(intent.clone())
                .await
                .map_err(|_| DriverError::Host)?
            {
                HostRecovery::NeverStarted => initial.push((intent, false)),
                HostRecovery::Running => initial.push((intent, true)),
                HostRecovery::Terminal(capsule) => {
                    self.ingest_capsule(&mut state, *capsule)?;
                }
                HostRecovery::Unknown => {
                    state.machine.require_recovery();
                    state.sequence += 1;
                    self.publish_state(&mut state, None)?;
                    return Err(DriverError::RecoveryRequired);
                }
            }
        }
        self.drive(state, cancellation, initial).await
    }

    async fn drive(
        &mut self,
        mut state: RunState,
        mut cancellation: watch::Receiver<bool>,
        initial: Vec<(DispatchIntent, bool)>,
    ) -> Result<RunResult, DriverError> {
        let mut tasks = JoinSet::new();
        let mut in_flight = BTreeMap::<String, DispatchIntent>::new();
        let mut cancellation_open = true;
        for (intent, await_existing) in initial {
            let host = self.host.clone();
            in_flight.insert(intent.invocation_id.clone(), intent.clone());
            tasks.spawn(async move {
                let invocation_id = intent.invocation_id.clone();
                let result = if await_existing {
                    host.await_terminal(intent).await
                } else {
                    host.invoke(intent).await
                };
                (invocation_id, result)
            });
        }
        loop {
            if *cancellation.borrow() {
                return self.cancel(&mut state, &mut tasks, &mut in_flight).await;
            }

            if state.machine.phase() == Phase::Dispatching {
                let intents = state
                    .machine
                    .request_dispatches()
                    .map_err(|_| DriverError::Machine)?;
                if !intents.is_empty() {
                    state.sequence += 1;
                    match self.publish_state(&mut state, None) {
                        Ok(()) => {}
                        Err(error) => {
                            let _ = self
                                .fail_and_drain(&mut state, &mut tasks, &mut in_flight)
                                .await;
                            return Err(error);
                        }
                    }
                    for intent in intents {
                        let host = self.host.clone();
                        in_flight.insert(intent.invocation_id.clone(), intent.clone());
                        tasks.spawn(async move {
                            let invocation_id = intent.invocation_id.clone();
                            (invocation_id, host.invoke(intent).await)
                        });
                    }
                }
            }

            if state.machine.phase() == Phase::RollingUp && tasks.is_empty() {
                if state.effects.prepared.is_empty() {
                    let base_matches = self
                        .effects
                        .observed_base_ledger_digest()
                        .is_ok_and(|digest| digest == state.machine.request().base_ledger_digest);
                    if !base_matches {
                        self.persist_recovery(&mut state, None)?;
                        return Err(DriverError::RecoveryRequired);
                    }
                    state.effects.prepared = state
                        .machine
                        .prepare_roll_up_effects()
                        .map_err(|_| DriverError::Machine)?;
                    if self.effects.validate_plan(&state.effects.prepared).is_err() {
                        self.persist_recovery(&mut state, None)?;
                        return Err(DriverError::RecoveryRequired);
                    }
                    state.sequence += 1;
                    self.publish_state(&mut state, None)?;
                }
                self.reconcile_effects(&mut state, None)?;
                let observed = match self.effects.observed_base_ledger_digest() {
                    Ok(observed) => observed,
                    Err(_) => {
                        self.persist_recovery(&mut state, None)?;
                        return Err(DriverError::RecoveryRequired);
                    }
                };
                if state.machine.finish_roll_up(&observed).is_err() {
                    self.persist_recovery(&mut state, None)?;
                    return Err(DriverError::RecoveryRequired);
                }
                state.sequence += 1;
                self.publish_state(&mut state, None)?;
                let terminal = state.machine.complete().map_err(|_| DriverError::Machine)?;
                state.sequence += 1;
                self.publish_state(&mut state, None)?;
                return Ok(RunResult {
                    terminal,
                    head: state.head,
                });
            }

            if state.machine.phase() == Phase::GateComputed && tasks.is_empty() {
                let terminal = state.machine.complete().map_err(|_| DriverError::Machine)?;
                state.sequence += 1;
                self.publish_state(&mut state, None)?;
                return Ok(RunResult {
                    terminal,
                    head: state.head,
                });
            }

            if tasks.is_empty() {
                return Err(DriverError::RecoveryRequired);
            }

            tokio::select! {
                changed = cancellation.changed(), if cancellation_open => {
                    match changed {
                        Ok(()) if *cancellation.borrow() => {
                            return self.cancel(&mut state, &mut tasks, &mut in_flight).await;
                        }
                        Ok(()) => {}
                        Err(_) => cancellation_open = false,
                    }
                }
                joined = tasks.join_next() => {
                    let Some(joined) = joined else {
                        return Err(DriverError::Join);
                    };
                    let (invocation_id, output) = match joined {
                        Ok(output) => output,
                        Err(_) => {
                            self.fail_and_drain(&mut state, &mut tasks, &mut in_flight).await?;
                            return Err(DriverError::Join);
                        }
                    };
                    in_flight.remove(&invocation_id);
                    let capsule = match output {
                        Ok(capsule) => capsule,
                        Err(_) => {
                            self.fail_and_drain(&mut state, &mut tasks, &mut in_flight).await?;
                            return Err(DriverError::Host);
                        }
                    };
                    match self.ingest_capsule(&mut state, capsule) {
                        Ok(()) => {}
                        Err(error) => {
                            let _ = self
                                .fail_and_drain(&mut state, &mut tasks, &mut in_flight)
                                .await;
                            return Err(error);
                        }
                    }
                }
            }
        }
    }

    async fn cancel(
        &mut self,
        state: &mut RunState,
        tasks: &mut JoinSet<(String, Result<AcceptedResultCapsule, H::Error>)>,
        in_flight: &mut BTreeMap<String, DispatchIntent>,
    ) -> Result<RunResult, DriverError> {
        state.machine.cancel().map_err(|_| DriverError::Machine)?;
        state.sequence += 1;
        let fence = state.sequence;
        let checkpoint_failed = self.publish_state(state, Some(fence)).is_err();
        let host_failed = self.cancel_all_and_drain(tasks, in_flight).await;
        if checkpoint_failed {
            return Err(DriverError::Checkpoint);
        }
        if host_failed {
            return Err(DriverError::Host);
        }
        let terminal = state
            .machine
            .finish_cancel()
            .map_err(|_| DriverError::Machine)?;
        state.sequence += 1;
        self.publish_state(state, Some(fence))?;
        Ok(RunResult {
            terminal,
            head: state.head.clone(),
        })
    }

    async fn fail_and_drain(
        &mut self,
        state: &mut RunState,
        tasks: &mut JoinSet<(String, Result<AcceptedResultCapsule, H::Error>)>,
        in_flight: &mut BTreeMap<String, DispatchIntent>,
    ) -> Result<(), DriverError> {
        state.machine.require_recovery();
        state.sequence += 1;
        let checkpoint = self.publish_state(state, None);
        let host_failed = self.cancel_all_and_drain(tasks, in_flight).await;
        if checkpoint.is_err() {
            Err(DriverError::Checkpoint)
        } else if host_failed {
            Err(DriverError::Host)
        } else {
            Ok(())
        }
    }

    async fn cancel_all_and_drain(
        &mut self,
        tasks: &mut JoinSet<(String, Result<AcceptedResultCapsule, H::Error>)>,
        in_flight: &mut BTreeMap<String, DispatchIntent>,
    ) -> bool {
        let mut host_failed = false;
        for invocation_id in in_flight.keys().cloned().collect::<Vec<_>>() {
            if self.host.cancel_and_reap(invocation_id).await.is_err() {
                host_failed = true;
            }
        }
        if host_failed {
            tasks.abort_all();
        }
        while tasks.join_next().await.is_some() {}
        in_flight.clear();
        host_failed
    }

    fn reconcile_effects(
        &mut self,
        state: &mut RunState,
        cancellation_fence: Option<u64>,
    ) -> Result<(), DriverError> {
        if state.effects.prepared.is_empty() {
            return Ok(());
        }
        let plan_valid = self.effects.validate_plan(&state.effects.prepared).is_ok();
        let base_matches = self
            .effects
            .observed_base_ledger_digest()
            .is_ok_and(|digest| digest == state.machine.request().base_ledger_digest);
        if !plan_valid || !base_matches {
            self.persist_recovery(state, cancellation_fence)?;
            return Err(DriverError::RecoveryRequired);
        }

        for (index, entry) in state.effects.prepared.clone().iter().enumerate() {
            let outcome = match self.effects.apply(entry) {
                Ok(outcome) => outcome,
                Err(_) => {
                    self.persist_recovery(state, cancellation_fence)?;
                    return Err(DriverError::RecoveryRequired);
                }
            };
            if index < state.effects.applied_ids.len()
                && outcome != EffectApplyOutcome::AlreadyApplied
            {
                self.persist_recovery(state, cancellation_fence)?;
                return Err(DriverError::RecoveryRequired);
            }
            if index < state.effects.applied_ids.len() {
                if state.effects.applied_ids[index] != entry.effect_id {
                    self.persist_recovery(state, cancellation_fence)?;
                    return Err(DriverError::RecoveryRequired);
                }
                continue;
            }
            state.effects.applied_ids.push(entry.effect_id.clone());
            state.sequence += 1;
            self.publish_state(state, cancellation_fence)?;
        }
        Ok(())
    }

    fn ingest_capsule(
        &mut self,
        state: &mut RunState,
        capsule: AcceptedResultCapsule,
    ) -> Result<(), DriverError> {
        let mut next_machine = state.machine.clone();
        next_machine
            .accept_capsule(&capsule)
            .map_err(|_| DriverError::RecoveryRequired)?;
        let assignment_id = capsule.result.common().2.to_owned();
        let reference = self.persist_capsule(&assignment_id, &capsule)?;
        state.capsules.insert(assignment_id, reference);
        state.machine = next_machine;
        state.sequence += 1;
        self.publish_state(state, None)
    }

    fn load_capsules(
        &self,
        generation: &CheckpointGeneration,
    ) -> Result<BTreeMap<String, AcceptedResultCapsule>, DriverError> {
        let mut capsules = BTreeMap::new();
        for row in &generation.rows {
            let Some(reference) = &row.result_capsule else {
                continue;
            };
            let bytes = self
                .artifacts
                .read(&reference.path)
                .map_err(|_| DriverError::RecoveryRequired)?;
            if digest_bytes(&bytes) != reference.sha256 {
                return Err(DriverError::RecoveryRequired);
            }
            let capsule: AcceptedResultCapsule =
                serde_json::from_slice(&bytes).map_err(|_| DriverError::RecoveryRequired)?;
            if capsules.insert(row.row_id.clone(), capsule).is_some() {
                return Err(DriverError::RecoveryRequired);
            }
        }
        Ok(capsules)
    }

    fn persist_capsule(
        &mut self,
        assignment_id: &str,
        capsule: &AcceptedResultCapsule,
    ) -> Result<ArtifactReference, DriverError> {
        let bytes = serde_json::to_vec(capsule).map_err(|_| DriverError::Artifact)?;
        let digest = digest_bytes(&bytes);
        let path = format!(
            "capsules/{}/{}.json",
            capsule.result.common().0,
            assignment_id
        );
        if self.artifacts.create(&path, &bytes).is_err() {
            let existing = self
                .artifacts
                .read(&path)
                .map_err(|_| DriverError::Artifact)?;
            if existing != bytes {
                return Err(DriverError::RecoveryRequired);
            }
        }
        Ok(ArtifactReference {
            schema_version: "v0".to_owned(),
            path,
            sha256: digest,
            media_type: "application/json".to_owned(),
        })
    }

    fn create_checkpoint(
        &mut self,
        machine: &SweepMachine,
        sequence: u64,
        cancellation_fence: Option<u64>,
        parent_generation_digest: Option<String>,
        capsules: &BTreeMap<String, ArtifactReference>,
        effects: &EffectProgress,
    ) -> Result<PublishedHead, DriverError> {
        let generation = checkpoint_generation(
            machine,
            sequence,
            parent_generation_digest,
            cancellation_fence,
            capsules,
            effects,
        )?;
        self.checkpoints
            .create(generation)
            .map_err(|_| DriverError::Checkpoint)
    }

    fn publish_state(
        &mut self,
        state: &mut RunState,
        cancellation_fence: Option<u64>,
    ) -> Result<(), DriverError> {
        let generation = checkpoint_generation(
            &state.machine,
            state.sequence,
            Some(state.head.generation_digest.clone()),
            cancellation_fence,
            &state.capsules,
            &state.effects,
        )?;
        state.head = self
            .checkpoints
            .publish(&state.head.generation_digest, generation)
            .map_err(|_| DriverError::Checkpoint)?;
        Ok(())
    }

    fn persist_recovery(
        &mut self,
        state: &mut RunState,
        cancellation_fence: Option<u64>,
    ) -> Result<(), DriverError> {
        state.machine.require_recovery();
        state.sequence += 1;
        self.publish_state(state, cancellation_fence)
    }
}

fn checkpoint_generation(
    machine: &SweepMachine,
    sequence: u64,
    parent_generation_digest: Option<String>,
    cancellation_fence: Option<u64>,
    capsules: &BTreeMap<String, ArtifactReference>,
    effects: &EffectProgress,
) -> Result<CheckpointGeneration, DriverError> {
    let request = machine.request();
    Ok(CheckpointGeneration {
        schema_version: "v0".to_owned(),
        attempt_id: request.attempt_id.clone(),
        parent_attempt_id: request.parent_attempt_id.clone(),
        sequence,
        phase: machine.phase(),
        parent_generation_digest,
        package_lock_digest: request.package_lock_digest.clone(),
        implementation_subject_digest: request.implementation_subject_digest.clone(),
        capability_contract_digest: request.capability_contract_digest.clone(),
        worker_role_digest: request.worker_role_digest.clone(),
        executor_digest: request.executor_digest.clone(),
        scenario_digest: request.scenario_digest.clone(),
        repository_snapshot_digest: request.repository_snapshot_digest.clone(),
        row_manifest_digest: request.row_manifest_digest.clone(),
        base_ledger_digest: request.base_ledger_digest.clone(),
        output_root_id: request.output_root_id.clone(),
        rows: machine
            .checkpoint_rows(capsules)
            .map_err(|_| DriverError::Machine)?,
        cancellation_fence,
        prepared_effects: effects.prepared.clone(),
        applied_effect_ids: effects.applied_ids.clone(),
    })
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
