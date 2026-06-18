//! Committed gate validator for the decommission audit (U6).
//!
//! This recomputes the decommission gate from the frozen per-row records under
//! `<workspace>/docs/migration-source/audit/` so the gate stays checkable in CI
//! **after** the old source is gone (R14). It does NOT re-compare records against
//! the old source — that comparison is a one-shot done while the source exists.
//! What it enforces forever is *record consistency*: schema validity, evidence-
//! pointer integrity (R13), inline claim transcription (R14), verdict-vs-ledger
//! reconciliation (R15), the credential-free non-negotiable across every
//! transcribed field (R12), source-coverage, and manifest<->ledger ID agreement.
//!
//! The gate-invariant logic is proven below against in-test fixture record sets
//! (the execution-note discipline: prove the logic before the real records
//! exist). The real records under `records/` are validated when present; until
//! the `audit-carried-rows` fleet runs, that set is empty and the gate computes
//! NOT-GREEN (incomplete) — the correct current state, not a test failure.
//!
//! Resolution idiom is copied locally from `ls-metadata`'s
//! `slice_metadata.rs::metadata_root` (there is no shared harness): records live
//! at `CARGO_MANIFEST_DIR/../../docs/migration-source/audit/` (two-level ascent
//! from `crates/ls-trackers`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Record schema (audit's OWN struct — NOT ls_metadata::EvidenceRecord).
// Mirrors .agents/skills/audit-carried-rows/references/record-format.md.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Record {
    row_id: String,
    #[allow(dead_code)]
    area: String,
    classification: String,
    verdict: String,
    #[allow(dead_code)]
    bar_applied: String,
    evidence_pointer: String,
    #[serde(default)]
    #[allow(dead_code)]
    provenance: Vec<String>,
    #[serde(default)]
    behavioral: Option<Behavioral>,
    #[serde(default)]
    knowledge: Option<Knowledge>,
    #[serde(default)]
    discard: Option<Discard>,
    #[serde(default)]
    re_disposition: Option<String>,
    #[serde(default)]
    gap: Option<String>,
    #[serde(default)]
    unverifiable_reason: Option<String>,
    #[serde(default)]
    acceptance: Option<Acceptance>,
}

