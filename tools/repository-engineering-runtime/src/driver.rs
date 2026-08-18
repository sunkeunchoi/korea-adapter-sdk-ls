use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use tokio::sync::watch;
use tokio::task::JoinSet;

use crate::machine::SweepMachine;
use crate::model::{
    AcceptedResultCapsule, ArtifactReference, CheckpointGeneration, DispatchIntent, Phase,
    PublishedHead, RunRequest, TerminalRecord,
};
use crate::ports::{ArtifactStore, CheckpointStore};
use crate::worker_host::{HostRecovery, WorkerHost};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    Machine,
    Artifact,
    Checkpoint,
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

pub struct Driver<H, A, C> {
    host: H,
    artifacts: A,
    checkpoints: C,
}

impl<H, A, C> Driver<H, A, C>
where
    H: WorkerHost,
    A: ArtifactStore,
    C: CheckpointStore,
{
    pub fn new(host: H, artifacts: A, checkpoints: C) -> Self {
        Self {
            host,
            artifacts,
            checkpoints,
        }
    }

    pub async fn run(
        &mut self,
        request: RunRequest,
        cancellation: watch::Receiver<bool>,
    ) -> Result<RunResult, DriverError> {
        let mut machine = SweepMachine::new(request).map_err(|_| DriverError::Machine)?;
        let capsules = BTreeMap::new();
        let mut sequence = 0;
        let mut head = self.create_checkpoint(&machine, sequence, None, None, &capsules)?;

        machine.begin_dispatch().map_err(|_| DriverError::Machine)?;
        sequence += 1;
        head = self.publish_checkpoint(&machine, sequence, &head, None, &capsules)?;

        self.drive(machine, capsules, sequence, head, cancellation, Vec::new())
            .await
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
        let mut machine = SweepMachine::restore(request, &recovered.generation, &loaded_capsules)
            .map_err(|_| DriverError::RecoveryRequired)?;
        let mut capsules = recovered
            .generation
            .rows
            .iter()
            .filter_map(|row| {
                row.result_capsule
                    .clone()
                    .map(|reference| (row.row_id.clone(), reference))
            })
            .collect::<BTreeMap<_, _>>();
        let mut sequence = recovered.generation.sequence;
        let mut head = recovered.head;

        if let Some(terminal) = machine.current_terminal_record() {
            return Ok(RunResult { terminal, head });
        }
        if machine.phase() == Phase::RecoveryRequired {
            return Err(DriverError::RecoveryRequired);
        }
        if machine.phase() == Phase::Discovering {
            machine.begin_dispatch().map_err(|_| DriverError::Machine)?;
            sequence += 1;
            head = self.publish_checkpoint(&machine, sequence, &head, None, &capsules)?;
        }
        if machine.phase() == Phase::Cancelling {
            for intent in machine.running_intents() {
                self.host
                    .cancel_and_reap(intent.invocation_id)
                    .await
                    .map_err(|_| DriverError::Host)?;
            }
            let terminal = machine.finish_cancel().map_err(|_| DriverError::Machine)?;
            sequence += 1;
            head = self.publish_checkpoint(
                &machine,
                sequence,
                &head,
                recovered.generation.cancellation_fence,
                &capsules,
            )?;
            return Ok(RunResult { terminal, head });
        }

        let mut initial = Vec::new();
        for intent in machine.running_intents() {
            match self
                .host
                .recover(intent.clone())
                .await
                .map_err(|_| DriverError::Host)?
            {
                HostRecovery::NeverStarted => initial.push((intent, false)),
                HostRecovery::Running => initial.push((intent, true)),
                HostRecovery::Terminal(capsule) => {
                    head = self.ingest_capsule(
                        &mut machine,
                        &mut capsules,
                        &mut sequence,
                        &head,
                        *capsule,
                    )?;
                }
                HostRecovery::Unknown => {
                    machine.require_recovery();
                    sequence += 1;
                    let _ = self.publish_checkpoint(&machine, sequence, &head, None, &capsules)?;
                    return Err(DriverError::RecoveryRequired);
                }
            }
        }
        self.drive(machine, capsules, sequence, head, cancellation, initial)
            .await
    }

    async fn drive(
        &mut self,
        mut machine: SweepMachine,
        mut capsules: BTreeMap<String, ArtifactReference>,
        mut sequence: u64,
        mut head: PublishedHead,
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
                return self
                    .cancel(
                        &mut machine,
                        &mut sequence,
                        head,
                        &capsules,
                        &mut tasks,
                        &mut in_flight,
                    )
                    .await;
            }

            if machine.phase() == Phase::Dispatching {
                let intents = machine
                    .request_dispatches()
                    .map_err(|_| DriverError::Machine)?;
                if !intents.is_empty() {
                    sequence += 1;
                    head = self.publish_checkpoint(&machine, sequence, &head, None, &capsules)?;
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

            if machine.phase() == Phase::RollingUp && tasks.is_empty() {
                machine
                    .finish_roll_up(&machine.request().base_ledger_digest.clone())
                    .map_err(|_| DriverError::Machine)?;
                sequence += 1;
                head = self.publish_checkpoint(&machine, sequence, &head, None, &capsules)?;
                let terminal = machine.complete().map_err(|_| DriverError::Machine)?;
                sequence += 1;
                head = self.publish_checkpoint(&machine, sequence, &head, None, &capsules)?;
                return Ok(RunResult { terminal, head });
            }

            if machine.phase() == Phase::GateComputed && tasks.is_empty() {
                let terminal = machine.complete().map_err(|_| DriverError::Machine)?;
                sequence += 1;
                head = self.publish_checkpoint(&machine, sequence, &head, None, &capsules)?;
                return Ok(RunResult { terminal, head });
            }

            if tasks.is_empty() {
                return Err(DriverError::RecoveryRequired);
            }

            tokio::select! {
                changed = cancellation.changed(), if cancellation_open => {
                    match changed {
                        Ok(()) if *cancellation.borrow() => {
                            return self.cancel(
                                &mut machine,
                                &mut sequence,
                                head,
                                &capsules,
                                &mut tasks,
                                &mut in_flight,
                            ).await;
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
                            self.fail_and_drain(
                                &mut machine,
                                &mut sequence,
                                head,
                                &capsules,
                                &mut tasks,
                                &mut in_flight,
                            ).await?;
                            return Err(DriverError::Join);
                        }
                    };
                    in_flight.remove(&invocation_id);
                    let capsule = match output {
                        Ok(capsule) => capsule,
                        Err(_) => {
                            self.fail_and_drain(
                                &mut machine,
                                &mut sequence,
                                head,
                                &capsules,
                                &mut tasks,
                                &mut in_flight,
                            ).await?;
                            return Err(DriverError::Host);
                        }
                    };
                    head = self.ingest_capsule(
                        &mut machine,
                        &mut capsules,
                        &mut sequence,
                        &head,
                        capsule,
                    )?;
                }
            }
        }
    }

    async fn cancel(
        &mut self,
        machine: &mut SweepMachine,
        sequence: &mut u64,
        mut head: PublishedHead,
        capsules: &BTreeMap<String, ArtifactReference>,
        tasks: &mut JoinSet<(String, Result<AcceptedResultCapsule, H::Error>)>,
        in_flight: &mut BTreeMap<String, DispatchIntent>,
    ) -> Result<RunResult, DriverError> {
        machine.cancel().map_err(|_| DriverError::Machine)?;
        *sequence += 1;
        let fence = *sequence;
        head = self.publish_checkpoint(machine, *sequence, &head, Some(fence), capsules)?;
        for invocation_id in in_flight.keys().cloned().collect::<Vec<_>>() {
            self.host
                .cancel_and_reap(invocation_id)
                .await
                .map_err(|_| DriverError::Host)?;
        }
        while tasks.join_next().await.is_some() {}
        in_flight.clear();
        let terminal = machine.finish_cancel().map_err(|_| DriverError::Machine)?;
        *sequence += 1;
        head = self.publish_checkpoint(machine, *sequence, &head, Some(fence), capsules)?;
        Ok(RunResult { terminal, head })
    }

    async fn fail_and_drain(
        &mut self,
        machine: &mut SweepMachine,
        sequence: &mut u64,
        mut head: PublishedHead,
        capsules: &BTreeMap<String, ArtifactReference>,
        tasks: &mut JoinSet<(String, Result<AcceptedResultCapsule, H::Error>)>,
        in_flight: &mut BTreeMap<String, DispatchIntent>,
    ) -> Result<(), DriverError> {
        machine.require_recovery();
        *sequence += 1;
        head = self.publish_checkpoint(machine, *sequence, &head, None, capsules)?;
        let _ = head;
        for invocation_id in in_flight.keys().cloned().collect::<Vec<_>>() {
            let _ = self.host.cancel_and_reap(invocation_id).await;
        }
        while tasks.join_next().await.is_some() {}
        in_flight.clear();
        Ok(())
    }

    fn ingest_capsule(
        &mut self,
        machine: &mut SweepMachine,
        capsules: &mut BTreeMap<String, ArtifactReference>,
        sequence: &mut u64,
        head: &PublishedHead,
        capsule: AcceptedResultCapsule,
    ) -> Result<PublishedHead, DriverError> {
        let mut next_machine = machine.clone();
        next_machine
            .accept_capsule(&capsule)
            .map_err(|_| DriverError::RecoveryRequired)?;
        let assignment_id = capsule.result.common().2.to_owned();
        let reference = self.persist_capsule(&assignment_id, &capsule)?;
        capsules.insert(assignment_id, reference);
        *machine = next_machine;
        *sequence += 1;
        self.publish_checkpoint(machine, *sequence, head, None, capsules)
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
    ) -> Result<PublishedHead, DriverError> {
        let generation = checkpoint_generation(
            machine,
            sequence,
            parent_generation_digest,
            cancellation_fence,
            capsules,
        )?;
        self.checkpoints
            .create(generation)
            .map_err(|_| DriverError::Checkpoint)
    }

    fn publish_checkpoint(
        &mut self,
        machine: &SweepMachine,
        sequence: u64,
        prior: &PublishedHead,
        cancellation_fence: Option<u64>,
        capsules: &BTreeMap<String, ArtifactReference>,
    ) -> Result<PublishedHead, DriverError> {
        let generation = checkpoint_generation(
            machine,
            sequence,
            Some(prior.generation_digest.clone()),
            cancellation_fence,
            capsules,
        )?;
        self.checkpoints
            .publish(&prior.generation_digest, generation)
            .map_err(|_| DriverError::Checkpoint)
    }
}

fn checkpoint_generation(
    machine: &SweepMachine,
    sequence: u64,
    parent_generation_digest: Option<String>,
    cancellation_fence: Option<u64>,
    capsules: &BTreeMap<String, ArtifactReference>,
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
        executor_digest: request.executor_digest.clone(),
        scenario_digest: request.scenario_digest.clone(),
        repository_snapshot_digest: request.repository_snapshot_digest.clone(),
        row_manifest_digest: request.row_manifest_digest.clone(),
        base_ledger_digest: request.base_ledger_digest.clone(),
        rows: machine
            .checkpoint_rows(capsules)
            .map_err(|_| DriverError::Machine)?,
        cancellation_fence,
        prepared_effects: Vec::new(),
        applied_effect_ids: Vec::new(),
    })
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
