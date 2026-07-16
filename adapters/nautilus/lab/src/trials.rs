//! The TRIALS ledger (U3, R10/R12; KTD4) — the append-only home for every
//! statistical *look* the strategy loop takes at a backtest sample.
//!
//! Backtest-overfitting discipline (Bailey & López de Prado) requires an
//! iterated loop to keep the trial count first-class: the more distinct looks a
//! fixed sample sees, the more a KEEP margin must be deflated to stay honest.
//! This ledger accumulates one record per look — each Phase-A gate reading
//! (including STOPs), each flip evaluation, each sweep leg — so
//! "how many trials against this sample, total and per family?" is a direct
//! query. **Record-only (KD2):** the KEEP bar is unchanged; a trial-count-adjusted
//! margin is a separate, pre-registered future decision. Re-baseline reconciles
//! are identity checks, not looks, and are never recorded here.
//!
//! Mechanics mirror the decisions registry ([`crate::agent::recording`], KTD4):
//! `schema_version` per record, `OpenOptions::append(true).create(true)`, one
//! `write_all` for record+newline (torn-line safety), typed per-line read errors,
//! and a free-text scrub at write time. The scrub targets only the genuinely
//! free-text fields (`verdict`, `source`) — the structured hash fields (a 64-hex
//! `catalog_fingerprint`) must render verbatim, since the account-number heuristic
//! would otherwise mask them and destroy lineage grouping.
//!
//! The schema is a subset/seed of the eventual structured turn-record substrate
//! (`turns.jsonl`) named in the source ideation, so a later adoption absorbs this
//! ledger as a superset-merge, not a migration.

use std::collections::{BTreeMap, HashMap};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The current trial-record schema version. A newer-producer line is a typed
/// refusal on read (never a silent admit), mirroring the replay loader's gate.
pub const TRIAL_SCHEMA_VERSION: u32 = 1;

/// The tracked-home ledger file, relative to the lab crate root (KTD2).
pub const LEDGER_RELPATH: &str = "ledger/trials.jsonl";

/// The kind of statistical look a record represents (KTD4). Each is one look the
/// overfitting literature counts; a re-baseline reconcile is an identity check,
/// not a look, and is never one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LookKind {
    /// A Phase-A gate reading (diagnose), including STOPs.
    GateReading,
    /// A flip evaluation (the KEEP-rule look).
    Flip,
    /// One leg of a sweep candidate.
    SweepLeg,
}

impl LookKind {
    fn parse(s: &str) -> anyhow::Result<Self> {
        match s {
            "gate-reading" => Ok(LookKind::GateReading),
            "flip" => Ok(LookKind::Flip),
            "sweep-leg" => Ok(LookKind::SweepLeg),
            other => anyhow::bail!(
                "look kind {other:?} not one of gate-reading | flip | sweep-leg"
            ),
        }
    }
}

/// The sample a trial ran against (KTD4): a fingerprint plus an optional parent
/// link declaring equivalence across catalog evolutions. Per-lineage counting
/// follows parent links so the 166/157/167 catalog eras that are the "same
/// evolving sample" merge into one lineage count, while per-fingerprint counting
/// keeps them distinct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleLineage {
    /// The range-scoped catalog fingerprint the look ran against.
    pub catalog_fingerprint: String,
    /// The prior-era fingerprint this sample descends from, when declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_fingerprint: Option<String>,
}

/// One trial record — one statistical look at the sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrialRecord {
    /// Schema version (gate on read).
    pub schema_version: u32,
    /// RFC3339-like UTC stamp. Supplied by the caller (deterministic in tests;
    /// `Utc::now()` at the CLI seam) so the record is reproducible.
    pub recorded_utc: String,
    /// The candidate slug this look belongs to.
    pub candidate: String,
    /// The lever family (e.g. `class-b`, `entry-filter`).
    pub family: String,
    /// Which look this is.
    pub look: LookKind,
    /// The sample lineage.
    pub lineage: SampleLineage,
    /// The readings taken (empty for a backfill whose readings were not recorded).
    #[serde(default)]
    pub readings: BTreeMap<String, f64>,
    /// The verdict this look reached (a verdict-grammar string or a backfilled
    /// outcome). Free text — scrubbed at write time.
    pub verdict: String,
    /// True for a record authored from the historical record (U8), not produced
    /// live by the command.
    #[serde(default, skip_serializing_if = "is_false")]
    pub backfill: bool,
    /// Where a backfill record's facts came from (a TURN-LOG anchor or archive
    /// path). Free text — scrubbed at write time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl TrialRecord {
    /// A live (non-backfill) trial record at the current schema version.
    pub fn new(
        candidate: impl Into<String>,
        family: impl Into<String>,
        look: LookKind,
        lineage: SampleLineage,
        readings: BTreeMap<String, f64>,
        verdict: impl Into<String>,
        recorded_utc: impl Into<String>,
    ) -> Self {
        TrialRecord {
            schema_version: TRIAL_SCHEMA_VERSION,
            recorded_utc: recorded_utc.into(),
            candidate: candidate.into(),
            family: family.into(),
            look,
            lineage,
            readings,
            verdict: verdict.into(),
            backfill: false,
            source: None,
        }
    }
}