#[derive(Debug, Deserialize)]
struct Behavioral {
    #[allow(dead_code)]
    target: String,
    #[serde(default)]
    line: Option<String>,
    #[serde(default)]
    production_only: bool,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Knowledge {
    #[serde(default)]
    extraction_mode: String,
    #[serde(default)]
    claim_map: Vec<Claim>,
    #[serde(default)]
    source_documents: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Claim {
    claim_text: String,
    target_location: String,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Deserialize)]
struct Discard {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    #[allow(dead_code)]
    coherence_note: String,
}

#[derive(Debug, Deserialize)]
struct Acceptance {
    accepted_by: String,
    acceptance_reason: String,
    accepted_date: String,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    rows: Vec<ManifestRow>,
}

#[derive(Debug, Deserialize, Clone)]
struct ManifestRow {
    id: String,
    area: String,
    disposition: String,
    #[serde(default)]
    #[allow(dead_code)]
    candidate_class: String,
    #[serde(default)]
    old_sources: Vec<String>,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <workspace>/crates/ls-trackers
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn audit_dir() -> PathBuf {
    workspace_root().join("docs").join("migration-source").join("audit")
}

fn records_dir() -> PathBuf {
    audit_dir().join("records")
}

fn manifest_path() -> PathBuf {
    audit_dir().join("manifest.yaml")
}

fn ledger_path() -> PathBuf {
    workspace_root()
        .join("docs")
        .join("migration-source-extraction-ledger.md")
}

fn example_path() -> PathBuf {
    workspace_root()
        .join(".agents")
        .join("skills")
        .join("audit-carried-rows")
        .join("references")
        .join("record-format.example.yaml")
}

// ---------------------------------------------------------------------------
// Credential-free non-negotiable (R12). Shared list — keep in lockstep with
// record-format.md "Credential-free non-negotiable".
// ---------------------------------------------------------------------------

const CRED_PATTERNS: &[&str] = &[
    "rsp_msg",
    "appkey",
    "app_key",
    "apikey",
    "api_key",
    "secret",
    "password",
    "passwd",
    "bearer",
    "authorization",
    "account_no",
    "acnt_no",
    "accountno",
    "account_number",
    "token=", // a token VALUE; "token_len=" is safe (a length, not the token)
];

fn scan_credentials(text: &str) -> Option<&'static str> {
    let low = text.to_lowercase();
    CRED_PATTERNS.iter().copied().find(|p| low.contains(p))
}

/// A claim must be transcribed inline in full (R14): non-empty and not a
/// reference into the soon-to-vanish old source.
fn inline_transcribed(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    let low = t.to_lowercase();
    // Reject PATH-style references into the soon-to-vanish source (a claim that
    // substitutes a pointer for its content), but allow legitimate mentions of
    // the repo name as the claim's substance (e.g. ADR 0010: "korea-broker-sdk-ls
    // is a migration source only").
    !(low.contains("korea-broker-sdk-ls/")
        || low.contains("see old source")
        || low.contains("old source line")
        || low.contains("old-source line"))
}

/// Evidence-pointer integrity (R13): `inline`, or a repo-relative path that is
/// not a `target/` artifact and resolves via the injected resolver (on disk +
/// git-tracked for real records). An old-source absolute path or a `../` escape
/// fails outright.
fn evidence_pointer_ok(ptr: &str, resolve: &dyn Fn(&str) -> bool) -> bool {
    if ptr == "inline" {
        return true;
    }
    if ptr.starts_with('/') || ptr.starts_with("../") {
        return false;
    }
    if ptr.starts_with("target/") {
        return false;
    }
    resolve(ptr)
}

// ---------------------------------------------------------------------------
// Per-record invariants (R12/R13/R14 + schema). Returns a list of violations;
// empty means the record is internally consistent.
// ---------------------------------------------------------------------------

fn record_errors(rec: &Record, resolve: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut e = Vec::new();
    let id = &rec.row_id;

    if !["behavioral", "knowledge", "discard"].contains(&rec.classification.as_str()) {
        e.push(format!("{id}: bad classification `{}`", rec.classification));
    }
    if !["confirmed", "refuted", "unverifiable", "assumption-accepted"].contains(&rec.verdict.as_str())
    {
        e.push(format!("{id}: bad verdict `{}`", rec.verdict));
    }

    if !evidence_pointer_ok(&rec.evidence_pointer, resolve) {
        e.push(format!(
            "{id}: evidence_pointer not inline / in-repo tracked path: `{}` (R13)",
            rec.evidence_pointer
        ));
    }

    // Credential scan across EVERY transcribed / free-text field (R12) — not
    // just the line/claim_text/acceptance_reason exemplars; any field copied
    // from an old-source doc can carry a credential field-name.
    let mut text_fields: Vec<(&str, &str)> = Vec::new();
    if let Some(b) = &rec.behavioral {
        if let Some(l) = &b.line {
            text_fields.push(("behavioral line", l));
        }
        if let Some(r) = &b.reason {
            text_fields.push(("behavioral reason", r));
        }
    }
    if let Some(k) = &rec.knowledge {
        for c in &k.claim_map {
            text_fields.push(("claim_text", &c.claim_text));
            text_fields.push(("claim target_location", &c.target_location));
        }
    }
    if let Some(d) = &rec.discard {
        text_fields.push(("discard reason", &d.reason));
        text_fields.push(("discard coherence_note", &d.coherence_note));
    }
    if let Some(a) = &rec.acceptance {
        text_fields.push(("acceptance_reason", &a.acceptance_reason));
    }
    if let Some(r) = &rec.unverifiable_reason {
        text_fields.push(("unverifiable_reason", r));
    }
    if let Some(g) = &rec.gap {
        text_fields.push(("gap", g));
    }
    for (name, text) in text_fields {
        if let Some(p) = scan_credentials(text) {
            e.push(format!("{id}: credential pattern `{p}` in {name} (R12)"));
        }
    }

    // Class block presence + class-specific checks.
    match rec.classification.as_str() {
        "behavioral" => {
            if rec.knowledge.is_some() || rec.discard.is_some() {
                e.push(format!("{id}: behavioral record carries a foreign class block"));
            }
            match &rec.behavioral {
                None => e.push(format!("{id}: behavioral record missing `behavioral` block")),
                Some(b) => {
                    if rec.verdict == "confirmed" {
                        if b.production_only {
                            e.push(format!(
                                "{id}: behavioral row marked confirmed while production_only (R6a)"
                            ));
                        }
                        if b.line.as_deref().unwrap_or("").trim().is_empty() {
                            e.push(format!("{id}: confirmed behavioral row has no captured line (R6)"));
                        }
                    }
                    if b.production_only && b.reason.as_deref().unwrap_or("").trim().is_empty() {
                        e.push(format!("{id}: production_only without a reason (R6a)"));
                    }
                }
            }
        }
        "knowledge" => {
            if rec.behavioral.is_some() || rec.discard.is_some() {
                e.push(format!("{id}: knowledge record carries a foreign class block"));
            }
            match &rec.knowledge {
                None => e.push(format!("{id}: knowledge record missing `knowledge` block")),
                Some(k) => {
                    if !k.extraction_mode.is_empty()
                        && !["full-transcription", "summary-plus-snapshot", "distilled-lesson"]
                            .contains(&k.extraction_mode.as_str())
                    {
                        e.push(format!("{id}: bad extraction_mode `{}`", k.extraction_mode));
                    }
                    // Per-claim validity runs for ANY verdict carrying a
                    // claim_map: R14 inline transcription must hold even for an
                    // assumption-accepted knowledge row (which counts toward
                    // green), not only for `confirmed`.
                    for c in &k.claim_map {
                        // A `missing` claim legitimately has no target_location (that
                        // is why it is missing); present/adapted claims must locate.
                        if c.status != "missing" && c.target_location.trim().is_empty() {
                            e.push(format!("{id}: a present/adapted claim has no target_location (R7a)"));
                        }
                        if !inline_transcribed(&c.claim_text) {
                            e.push(format!("{id}: claim_text not inline-transcribed in full (R14)"));
                        }
                        if !c.status.is_empty()
                            && !["present", "adapted", "missing"].contains(&c.status.as_str())
                        {
                            e.push(format!("{id}: bad claim status `{}`", c.status));
                        }
                    }
                    if rec.verdict == "confirmed" {
                        if k.extraction_mode.trim().is_empty() {
                            e.push(format!(
                                "{id}: confirmed knowledge row has no extraction_mode (R7b)"
                            ));
                        }
                        if k.claim_map.is_empty() {
                            e.push(format!("{id}: confirmed knowledge row has an empty claim_map (R7a)"));
                        }
                    }
                }
            }
        }
        "discard" => {
            if rec.behavioral.is_some() || rec.knowledge.is_some() {
                e.push(format!("{id}: discard record carries a foreign class block"));
            }
            match &rec.discard {
                None => e.push(format!("{id}: discard record missing `discard` block")),
                Some(d) => {
                    if d.reason.trim().is_empty() {
                        e.push(format!("{id}: discard row has no recorded reason (R8)"));
                    }
                    if rec.verdict == "confirmed" && d.coherence_note.trim().is_empty() {
                        e.push(format!("{id}: confirmed discard row has no coherence_note (R8)"));
                    }
                }
            }
        }
        _ => {}
    }

    // Verdict tail blocks.
    match rec.verdict.as_str() {
        "refuted" => {
            match rec.re_disposition.as_deref() {
                Some(rd) if ["extract", "defer", "discard"].contains(&rd) => {}
                _ => e.push(format!("{id}: refuted row needs re_disposition extract/defer/discard (R3)")),
            }
            if rec.gap.as_deref().unwrap_or("").trim().is_empty() {
                e.push(format!("{id}: refuted row must name the gap (R3)"));
            }
        }
        "unverifiable" => {
            if rec.unverifiable_reason.as_deref().unwrap_or("").trim().is_empty() {
                e.push(format!("{id}: unverifiable row missing reason (R4)"));
            }
        }
        "assumption-accepted" => {
            if rec.unverifiable_reason.as_deref().unwrap_or("").trim().is_empty() {
                e.push(format!("{id}: accepted row missing unverifiable_reason (R4)"));
            }
            match &rec.acceptance {
                None => e.push(format!("{id}: accepted row missing acceptance block (R4a)")),
                Some(a) => {
                    if a.accepted_by.trim().is_empty() {
                        e.push(format!("{id}: acceptance missing accepted_by (R4a)"));
                    }
                    if a.acceptance_reason.trim().is_empty() {
                        e.push(format!("{id}: acceptance missing acceptance_reason (R4a)"));
                    }
                    if a.accepted_date.trim().is_empty() {
                        e.push(format!("{id}: acceptance missing accepted_date (R4a)"));
                    }
                }
            }
        }
        _ => {}
    }

    e
}

// ---------------------------------------------------------------------------
// Gate computation (R15). Returns blocking reasons; empty == trustworthy-green.
// `ledger` is the CURRENT disposition per id; each manifest row carries the
// ORIGINAL disposition for the drift cross-check.
// ---------------------------------------------------------------------------

fn compute_gate(
    expected: &[ManifestRow],
    records: &BTreeMap<String, Record>,
    ledger: &BTreeMap<String, String>,
    resolve: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    let mut blockers = Vec::new();
    for m in expected {
        let orig = &m.disposition;
        let cur = ledger.get(&m.id).cloned().unwrap_or_default();
        let Some(r) = records.get(&m.id) else {
            blockers.push(format!("{}: missing verdict (R15)", m.id));
            continue;
        };

        // A verdict can only count toward (or against) the gate if its record is
        // well-formed AND maps to the right row. Green requires a valid record,
        // not merely a `confirmed` string — this is the gate's soundness floor.
        if r.row_id != m.id {
            blockers.push(format!(
                "{}: record internal row_id `{}` != manifest id (R15)",
                m.id, r.row_id
            ));
        }
        for err in record_errors(r, resolve) {
            blockers.push(err);
        }

        match r.verdict.as_str() {
            "confirmed" | "assumption-accepted" => {
                if &cur != orig {
                    blockers.push(format!(
                        "{}: verdict {} but ledger disposition drifted {} -> {} (R15)",
                        m.id, r.verdict, orig, cur
                    ));
                }
            }
            "unverifiable" => {
                blockers.push(format!("{}: unverifiable (un-accepted) (R4/R15)", m.id));
                if &cur != orig {
                    blockers.push(format!("{}: unverifiable but ledger drifted to {} (R15)", m.id, cur));
                }
            }
            "refuted" => {
                let rd = r.re_disposition.clone().unwrap_or_default();
                if &cur == orig {
                    blockers.push(format!("{}: refuted but ledger still `{}` (drift, R3)", m.id, cur));
                } else if cur != rd {
                    blockers.push(format!(
                        "{}: refuted re_disposition `{}` != ledger `{}` (R3)",
                        m.id, rd, cur
                    ));
                } else {
                    blockers.push(format!("{}: refuted -> {} (re-blocks the gate, R3)", m.id, rd));
                }
            }
            other => blockers.push(format!("{}: unknown verdict `{}`", m.id, other)),
        }
    }
    blockers
}

/// Old-source documents referenced by manifest rows that no record's claim-map
/// `source_documents` claims (under-enumeration surface, R7a / U6).
fn source_coverage_gaps(manifest: &[ManifestRow], records: &[Record]) -> Vec<String> {
    // Only document-path old_sources are claimable by a record's claim-map;
    // manifest prose provenance ("old runtime layout", "WS tests") is not a
    // document and would otherwise read as a perpetual false under-enumeration.
    let referenced: BTreeSet<String> = manifest
        .iter()
        .flat_map(|m| m.old_sources.iter().cloned())
        .filter(|s| s.contains('/'))
        .collect();
    let covered: BTreeSet<String> = records
        .iter()
        .filter_map(|r| r.knowledge.as_ref())
        .flat_map(|k| k.source_documents.iter().cloned())
        .collect();
    referenced.difference(&covered).cloned().collect()
}

// ---------------------------------------------------------------------------
// File loaders / ledger parsing (robust, by header name — not naive split).
// ---------------------------------------------------------------------------

fn parse_manifest() -> Vec<ManifestRow> {
    let yaml = fs::read_to_string(manifest_path())
        .unwrap_or_else(|e| panic!("read manifest {}: {e}", manifest_path().display()));
    let m: Manifest = serde_yaml::from_str(&yaml)
        .unwrap_or_else(|e| panic!("parse manifest {}: {e}", manifest_path().display()));
    m.rows
}

/// Split a markdown table row into trimmed cells, dropping the leading/trailing
/// pipe. Cells contain backticks and prose but no literal `|`, so a `|` split is
/// safe once the row borders are removed.
fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn is_l_id(s: &str) -> bool {
    s.starts_with('L') && s.len() > 1 && s[1..].chars().all(|c| c.is_ascii_digit())
}

/// (id, area, current-disposition) for each `L<N>` row, located by header name.
fn ledger_rows() -> Vec<(String, String, String)> {
    let content = fs::read_to_string(ledger_path())
        .unwrap_or_else(|e| panic!("read ledger {}: {e}", ledger_path().display()));
    let mut header: Option<(usize, usize, usize)> = None; // (id, area, disposition)
    let mut out = Vec::new();
    for line in content.lines() {
        // Stop at the next heading once the table has started, so a later pipe
        // table elsewhere in the doc cannot be parsed as ledger data.
        if header.is_some() && line.trim_start().starts_with('#') {
            break;
        }
        if !line.trim_start().starts_with('|') {
            continue;
        }
        let cs = cells(line);
        if header.is_none() {
            let id = cs.iter().position(|c| c.eq_ignore_ascii_case("ID"));
            let area = cs.iter().position(|c| c.eq_ignore_ascii_case("Area"));
            let disp = cs.iter().position(|c| c.eq_ignore_ascii_case("Current disposition"));
            if let (Some(i), Some(a), Some(d)) = (id, area, disp) {
                header = Some((i, a, d));
            }
            continue;
        }
        let (i, a, d) = header.unwrap();
        let max = i.max(a).max(d);
        if cs.len() <= max {
            continue;
        }
        let id = cs[i].trim_matches('`').trim().to_string();
        if !is_l_id(&id) {
            continue;
        }
        let area = cs[a].trim().to_string();
        let disp = cs[d].trim_matches('`').trim().to_string();
        out.push((id, area, disp));
    }
    out
}

fn ledger_map() -> BTreeMap<String, String> {
    ledger_rows().into_iter().map(|(id, _, d)| (id, d)).collect()
}

/// Returns (conforming `L<N>.yaml` files, non-conforming files) under records/.
fn list_record_files() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let dir = records_dir();
    let mut keep = Vec::new();
    let mut other = Vec::new();
    if !dir.exists() {
        return (keep, other);
    }
    for entry in fs::read_dir(&dir).expect("read records dir") {
        let path = entry.expect("dir entry").path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match name.strip_suffix(".yaml") {
            Some(stem) if is_l_id(stem) => keep.push(path),
            // a .gitkeep or README is fine; only stray *.yaml that is not L<N> is "other"
            _ if name.ends_with(".yaml") => other.push(path),
            _ => {}
        }
    }
    (keep, other)
}

