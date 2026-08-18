use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use repository_engineering_runtime::model::{AcceptedResultCapsule, DispatchIntent};
use repository_engineering_runtime::worker_host::{HostRecovery, WorkerHost};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

const MAX_STDOUT_BYTES: u64 = 256 * 1024;
const MAX_STDERR_BYTES: u64 = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubprocessHostError {
    Boundary,
    Timeout,
    Cancelled,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerObservation {
    pub pid: u32,
    pub cwd_empty: bool,
    pub environment: Vec<String>,
}

#[derive(Debug, Default)]
struct State {
    active: AtomicUsize,
    maximum_active: AtomicUsize,
    sequence: AtomicUsize,
    cancellations: Mutex<BTreeMap<String, watch::Sender<bool>>>,
    observations: Mutex<Vec<WorkerObservation>>,
}

#[derive(Debug, Clone)]
pub struct SubprocessHost {
    executable: PathBuf,
    work_root: PathBuf,
    timeout: Duration,
    mode: String,
    state: Arc<State>,
}

impl SubprocessHost {
    pub fn new(
        executable: impl AsRef<Path>,
        work_root: impl AsRef<Path>,
        timeout: Duration,
    ) -> Result<Self, SubprocessHostError> {
        let executable = executable
            .as_ref()
            .canonicalize()
            .map_err(|_| SubprocessHostError::Boundary)?;
        let work_root = work_root
            .as_ref()
            .canonicalize()
            .map_err(|_| SubprocessHostError::Boundary)?;
        if !executable.is_file() || !work_root.is_dir() {
            return Err(SubprocessHostError::Boundary);
        }
        Ok(Self {
            executable,
            work_root,
            timeout,
            mode: "success".to_owned(),
            state: Arc::new(State::default()),
        })
    }

    pub fn with_mode(mut self, mode: &str) -> Self {
        self.mode = mode.to_owned();
        self
    }

    pub fn active(&self) -> usize {
        self.state.active.load(Ordering::SeqCst)
    }

    pub fn maximum_active(&self) -> usize {
        self.state.maximum_active.load(Ordering::SeqCst)
    }

    pub fn observations(&self) -> Vec<WorkerObservation> {
        self.state
            .observations
            .lock()
            .expect("observations lock")
            .clone()
    }
}

struct ActiveGuard(Arc<State>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

impl WorkerHost for SubprocessHost {
    type Error = SubprocessHostError;

    async fn invoke(&self, intent: DispatchIntent) -> Result<AcceptedResultCapsule, Self::Error> {
        let active = self.state.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.state
            .maximum_active
            .fetch_max(active, Ordering::SeqCst);
        let _guard = ActiveGuard(self.state.clone());
        let sequence = self.state.sequence.fetch_add(1, Ordering::SeqCst);
        let working_directory = self.work_root.join(format!("worker-{sequence:08}"));
        std::fs::create_dir(&working_directory).map_err(|_| SubprocessHostError::Boundary)?;

        let (cancel, mut cancelled) = watch::channel(false);
        self.state
            .cancellations
            .lock()
            .map_err(|_| SubprocessHostError::Boundary)?
            .insert(intent.invocation_id.clone(), cancel);

        let intent_json =
            serde_json::to_string(&intent).map_err(|_| SubprocessHostError::Protocol)?;
        let mut child = Command::new(&self.executable)
            .arg(intent_json)
            .arg(&self.mode)
            .env_clear()
            .env("FIXTURE_ALLOWED", "1")
            .current_dir(&working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| SubprocessHostError::Boundary)?;
        let stdout = child.stdout.take().ok_or(SubprocessHostError::Boundary)?;
        let stderr = child.stderr.take().ok_or(SubprocessHostError::Boundary)?;
        let stdout_task = tokio::spawn(read_bounded(stdout, MAX_STDOUT_BYTES));
        let stderr_task = tokio::spawn(read_bounded(stderr, MAX_STDERR_BYTES));

        let status = tokio::select! {
            status = child.wait() => status.map_err(|_| SubprocessHostError::Boundary),
            changed = cancelled.changed() => {
                let _ = changed;
                let _ = child.kill().await;
                Err(SubprocessHostError::Cancelled)
            }
            () = tokio::time::sleep(self.timeout) => {
                let _ = child.kill().await;
                Err(SubprocessHostError::Timeout)
            }
        };
        self.state
            .cancellations
            .lock()
            .map_err(|_| SubprocessHostError::Boundary)?
            .remove(&intent.invocation_id);
        let stdout = stdout_task
            .await
            .map_err(|_| SubprocessHostError::Boundary)??;
        let stderr = stderr_task
            .await
            .map_err(|_| SubprocessHostError::Boundary)??;
        let status = status?;
        if !status.success() || !stderr.is_empty() {
            return Err(SubprocessHostError::Protocol);
        }
        let capsule: AcceptedResultCapsule =
            serde_json::from_slice(&stdout).map_err(|_| SubprocessHostError::Protocol)?;
        let observation: WorkerObservation =
            serde_json::from_slice(&capsule.worker_instance_receipt_bytes)
                .map_err(|_| SubprocessHostError::Protocol)?;
        self.state
            .observations
            .lock()
            .map_err(|_| SubprocessHostError::Boundary)?
            .push(observation);
        Ok(capsule)
    }

    async fn recover(&self, _intent: DispatchIntent) -> Result<HostRecovery, Self::Error> {
        Ok(HostRecovery::NeverStarted)
    }

    async fn await_terminal(
        &self,
        _intent: DispatchIntent,
    ) -> Result<AcceptedResultCapsule, Self::Error> {
        Err(SubprocessHostError::Boundary)
    }

    async fn cancel_and_reap(&self, invocation_id: String) -> Result<(), Self::Error> {
        if let Some(sender) = self
            .state
            .cancellations
            .lock()
            .map_err(|_| SubprocessHostError::Boundary)?
            .get(&invocation_id)
            .cloned()
        {
            let _ = sender.send(true);
        }
        for _ in 0..5000 {
            if !self
                .state
                .cancellations
                .lock()
                .map_err(|_| SubprocessHostError::Boundary)?
                .contains_key(&invocation_id)
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        Err(SubprocessHostError::Timeout)
    }
}

async fn read_bounded<R>(reader: R, limit: u64) -> Result<Vec<u8>, SubprocessHostError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    BufReader::new(reader)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| SubprocessHostError::Boundary)?;
    if bytes.len() as u64 > limit {
        return Err(SubprocessHostError::Protocol);
    }
    Ok(bytes)
}

pub fn unique_pids(observations: &[WorkerObservation]) -> usize {
    observations
        .iter()
        .map(|observation| observation.pid)
        .collect::<BTreeSet<_>>()
        .len()
}