/// The append-only trials ledger at a given path (library-functions-over-config:
/// the CLI resolves the fixed tracked path, tests point it at a tempdir).
#[derive(Clone, Debug)]
pub struct TrialsLedger {
    path: PathBuf,
}

impl TrialsLedger {
    /// A ledger at `path` (created lazily on first append).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        TrialsLedger { path: path.into() }
    }

    /// The ledger file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one record as a compact single-line JSON, scrubbed. Create-if-missing
    /// (with parent dirs), never truncate; one `write_all` for record+newline so a
    /// crash between content and terminator cannot tear a line (mirroring
    /// [`crate::agent::recording::DecisionRecorder::append`]).
    ///
    /// # Errors
    ///
    /// When the parent directory or file cannot be created, or serialization fails.
    pub fn append(&self, record: &TrialRecord) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let line = to_scrubbed_line(record)?;
        let mut file = OpenOptions::new().append(true).create(true).open(&self.path)?;
        file.write_all(line.as_bytes())?;
        Ok(())
    }

    /// Read every record back in append order. An absent file reads empty; a torn
    /// or newer-schema line is a typed per-line error naming the line number.
    ///
    /// # Errors
    ///
    /// When the file cannot be read, a line fails to parse, or a line carries an
    /// unsupported schema version.
    pub fn read_all(&self) -> anyhow::Result<Vec<TrialRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&self.path)
            .map_err(|e| anyhow::anyhow!("reading trials ledger {}: {e}", self.path.display()))?;
        let mut out = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let rec: TrialRecord = serde_json::from_str(line)
                .map_err(|e| anyhow::anyhow!("trials ledger line {}: {e}", i + 1))?;
            if rec.schema_version != TRIAL_SCHEMA_VERSION {
                anyhow::bail!(
                    "trials ledger line {}: unsupported schema version {} (this build reads {})",
                    i + 1,
                    rec.schema_version,
                    TRIAL_SCHEMA_VERSION
                );
            }
            out.push(rec);
        }
        Ok(out)
    }
}

/// Serialize a record to one scrubbed line + newline. Only the free-text fields
/// (`verdict`, `source`) route through the scrub; structured hash fields render
/// verbatim so lineage grouping survives (a 64-hex fingerprint would otherwise be
/// masked by the account-number heuristic).
fn to_scrubbed_line(record: &TrialRecord) -> anyhow::Result<String> {
    let mut rec = record.clone();
    rec.verdict = nautilus_ls::scrub::scrub_secrets(&rec.verdict);
    if let Some(s) = &rec.source {
        rec.source = Some(nautilus_ls::scrub::scrub_secrets(s));
    }
    let mut line = serde_json::to_string(&rec)?;
    line.push('\n');
    Ok(line)
}

/// A `trials count` outcome.
#[derive(Debug, Clone)]
pub struct CountOutcome {
    /// Total trials in the ledger.
    pub total: usize,
    /// Trials per lever family.
    pub per_family: BTreeMap<String, usize>,
    /// Trials per sample lineage (keyed by the lineage-root fingerprint after
    /// following parent links).
    pub per_lineage: BTreeMap<String, usize>,
    /// The report lines (printed by the bin).
    pub lines: Vec<String>,
}