fn read_records_map() -> BTreeMap<String, Record> {
    let (keep, _) = list_record_files();
    let mut map = BTreeMap::new();
    for path in keep {
        // Key by FILENAME stem, not the record's internal row_id — otherwise a
        // record whose internal row_id disagrees with its filename would
        // silently mis-map or overwrite another row. The disagreement itself is
        // caught as a blocker in compute_gate (record row_id != manifest id).
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let yaml = fs::read_to_string(&path).expect("read record");
        let rec: Record = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("parse record {}: {e}", path.display()));
        map.insert(stem, rec);
    }
    map
}

fn is_git_tracked(rel: &str) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(workspace_root())
        .args(["ls-files", "--error-unmatch", rel])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn real_resolver(rel: &str) -> bool {
    workspace_root().join(rel).is_file() && is_git_tracked(rel)
}

// ===========================================================================
// Tests over the REAL authored artifacts (hold today).
// ===========================================================================

/// Manifest <-> ledger ID reconciliation (U1/U6): every `L<N>` <-> area mapping
/// agrees 1:1 between the manifest and the parsed ledger table, so a future
/// ledger reorder cannot silently re-map an ID.
#[test]
fn manifest_and_ledger_reconcile_one_to_one() {
    let manifest = parse_manifest();
    let ledger = ledger_rows();

    assert_eq!(manifest.len(), 26, "manifest must enumerate 26 rows");
    assert_eq!(ledger.len(), 26, "ledger must carry 26 L<N> rows");

    let m_ids: BTreeMap<String, String> =
        manifest.iter().map(|m| (m.id.clone(), m.area.clone())).collect();
    let l_ids: BTreeMap<String, String> =
        ledger.iter().map(|(id, area, _)| (id.clone(), area.clone())).collect();

    let m_set: BTreeSet<_> = m_ids.keys().cloned().collect();
    let l_set: BTreeSet<_> = l_ids.keys().cloned().collect();
    assert_eq!(m_set, l_set, "manifest and ledger ID sets must match");

    for (id, m_area) in &m_ids {
        assert_eq!(
            Some(m_area),
            l_ids.get(id),
            "area for {id} must match between manifest and ledger"
        );
    }

    // Disposition agreement: the manifest snapshot equals the ledger for every
    // row that has NOT been legitimately re-dispositioned by a refuted record.
    // (Until the fleet runs there are no records, so this guards the snapshot;
    // once a row is refuted and the ledger moves carried -> extract, that row is
    // skipped here — the drift discipline for it lives in compute_gate.)
    let ldisp = ledger_map();
    let records = read_records_map();
    for m in &manifest {
        let refuted = records.get(&m.id).map(|r| r.verdict == "refuted").unwrap_or(false);
        if refuted {
            continue;
        }
        assert_eq!(
            ldisp.get(&m.id),
            Some(&m.disposition),
            "{}: manifest disposition must match the ledger at audit start",
            m.id
        );
    }
}

