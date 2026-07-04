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
//! line: the adapter's scrub masks any token carrying a 6+ consecutive-digit
//! run or a 20+-character alphanumeric run, which would mangle the
//! envelope/intent UUIDs (and any embedded 6-digit shcode) and make the
//! recorded line unparseable — the same free-text-only discipline
//! [`crate::artifacts::RunWriter::write_data_quality`] applies to its
//! observations. The scrub itself lives beside the envelope
//! ([`crate::agent::envelope::to_scrubbed_jsonl_line`]) and is shared with
//! [`crate::artifacts::RunWriter::write_decisions`] — one discipline for both
//! destinations.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::agent::envelope::{to_scrubbed_jsonl_line, DecisionEnvelope};

/// The cross-run decisions file name under `<data>/decisions/`.
pub const DECISIONS_FILE: &str = "decisions.jsonl";

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
    /// **Scrub boundary:** only the free-text keys are scrubbed — typed
    /// `AgentIntent` string fields (e.g. `client_order_id`) write verbatim.
    /// That is safe for this increment's only producer (the Research policy's
    /// compile-time-constant fields), but before any order-management producer
    /// records intents here, identifiers must become scrub-safe by type, not
    /// by key name (see docs/solutions on type-level secret safety).
    ///
    /// # Errors
    ///
    /// Errors when serialization or the filesystem append fails.
    pub fn append(&self, envelope: &DecisionEnvelope) -> anyhow::Result<PathBuf> {
        let line = to_scrubbed_jsonl_line(envelope)?;
        let path = self.path();
        let mut file = OpenOptions::new().append(true).create(true).open(&path)?;
        // One write_all for record + newline: writeln! issues separate writes
        // for content and terminator, so a crash between them (or a future
        // concurrent appender under O_APPEND) could tear a line — and the
        // all-or-nothing readers would then fail on the whole registry.
        file.write_all(line.as_bytes())?;
        Ok(path)
    }

    /// Read every recorded envelope back, in append order (tests + replay),
    /// through the replay loader — so the cross-run registry gets the same
    /// typed per-line errors and per-line schema-version gate as any other
    /// envelope stream (R7): a torn or hand-mangled line, or a line written
    /// by a newer-schema producer, is a named error, never a silent admit.
    /// An absent registry file reads as empty.
    ///
    /// # Errors
    ///
    /// Errors when the file cannot be read, a line fails to parse, or a line
    /// carries an unsupported schema version.
    pub fn read_all(&self) -> anyhow::Result<Vec<DecisionEnvelope>> {
        let path = self.path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        Ok(crate::agent::replay::read_envelopes(&path)?)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use tempfile::TempDir;

    use super::*;
    use crate::agent::capability::{ActionCapability, CapabilitySet};
    use crate::agent::envelope::from_jsonl;
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

    #[test]
    fn malformed_registry_line_is_a_typed_error_naming_the_line() {
        // The exact state a crash mid-append produces: a partial, newline-less
        // final line. read_all (via the replay loader) names the line instead
        // of silently dropping or admitting it.
        let tmp = TempDir::new().unwrap();
        let recorder = DecisionRecorder::new(tmp.path()).unwrap();
        recorder.append(&research_envelope(1)).unwrap();
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new().append(true).open(recorder.path()).unwrap();
        write!(file, "{{\"schema_version\":1,\"envelope").unwrap();
        drop(file);
        let err = recorder.read_all().unwrap_err();
        assert!(err.to_string().contains("line 2"), "typed per-line error: {err}");
    }

    #[test]
    fn unsupported_schema_version_in_the_registry_is_rejected() {
        // The registry read path shares the replay loader's per-line schema
        // gate (R7): a newer-producer line is a named error, never a silent
        // admit into replay.
        let tmp = TempDir::new().unwrap();
        let recorder = DecisionRecorder::new(tmp.path()).unwrap();
        recorder.append(&research_envelope(1)).unwrap();
        let mut bumped = serde_json::to_value(research_envelope(2)).unwrap();
        bumped["schema_version"] = serde_json::json!(999);
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new().append(true).open(recorder.path()).unwrap();
        writeln!(file, "{bumped}").unwrap();
        drop(file);
        let err = recorder.read_all().unwrap_err();
        assert!(err.to_string().contains("schema version 999"), "{err}");
    }
}