/// Answer "how many trials against this sample, total / per family / per lineage"
/// (R12). Per-lineage counting union-finds fingerprints across their declared
/// parent links, so an evolving sample counts as one lineage even as its
/// fingerprint changes era to era.
///
/// # Errors
///
/// When the ledger cannot be read (see [`TrialsLedger::read_all`]).
pub fn count_trials(ledger: &TrialsLedger) -> anyhow::Result<CountOutcome> {
    let records = ledger.read_all()?;
    let total = records.len();

    let mut per_family: BTreeMap<String, usize> = BTreeMap::new();
    for r in &records {
        *per_family.entry(r.family.clone()).or_default() += 1;
    }

    // Union-find over fingerprints: parent_fingerprint links a child era to its
    // parent so both resolve to one lineage root.
    let mut uf = UnionFind::default();
    for r in &records {
        uf.ensure(&r.lineage.catalog_fingerprint);
        if let Some(parent) = &r.lineage.parent_fingerprint {
            uf.ensure(parent);
            uf.union(&r.lineage.catalog_fingerprint, parent);
        }
    }
    let mut per_lineage: BTreeMap<String, usize> = BTreeMap::new();
    for r in &records {
        let root = uf.find(&r.lineage.catalog_fingerprint);
        *per_lineage.entry(root).or_default() += 1;
    }

    let mut lines = vec![format!("trials total: {total}")];
    lines.push(format!("families: {}", per_family.len()));
    for (family, n) in &per_family {
        lines.push(format!("  family {family}: {n}"));
    }
    lines.push(format!("lineages: {}", per_lineage.len()));
    for (root, n) in &per_lineage {
        lines.push(format!("  lineage {root}: {n}"));
    }
    Ok(CountOutcome { total, per_family, per_lineage, lines })
}

/// A `trials record` outcome (a hand-run look appended).
#[derive(Debug, Clone)]
pub struct RecordOutcome {
    /// The report lines.
    pub lines: Vec<String>,
}

/// Append one hand-run trial from `LS_TRIAL_*` env (R10's record-only path).
/// Required: `LS_TRIAL_CANDIDATE`, `LS_TRIAL_FAMILY`, `LS_TRIAL_LOOK`,
/// `LS_TRIAL_FINGERPRINT`, `LS_TRIAL_VERDICT`. Optional: `LS_TRIAL_PARENT`,
/// `LS_TRIAL_READINGS` (a JSON object of `{key: number}`). A missing required var
/// is a loud refusal that appends nothing.
///
/// # Errors
///
/// When a required var is absent, `LS_TRIAL_LOOK` is unknown, `LS_TRIAL_READINGS`
/// is malformed, or the append fails.
pub fn record_from_env(ledger: &TrialsLedger, recorded_utc: impl Into<String>) -> anyhow::Result<RecordOutcome> {
    let req = |key: &str| -> anyhow::Result<String> {
        std::env::var(key)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("{key} is required for `trials record`"))
    };
    let candidate = req("LS_TRIAL_CANDIDATE")?;
    let family = req("LS_TRIAL_FAMILY")?;
    let look = LookKind::parse(&req("LS_TRIAL_LOOK")?)?;
    let catalog_fingerprint = req("LS_TRIAL_FINGERPRINT")?;
    let verdict = req("LS_TRIAL_VERDICT")?;
    let parent_fingerprint =
        std::env::var("LS_TRIAL_PARENT").ok().filter(|s| !s.trim().is_empty());
    let readings: BTreeMap<String, f64> = match std::env::var("LS_TRIAL_READINGS") {
        Ok(json) if !json.trim().is_empty() => serde_json::from_str(&json)
            .map_err(|e| anyhow::anyhow!("LS_TRIAL_READINGS must be a JSON object of numbers: {e}"))?,
        _ => BTreeMap::new(),
    };

    let record = TrialRecord::new(
        candidate.clone(),
        family.clone(),
        look,
        SampleLineage { catalog_fingerprint, parent_fingerprint },
        readings,
        verdict,
        recorded_utc,
    );
    ledger.append(&record)?;
    Ok(RecordOutcome {
        lines: vec![format!("recorded trial: candidate {candidate}, family {family}")],
    })
}

/// A minimal string union-find for lineage grouping.
#[derive(Default)]
struct UnionFind {
    parent: HashMap<String, String>,
}

impl UnionFind {
    fn ensure(&mut self, x: &str) {
        self.parent.entry(x.to_string()).or_insert_with(|| x.to_string());
    }

    fn find(&mut self, x: &str) -> String {
        self.ensure(x);
        let mut root = x.to_string();
        while self.parent[&root] != root {
            let grand = self.parent[&self.parent[&root]].clone();
            let p = self.parent.get_mut(&root).unwrap();
            *p = grand.clone(); // path halving
            root = grand;
        }
        root
    }