/// The worked examples (one per class, outside `records/`) satisfy the
/// per-record invariants — schema, pointer, credential, inline transcription.
#[test]
fn example_records_satisfy_per_record_invariants() {
    let content = fs::read_to_string(example_path())
        .unwrap_or_else(|e| panic!("read example {}: {e}", example_path().display()));
    let mut count = 0;
    for doc in serde_yaml::Deserializer::from_str(&content) {
        let rec = Record::deserialize(doc).expect("example doc deserializes as a Record");
        let errs = record_errors(&rec, &|_: &str| true);
        assert!(errs.is_empty(), "example {} invalid: {:?}", rec.row_id, errs);
        count += 1;
    }
    assert!(count >= 3, "expected a worked example per class (>=3), got {count}");
}

/// Real records (when the fleet has run) are internally consistent and reconcile
/// with the ledger. Empty until the audit runs — then this passes vacuously.
#[test]
fn real_records_are_internally_consistent() {
    let (keep, other) = list_record_files();
    assert!(
        other.is_empty(),
        "records/ must contain only L<N>.yaml record files; stray: {other:?}"
    );

    let manifest = parse_manifest();
    let orig: BTreeMap<String, String> =
        manifest.iter().map(|m| (m.id.clone(), m.disposition.clone())).collect();
    let ledger = ledger_map();

    for path in &keep {
        let yaml = fs::read_to_string(path).expect("read record");
        let rec: Record = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));

        let stem = path.file_stem().unwrap().to_string_lossy();
        assert_eq!(stem, rec.row_id, "record filename must equal row_id");
        let m = manifest
            .iter()
            .find(|m| m.id == rec.row_id)
            .unwrap_or_else(|| panic!("{} is not a manifest id", rec.row_id));
        assert_eq!(m.area, rec.area, "{}: record area must match the manifest area", rec.row_id);

        let errs = record_errors(&rec, &real_resolver);
        assert!(errs.is_empty(), "{} invalid: {:?}", path.display(), errs);

        // Per-row verdict-vs-ledger reconciliation (drift discipline, R15).
        let cur = ledger.get(&rec.row_id).cloned().unwrap_or_default();
        let o = orig.get(&rec.row_id).cloned().unwrap_or_default();
        match rec.verdict.as_str() {
            "refuted" => assert_ne!(
                cur, o,
                "{}: refuted but ledger still `{}` (R3 drift)",
                rec.row_id, cur
            ),
            "confirmed" | "unverifiable" | "assumption-accepted" => assert_eq!(
                cur, o,
                "{}: verdict {} but ledger `{}` != original `{}` (R15 drift)",
                rec.row_id, rec.verdict, cur, o
            ),
            _ => {}
        }
    }
}

