//! Cross-run decisions registry (KTD5, R9 partial) — the append-only JSONL
//! home for intent-bearing Research envelopes at `<data>/decisions/decisions.jsonl`.
//!
//! Distinct from the immutable per-run registry (KTD5): a finalized run
//! directory under `<data>/runs/<run_id>/` is never appended to, so cross-run
//! decisions (a Research proposal reads run N and shapes run N+1) land here,
//! never in a finalized run dir. The file is append-only — one compact JSON
//! envelope per line, scrubbed of account/secret-like tokens at write time
//! via [`crate::artifacts::scrub`] (R9), mirroring the artifact writer's
//! scrub-at-write discipline.
//!
//! The scrub targets the envelope's **free-text fields** (`reason`,
//! `rationale`, `description`, `message`) rather than the whole serialized
//! line: the adapter's scrub masks every 20+ alphanumeric token, which would
//! mangle the envelope/intent UUIDs and make the recorded line unparseable —
//! the same free-text-only discipline
//! [`crate::artifacts::RunWriter::write_data_quality`] applies to its
//! observations.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::agent::envelope::{from_jsonl, DecisionEnvelope};
use crate::artifacts::scrub;

/// The cross-run decisions file name under `<data>/decisions/`.
pub const DECISIONS_FILE: &str = "decisions.jsonl";

/// The envelope's free-text JSON keys, scrubbed at write time (R9). Everything
/// else in an envelope is typed (ids, numbers, tags) and stays intact so the
/// line parses back via [`from_jsonl`].
const FREE_TEXT_KEYS: [&str; 4] = ["reason", "rationale", "description", "message"];

/// Scrub the free-text string values (by [`FREE_TEXT_KEYS`]) anywhere in the
/// serialized envelope tree, delegating to [`crate::artifacts::scrub`]. Shared
/// with [`crate::artifacts::RunWriter::write_decisions`] so the in-run stream
/// and the cross-run registry apply one scrub discipline (R9).
pub(crate) fn scrub_free_text(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                match val {
                    serde_json::Value::String(s) if FREE_TEXT_KEYS.contains(&key.as_str()) => {
                        *s = scrub(s);
                    }
                    _ => scrub_free_text(val),
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                scrub_free_text(item);
            }
        }
        _ => {}
    }
}

/// The append-only cross-run decisions recorder (KTD5). Owns
/// `<data>/decisions/` and appends one scrubbed envelope line per decision.
#[derive(Clone, Debug)]
pub struct DecisionRecorder {
    dir: PathBuf,
}

impl DecisionRecorder {
    /// Open the registry at `<data_home>/decisions/`, creating the directory
    /// if needed.
    ///
    /// # Errors
    ///
    /// Errors when the directory cannot be created.
    pub fn new(data_home: &Path) -> anyhow::Result<Self> {
        let dir = data_home.join("decisions");
        std::fs::create_dir_all(&dir)?;
        Ok(DecisionRecorder { dir })
    }

    /// The registry file's path (`<data>/decisions/decisions.jsonl`).
    pub fn path(&self) -> PathBuf {
        self.dir.join(DECISIONS_FILE)
    }

    /// Append one envelope as a compact single-line JSON record. Free-text
    /// fields are scrubbed via [`crate::artifacts::scrub`] **before** the line
    /// hits disk (R9 — write-time scrub; see the module docs for why the scrub
    /// is free-text-targeted), then appended (create-if-missing, never
    /// truncate). Returns the registry file's path.
    ///
    /// # Errors
    ///
    /// Errors when serialization or the filesystem append fails.
    pub fn append(&self, envelope: &DecisionEnvelope) -> anyhow::Result<PathBuf> {
        let mut value = serde_json::to_value(envelope)?;
        scrub_free_text(&mut value);
        let line = serde_json::to_string(&value)?;
        let path = self.path();
        let mut file = OpenOptions::new().append(true).create(true).open(&path)?;
        writeln!(file, "{line}")?;
        Ok(path)
    }

    /// Read every recorded envelope back, in append order (tests + replay).
    /// An absent registry file reads as empty.
    ///
    /// # Errors
    ///
    /// Errors when the file cannot be read or a line fails to parse.
    pub fn read_all(&self) -> anyhow::Result<Vec<DecisionEnvelope>> {
        let path = self.path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&path)?;
        Ok(from_jsonl(&text)?)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use tempfile::TempDir;