    fn union(&mut self, a: &str, b: &str) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            // Deterministic root: the lexicographically smaller fingerprint wins,
            // so the lineage key is stable regardless of insertion order.
            let (root, child) = if ra <= rb { (ra, rb) } else { (rb, ra) };
            self.parent.insert(child, root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn rec(candidate: &str, family: &str, look: LookKind, fp: &str, parent: Option<&str>, verdict: &str) -> TrialRecord {
        TrialRecord::new(
            candidate,
            family,
            look,
            SampleLineage {
                catalog_fingerprint: fp.to_string(),
                parent_fingerprint: parent.map(String::from),
            },
            BTreeMap::new(),
            verdict,
            "2026-07-16T00:00:00+00:00",
        )
    }

    #[test]
    fn append_two_and_read_back_in_order_absent_reads_empty() {
        let tmp = TempDir::new().unwrap();
        let ledger = TrialsLedger::new(tmp.path().join("ledger/trials.jsonl"));
        assert!(ledger.read_all().unwrap().is_empty(), "absent file reads empty");
        ledger.append(&rec("cand-a", "class-b", LookKind::GateReading, "fpA", None, "GO")).unwrap();
        ledger.append(&rec("cand-b", "class-b", LookKind::Flip, "fpA", None, "KEEP")).unwrap();
        let back = ledger.read_all().unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].candidate, "cand-a", "append order preserved");
        assert_eq!(back[1].candidate, "cand-b");
    }

    #[test]
    fn torn_final_line_is_a_typed_error_naming_the_line() {
        let tmp = TempDir::new().unwrap();
        let ledger = TrialsLedger::new(tmp.path().join("trials.jsonl"));
        ledger.append(&rec("cand-a", "f", LookKind::GateReading, "fp", None, "GO")).unwrap();
        let mut f = OpenOptions::new().append(true).open(ledger.path()).unwrap();
        write!(f, "{{\"schema_version\":1,\"recorded").unwrap();
        drop(f);
        let err = ledger.read_all().unwrap_err();
        assert!(err.to_string().contains("line 2"), "typed per-line error: {err}");
    }

    #[test]
    fn unknown_schema_version_is_refused() {
        let tmp = TempDir::new().unwrap();
        let ledger = TrialsLedger::new(tmp.path().join("trials.jsonl"));
        ledger.append(&rec("cand-a", "f", LookKind::GateReading, "fp", None, "GO")).unwrap();
        let mut bumped = serde_json::to_value(rec("c", "f", LookKind::Flip, "fp", None, "KEEP")).unwrap();
        bumped["schema_version"] = serde_json::json!(999);
        let mut f = OpenOptions::new().append(true).open(ledger.path()).unwrap();
        writeln!(f, "{bumped}").unwrap();
        drop(f);
        let err = ledger.read_all().unwrap_err();
        assert!(err.to_string().contains("schema version 999"), "{err}");
    }

    #[test]
    fn count_groups_by_family_and_by_lineage_with_parent_merge() {
        let tmp = TempDir::new().unwrap();
        let ledger = TrialsLedger::new(tmp.path().join("trials.jsonl"));
        // Two families; two eras of one lineage (fp2 descends from fp1) plus a
        // separate lineage fp3.
        ledger.append(&rec("c1", "class-b", LookKind::GateReading, "fp1", None, "GO")).unwrap();
        ledger.append(&rec("c2", "class-b", LookKind::Flip, "fp2", Some("fp1"), "KEEP")).unwrap();
        ledger.append(&rec("c3", "entry-filter", LookKind::SweepLeg, "fp3", None, "REVERT")).unwrap();

        let out = count_trials(&ledger).unwrap();
        assert_eq!(out.total, 3);
        assert_eq!(out.per_family["class-b"], 2);
        assert_eq!(out.per_family["entry-filter"], 1);
        // fp1 and fp2 merge into one lineage of 2; fp3 stands alone.
        assert_eq!(out.per_lineage.len(), 2, "two lineages: {:?}", out.per_lineage);
        assert_eq!(out.per_lineage.values().copied().max().unwrap(), 2, "the merged lineage counts 2");
    }

    #[test]
    fn scrub_masks_a_planted_secret_in_the_verdict_but_keeps_the_fingerprint() {
        let tmp = TempDir::new().unwrap();
        let ledger = TrialsLedger::new(tmp.path().join("trials.jsonl"));
        // A 64-hex fingerprint must survive; an account-like token in free text is masked.
        let fp = "a".repeat(64);
        ledger
            .append(&rec("c1", "class-b", LookKind::Flip, &fp, None, "KEEP acct 20187511401"))
            .unwrap();
        let bytes = std::fs::read_to_string(ledger.path()).unwrap();
        assert!(!bytes.contains("20187511401"), "secret masked in free text: {bytes}");
        assert!(bytes.contains(&fp), "structured fingerprint survives verbatim");
        // The masked line still parses.
        assert_eq!(ledger.read_all().unwrap().len(), 1);
    }
}