/// Until the fleet runs, the gate is honestly NOT-GREEN (rows missing verdicts).
#[test]
fn gate_is_not_green_until_the_audit_runs() {
    let manifest = parse_manifest();
    let records = read_records_map();
    let ledger = ledger_map();
    let blockers = compute_gate(&manifest, &records, &ledger, &real_resolver);
    if records.len() < manifest.len() {
        assert!(
            !blockers.is_empty(),
            "an incomplete audit (records {} < {}) must compute NOT-GREEN",
            records.len(),
            manifest.len()
        );
    }
}

// ===========================================================================
// Gate-invariant logic proven against in-test fixtures (execution note).
// ===========================================================================

fn rec(yaml: &str) -> Record {
    serde_yaml::from_str(yaml).expect("fixture record parses")
}

fn mrow(id: &str, disp: &str) -> ManifestRow {
    ManifestRow {
        id: id.into(),
        area: format!("area {id}"),
        disposition: disp.into(),
        candidate_class: String::new(),
        old_sources: vec![],
    }
}

const OK_BEHAVIORAL: &str = r#"
row_id: L16
area: WebSocket lifecycle runtime
classification: behavioral
verdict: confirmed
bar_applied: passing smoke
evidence_pointer: inline
behavioral:
  target: "make live-smoke-ws"
  line: "LIVE-SMOKE target=live-smoke-ws result=[connected subscribed unsubscribed]"
