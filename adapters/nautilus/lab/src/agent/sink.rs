//! In-run decision sink (R6) — a thread-safe destination for per-decision
//! [`DecisionEnvelope`]s. Ordinary single-session runs collect in memory; long-running
//! paths can stream scrubbed JSONL into an artifact staging directory instead. Clones
//! share the same destination in either mode.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::agent::envelope::{from_jsonl, to_scrubbed_jsonl_line, DecisionEnvelope};

#[derive(Debug)]
enum SinkState {
    Buffered(Vec<DecisionEnvelope>),
    Streaming(StreamingState),
}

#[derive(Debug)]
struct StreamingState {
    writer: Option<BufWriter<File>>,
    path: PathBuf,
    len: usize,
    error: Option<String>,
}

/// A thread-safe collector for decision envelopes (the in-run counterpart of
/// the cross-run [`crate::agent::recording::DecisionRecorder`]).
#[derive(Debug, Clone)]
pub struct DecisionSink {
    state: Arc<Mutex<SinkState>>,
}

impl Default for DecisionSink {
    fn default() -> Self {
        DecisionSink { state: Arc::new(Mutex::new(SinkState::Buffered(Vec::new()))) }
    }
}

impl DecisionSink {
    /// A fresh, empty sink.
    pub fn new() -> Self {
        DecisionSink::default()
    }

    /// A sink that writes bounded, scrubbed JSONL directly to `path`.
    pub(crate) fn streaming(path: &Path) -> anyhow::Result<Self> {
        let writer = BufWriter::new(File::create(path)?);
        Ok(DecisionSink {
            state: Arc::new(Mutex::new(SinkState::Streaming(StreamingState {
                writer: Some(writer),
                path: path.to_path_buf(),
                len: 0,
                error: None,
            }))),
        })
    }

    /// Record one decision envelope. Streaming failures are retained and surfaced by
    /// [`flush`](Self::flush) or [`finish`](Self::finish), since strategy callbacks cannot
    /// return I/O errors through the nautilus engine.
    pub fn emit(&self, envelope: DecisionEnvelope) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match &mut *state {
            SinkState::Buffered(envelopes) => envelopes.push(envelope),
            SinkState::Streaming(stream) => {
                if stream.error.is_some() {
                    return;
                }
                let line = match to_scrubbed_jsonl_line(&envelope) {
                    Ok(line) => line,
                    Err(error) => {
                        stream.error = Some(format!("serialize decision envelope: {error}"));
                        return;
                    }
                };
                let Some(writer) = stream.writer.as_mut() else {
                    stream.error = Some("decision emitted after the stream was finished".to_string());
                    return;
                };
                if let Err(error) = writer.write_all(line.as_bytes()) {
                    stream.error = Some(format!("append decision envelope: {error}"));
                    return;
                }
                stream.len += 1;
            }
        }
    }

    /// A snapshot copy of all envelopes recorded so far (insertion order). Streaming
    /// callers should avoid this whole-stream diagnostic on window-scale runs.
    pub fn snapshot(&self) -> Vec<DecisionEnvelope> {
        self.flush().expect("decision stream must be readable for a snapshot");
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match &*state {
            SinkState::Buffered(envelopes) => envelopes.clone(),
            SinkState::Streaming(stream) => {
                let text = std::fs::read_to_string(&stream.path)
                    .expect("decision stream must be readable for a snapshot");
                from_jsonl(&text).expect("decision stream must parse for a snapshot")
            }
        }
    }

    /// The number of envelopes recorded.
    pub fn len(&self) -> usize {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match &*state {
            SinkState::Buffered(envelopes) => envelopes.len(),
            SinkState::Streaming(stream) => stream.len,
        }
    }

    /// Whether no envelopes have been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Flush any streamed envelopes so they are visible in the staging file.
    pub fn flush(&self) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let SinkState::Streaming(stream) = &mut *state else { return Ok(()) };
        if let Some(error) = &stream.error {
            anyhow::bail!("decision stream failed: {error}");
        }
        if let Some(writer) = stream.writer.as_mut() {
            writer.flush()?;
        }
        Ok(())
    }

    /// Flush and close a streaming destination before its staging directory is renamed.
    pub fn finish(&self) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let SinkState::Streaming(stream) = &mut *state else { return Ok(()) };
        if let Some(mut writer) = stream.writer.take() {
            if let Err(error) = writer.flush() {
                let flush_error = format!("flush decision stream: {error}");
                match &mut stream.error {
                    Some(prior) => {
                        prior.push_str("; ");
                        prior.push_str(&flush_error);
                    }
                    None => stream.error = Some(flush_error),
                }
            }
        }
        if let Some(error) = &stream.error {
            anyhow::bail!("decision stream failed: {error}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::agent::context::AgentContext;
    use crate::agent::envelope::{Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger};

    fn telemetry(ts_event: u64) -> DecisionEnvelope {
        DecisionEnvelope::telemetry(
            ts_event,
            DecisionTrigger::StateChange { description: "unit test".to_string() },
            DecisionDetail::universe("005930.XKRX", Decision::Accept, None, BTreeMap::new()),
            AgentContext::telemetry("orb", 0, BTreeMap::new(), BTreeMap::new()),
        )
    }

    #[test]
    fn sink_collects_in_insertion_order() {
        let sink = DecisionSink::new();
        assert!(sink.is_empty());
        sink.emit(telemetry(1));
        sink.emit(telemetry(2));
        assert_eq!(sink.len(), 2);
        assert!(!sink.is_empty());
        let snap = sink.snapshot();
        assert_eq!(snap[0].ts_event, 1);
        assert_eq!(snap[1].ts_event, 2);
        // A clone shares the same underlying store (the runner drains the
        // engine thread's emissions through its own clone).
        let clone = sink.clone();
        clone.emit(telemetry(3));
        assert_eq!(sink.len(), 3);
    }

    #[test]
    fn finish_closes_a_failed_stream_and_keeps_reporting_the_error() {
        let dir = tempfile::tempdir().unwrap();
        let sink = DecisionSink::streaming(&dir.path().join("decisions.jsonl")).unwrap();
        {
            let mut state = sink.state.lock().unwrap();
            let SinkState::Streaming(stream) = &mut *state else { unreachable!() };
            stream.error = Some("earlier append failure".to_string());
        }

        let first = sink.finish().unwrap_err();
        assert!(first.to_string().contains("earlier append failure"));
        {
            let state = sink.state.lock().unwrap();
            let SinkState::Streaming(stream) = &*state else { unreachable!() };
            assert!(stream.writer.is_none(), "finish releases the file after an earlier error");
        }
        let retry = sink.finish().unwrap_err();
        assert!(retry.to_string().contains("earlier append failure"));
    }
}