    use super::*;
    use crate::agent::capability::{ActionCapability, CapabilitySet};
    use crate::agent::context::AgentContext;
    use crate::agent::envelope::{CapabilityOutcome, DecisionTrigger, PolicyDecisionRecord};
    use crate::agent::guardrails::proposal_bounds::ProposalBoundsGuardrail;
    use crate::agent::pipeline::DecisionPipeline;
    use crate::agent::policies::research::{
        ResearchPolicy, KEY_NUM_TRADES, PARAM_GAP_MIN_PCT,
    };
    use crate::agent::policy::AgentPolicy;

    fn fixture_context() -> AgentContext {
        AgentContext::run_state(
            1_000_000.0,
            Vec::new(),
            BTreeMap::from([(PARAM_GAP_MIN_PCT.to_string(), 3.0)]),
            BTreeMap::from([(KEY_NUM_TRADES.to_string(), 2.0)]),
        )
    }

    fn pipeline() -> DecisionPipeline {
        DecisionPipeline::new(
            CapabilitySet {
                observations: BTreeSet::new(),
                actions: BTreeSet::from([ActionCapability::Research]),
                instrument_scope: BTreeSet::new(),
            },
            vec![Box::new(ProposalBoundsGuardrail { max_relative_change: 0.5 })],
        )
    }

    fn research_envelope(ts_event: u64) -> crate::agent::envelope::DecisionEnvelope {
        let policy = ResearchPolicy::default();
        let context = fixture_context();
        let decision = policy.evaluate(&context).unwrap();
        pipeline().run(
            ts_event,
            DecisionTrigger::Manual { reason: "unit test".to_string() },
            context,
            decision,
        )
    }

    #[test]
    fn registry_is_append_only_jsonl_and_scrubbed() {
        let tmp = TempDir::new().unwrap();
        let recorder = DecisionRecorder::new(tmp.path()).unwrap();
        let path = recorder.append(&research_envelope(1)).unwrap();
        recorder.append(&research_envelope(2)).unwrap();

        assert_eq!(path, tmp.path().join("decisions").join(DECISIONS_FILE));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2, "two appends -> two lines");
        let back = from_jsonl(&text).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].ts_event, 1, "append order preserved");
        assert_eq!(back[1].ts_event, 2);
        // R9: no credential-like tokens on disk.
        assert!(!text.contains("account"), "scrubbed registry: {text}");

        let read = recorder.read_all().unwrap();
        assert_eq!(read.len(), 2);
    }

    #[test]
    fn account_like_token_in_free_text_is_masked_but_the_line_still_parses() {
        // R9 write-time scrub engages on real credential-like content while
        // the envelope's UUIDs survive, keeping the line parseable.
        let tmp = TempDir::new().unwrap();
        let recorder = DecisionRecorder::new(tmp.path()).unwrap();
        let mut envelope = research_envelope(3);
        if let PolicyDecisionRecord::Execute {
            intent: crate::agent::intent::AgentIntent::ProposeParameterChange { rationale, .. },
            ..
        } = &mut envelope.policy_decision
        {
            *rationale = "acct 20187511401 underperformed".to_string();
        }
        let path = recorder.append(&envelope).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("20187511401"), "account token masked: {text}");
        assert!(text.contains("***"), "masking marker present: {text}");
        let back = recorder.read_all().unwrap();
        assert_eq!(back.len(), 1, "scrubbed line still parses");
    }

    #[test]
    fn absent_registry_reads_as_empty() {
        let tmp = TempDir::new().unwrap();
        let recorder = DecisionRecorder::new(tmp.path()).unwrap();
        assert!(recorder.read_all().unwrap().is_empty());
    }

    #[test]
    fn recorded_context_alone_reconstructs_the_policy_decision() {
        // The policy-replay deferral premise: the envelope's captured context
        // is policy-sufficient for the shipped policy class — re-evaluating
        // the policy on the RECORDED context reproduces the recorded intent.
        let tmp = TempDir::new().unwrap();
        let recorder = DecisionRecorder::new(tmp.path()).unwrap();
        recorder.append(&research_envelope(42)).unwrap();

        let recorded = &recorder.read_all().unwrap()[0];
        let PolicyDecisionRecord::Execute { intent: recorded_intent, .. } =
            &recorded.policy_decision
        else {
            panic!("expected an Execute record, got {:?}", recorded.policy_decision);
        };
        assert_eq!(recorded.capability, CapabilityOutcome::Granted);

        let policy = ResearchPolicy::default();
        let replayed = policy.evaluate(&recorded.context).unwrap();
        let crate::agent::policy::PolicyDecision::Execute(planned) = replayed else {
            panic!("expected Execute on replay, got {replayed:?}");
        };
        assert_eq!(
            &planned.intent, recorded_intent,
            "captured context is policy-sufficient"
        );
    }
}