"#;

const OK_KNOWLEDGE: &str = r#"
row_id: L5
area: Full response-code taxonomy
classification: knowledge
verdict: confirmed
bar_applied: completeness-vs-source (full-transcription)
evidence_pointer: inline
knowledge:
  extraction_mode: full-transcription
  source_documents:
    - docs/certification_taxonomy.md
  claim_map:
    - claim_text: "01900 is the sole paper-incompatible signal."
      target_location: docs/design/ls-gateway-response-semantics.md
      status: present
"#;

const OK_DISCARD: &str = r#"
row_id: L25
area: Old generated Rust API surface
classification: discard
verdict: confirmed
bar_applied: presence-and-coherence
evidence_pointer: inline
discard:
  reason: "Intentional non-carry of the generated surface."
  coherence_note: "Coherent with ADR 0010."
"#;

#[test]
fn fixtures_each_class_confirmed_pass() {
    for y in [OK_BEHAVIORAL, OK_KNOWLEDGE, OK_DISCARD] {
        let r = rec(y);
        assert!(record_errors(&r, &|_: &str| true).is_empty(), "{} should be valid", r.row_id);
    }
}

#[test]
fn credential_scan_flags_each_field() {
    // behavioral line
    let mut r = rec(OK_BEHAVIORAL);
    r.behavioral.as_mut().unwrap().line =
        Some("LIVE-SMOKE result=[appkey=ABC123]".into());
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("appkey")));

    // claim_text
    let mut r = rec(OK_KNOWLEDGE);
    r.knowledge.as_mut().unwrap().claim_map[0].claim_text =
        "account_no 12345678 must be redacted".into();
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("account_no")));

    // acceptance_reason
    let r = rec(
        r#"
row_id: L9
area: x
classification: knowledge
verdict: assumption-accepted
bar_applied: accepted
evidence_pointer: inline
knowledge:
  extraction_mode: distilled-lesson
  claim_map: []
unverifiable_reason: "no runtime"
acceptance:
  accepted_by: "maintainer"
  acceptance_reason: "rsp_msg leaked here"
  accepted_date: 2026-06-18
"#,
    );
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("rsp_msg")));

    // token_len is NOT a credential (length, not the token)
    assert!(scan_credentials("token_len=380 rsp_cd=00000 price=346500").is_none());
}

#[test]
fn pointer_integrity_cases() {
    let resolves = |_: &str| true;
    let missing = |_: &str| false;

    assert!(evidence_pointer_ok("inline", &resolves));
    assert!(evidence_pointer_ok("docs/design/x.md", &resolves));
    // old-source absolute path
    assert!(!evidence_pointer_ok("/Users/x/korea-broker-sdk-ls/docs/a.md", &resolves));
    // target artifact
    assert!(!evidence_pointer_ok("target/debug/out", &resolves));
    // escape
    assert!(!evidence_pointer_ok("../secret.txt", &resolves));
    // uncommitted / unresolvable in-repo path
    assert!(!evidence_pointer_ok("docs/uncommitted.md", &missing));
}

#[test]
fn inline_transcription_rejects_reference_and_empty() {
    assert!(inline_transcribed("01900 is paper-incompatible."));
    // Legitimate repo-naming as claim substance (ADR 0010) is fine; only
    // path-style references are rejected.
    assert!(inline_transcribed("korea-broker-sdk-ls is a migration source only (ADR 0010)"));
    assert!(!inline_transcribed(""));
    assert!(!inline_transcribed("see old source line 42"));
    assert!(!inline_transcribed("korea-broker-sdk-ls/docs/x.md describes it"));

    // a confirmed knowledge row with a by-reference claim fails record_errors
    let mut r = rec(OK_KNOWLEDGE);
    r.knowledge.as_mut().unwrap().claim_map[0].claim_text = "see old source line 42".into();
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("R14")));
}

#[test]
fn behavioral_confirmed_rejects_production_only_and_missing_line() {
    let mut r = rec(OK_BEHAVIORAL);
    r.behavioral.as_mut().unwrap().production_only = true;
    r.behavioral.as_mut().unwrap().reason = Some("prod only".into());
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("R6a")));

    let mut r = rec(OK_BEHAVIORAL);
    r.behavioral.as_mut().unwrap().line = None;
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("R6")));
}

#[test]
fn knowledge_confirmed_requires_mode_and_locations() {
    let mut r = rec(OK_KNOWLEDGE);
    r.knowledge.as_mut().unwrap().extraction_mode = String::new();
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("R7b")));

    let mut r = rec(OK_KNOWLEDGE);
    r.knowledge.as_mut().unwrap().claim_map[0].target_location = String::new();
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("R7a")));
}

#[test]
fn refuted_requires_re_disposition_and_gap() {
    // valid refuted record
    let ok = rec(
        r#"
row_id: L4
area: x
classification: behavioral
verdict: refuted
bar_applied: no proof
evidence_pointer: inline
behavioral:
  target: "cargo test"
re_disposition: extract
gap: "behavior described in a doc but no passing test exists"
"#,
    );
    assert!(record_errors(&ok, &|_: &str| true).is_empty());

    let mut bad = rec(
        r#"
row_id: L4
area: x
classification: behavioral
verdict: refuted
bar_applied: no proof
evidence_pointer: inline
behavioral:
  target: "cargo test"
gap: "missing"
"#,
    );
    assert!(record_errors(&bad, &|_: &str| true).iter().any(|e| e.contains("re_disposition")));
    bad.re_disposition = Some("nonsense".into());
    assert!(record_errors(&bad, &|_: &str| true).iter().any(|e| e.contains("re_disposition")));
}

#[test]
fn accepted_requires_full_acceptance_block() {
    let mut r = rec(
        r#"
row_id: L6
area: x
classification: behavioral
verdict: assumption-accepted
bar_applied: production-only
evidence_pointer: inline
behavioral:
  target: "(none)"
  production_only: true
  reason: "prod only"
unverifiable_reason: "production-only (R6a)"
acceptance:
  accepted_by: "maintainer"
  acceptance_reason: "residual risk: order-ack codes design-only"
  accepted_date: 2026-06-18
"#,
    );
    assert!(record_errors(&r, &|_: &str| true).is_empty());

    r.acceptance = None;
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("R4a")));
}

#[test]
fn gate_green_when_all_confirmed_or_accepted() {
    let manifest = vec![mrow("L1", "carried"), mrow("L2", "carried")];
    let ledger: BTreeMap<String, String> =
        [("L1", "carried"), ("L2", "carried")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();

    let mut records = BTreeMap::new();
    records.insert("L1".into(), rec(OK_KNOWLEDGE_L1));
    records.insert("L2".into(), rec(ACCEPTED_L2));

    assert!(
        compute_gate(&manifest, &records, &ledger, &|_: &str| true).is_empty(),
        "should be green"
    );
}

#[test]
fn gate_not_green_on_missing_refuted_and_unaccepted() {
    let manifest = vec![mrow("L1", "carried"), mrow("L2", "carried"), mrow("L3", "carried")];

    // L1 missing; L2 refuted (ledger re-dispositioned to extract); L3 unverifiable.
    let ledger: BTreeMap<String, String> = [("L1", "carried"), ("L2", "extract"), ("L3", "carried")]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    let mut records = BTreeMap::new();
    records.insert(
        "L2".into(),
        rec(r#"
row_id: L2
area: x
classification: knowledge
verdict: refuted
bar_applied: gap
evidence_pointer: inline
knowledge:
  extraction_mode: full-transcription
  claim_map: []
re_disposition: extract
gap: "missing constraint"
"#),
    );
    records.insert(
        "L3".into(),
        rec(r#"
row_id: L3
area: x
classification: behavioral
verdict: unverifiable
bar_applied: no env
evidence_pointer: inline
behavioral:
  target: "make live-smoke-ws"
  production_only: false
unverifiable_reason: "no reachable paper gateway"
"#),
    );

    let blockers = compute_gate(&manifest, &records, &ledger, &|_: &str| true);
    assert!(blockers.iter().any(|b| b.contains("L1") && b.contains("missing")));
    assert!(blockers.iter().any(|b| b.contains("L2") && b.contains("refuted")));
    assert!(blockers.iter().any(|b| b.contains("L3") && b.contains("unverifiable")));
}

#[test]
fn gate_drift_fails_refuted_but_ledger_still_carried() {
    let manifest = vec![mrow("L2", "carried")];
    // record refuted, but the ledger was NOT re-dispositioned.
    let ledger: BTreeMap<String, String> =
        [("L2", "carried")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let mut records = BTreeMap::new();
    records.insert(
        "L2".into(),
        rec(r#"
row_id: L2
area: x
classification: knowledge
verdict: refuted
bar_applied: gap
evidence_pointer: inline
knowledge:
  extraction_mode: full-transcription
  claim_map: []
re_disposition: extract
gap: "missing"
"#),
    );
    let blockers = compute_gate(&manifest, &records, &ledger, &|_: &str| true);
    assert!(blockers.iter().any(|b| b.contains("drift")), "expected drift blocker: {blockers:?}");
}

#[test]
fn source_coverage_flags_unclaimed_source() {
    let mut manifest = vec![mrow("L5", "carried")];
    manifest[0].old_sources = vec!["docs/A.md".into(), "docs/B.md".into()];

    // a record that only claims A leaves B uncovered
    let r = rec(r#"
row_id: L5
area: x
classification: knowledge
verdict: confirmed
bar_applied: completeness
evidence_pointer: inline
knowledge:
  extraction_mode: summary-plus-snapshot
  source_documents:
    - docs/A.md
  claim_map:
    - claim_text: "a claim"
      target_location: docs/target.md
      status: present
"#);
    let gaps = source_coverage_gaps(&manifest, std::slice::from_ref(&r));
    assert_eq!(gaps, vec!["docs/B.md".to_string()]);
}

#[test]
fn gate_rejects_confirmed_or_accepted_with_invalid_body() {
    let manifest = vec![mrow("L1", "carried")];
    let ledger: BTreeMap<String, String> =
        [("L1", "carried")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();

    // 'confirmed' knowledge with an EMPTY claim_map must not green the gate.
    let mut records = BTreeMap::new();
    records.insert(
        "L1".into(),
        rec(r#"
row_id: L1
area: x
classification: knowledge
verdict: confirmed
bar_applied: completeness
evidence_pointer: inline
knowledge:
  extraction_mode: full-transcription
  claim_map: []
"#),
    );
    assert!(
        !compute_gate(&manifest, &records, &ledger, &|_: &str| true).is_empty(),
        "a confirmed knowledge row with no claim_map must not be green"
    );

    // 'assumption-accepted' with NO acceptance block must not green the gate.
    let mut records = BTreeMap::new();
    records.insert(
        "L1".into(),
        rec(r#"
row_id: L1
area: x
classification: behavioral
verdict: assumption-accepted
bar_applied: production-only
evidence_pointer: inline
behavioral:
  target: "(none)"
  production_only: true
  reason: "prod only"
unverifiable_reason: "production-only (R6a)"
"#),
    );
    let blockers = compute_gate(&manifest, &records, &ledger, &|_: &str| true);
    assert!(
        blockers.iter().any(|b| b.contains("R4a")),
        "accepted-without-acceptance must block the gate: {blockers:?}"
    );
}

#[test]
fn gate_blocks_record_row_id_mismatch() {
    let manifest = vec![mrow("L1", "carried")];
    let ledger: BTreeMap<String, String> =
        [("L1", "carried")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    // file would be L1.yaml (key L1) but its internal row_id says L2.
    let mut records = BTreeMap::new();
    let mut r = rec(OK_KNOWLEDGE_L1);
    r.row_id = "L2".into();
    records.insert("L1".into(), r);
    let blockers = compute_gate(&manifest, &records, &ledger, &|_: &str| true);
    assert!(
        blockers.iter().any(|b| b.contains("internal row_id")),
        "a record whose internal row_id != its key must block: {blockers:?}"
    );
}

#[test]
fn credential_scan_covers_discard_unverifiable_and_gap_fields() {
    // discard.reason
    let mut r = rec(OK_DISCARD);
    r.discard.as_mut().unwrap().reason = "appkey=ABC must not leak".into();
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("discard reason")));

    // unverifiable_reason
    let r = rec(r#"
row_id: L3
area: x
classification: behavioral
verdict: unverifiable
bar_applied: no env
evidence_pointer: inline
behavioral:
  target: "make live-smoke-ws"
unverifiable_reason: "blocked: account_no 123 in the way"
"#);
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("unverifiable_reason")));

    // gap
    let r = rec(r#"
row_id: L4
area: x
classification: behavioral
verdict: refuted
bar_applied: no proof
evidence_pointer: inline
behavioral:
  target: "cargo test"
re_disposition: extract
gap: "missing; secret token in old doc"
"#);
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("gap")));
}

#[test]
fn inline_transcription_enforced_for_accepted_knowledge_row() {
    // An assumption-accepted knowledge row counts toward green, so its claims
    // must still be inline-transcribed (R14) — a by-reference claim must fail.
    let r = rec(r#"
row_id: L9
area: x
classification: knowledge
verdict: assumption-accepted
bar_applied: accepted
evidence_pointer: inline
knowledge:
  extraction_mode: distilled-lesson
  claim_map:
    - claim_text: "see old source line 42"
      target_location: docs/target.md
      status: present
unverifiable_reason: "subjective completeness; maintainer accepted"
acceptance:
  accepted_by: "maintainer"
  acceptance_reason: "residual risk named"
  accepted_date: 2026-06-18
"#);
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("R14")));
}

#[test]
fn confirmed_discard_requires_coherence_note() {
    let mut r = rec(OK_DISCARD);
    r.discard.as_mut().unwrap().coherence_note = String::new();
    assert!(record_errors(&r, &|_: &str| true).iter().any(|e| e.contains("coherence_note")));
}

// Re-id'd OK fixtures for the gate-green test.
const OK_KNOWLEDGE_L1: &str = r#"
row_id: L1
area: x
classification: knowledge
verdict: confirmed
bar_applied: completeness
evidence_pointer: inline
knowledge:
  extraction_mode: full-transcription
  claim_map:
    - claim_text: "a transcribed claim"
      target_location: docs/target.md
      status: present
"#;

const ACCEPTED_L2: &str = r#"
row_id: L2
area: x
classification: behavioral
verdict: assumption-accepted
bar_applied: production-only
evidence_pointer: inline
behavioral:
  target: "(none)"
  production_only: true
  reason: "prod only"
unverifiable_reason: "production-only (R6a)"
acceptance:
  accepted_by: "maintainer"
  acceptance_reason: "residual risk named"
  accepted_date: 2026-06-18
"#;
