//! Legacy TODO-file merge-block (plan 2026-07-29-002 U8; requirement R16, decision KTD5).
//!
//! Mirrors `adapters/nautilus/nautilus-ls-calendar/tests/merge_block.rs` with INVERTED
//! polarity: that guard requires a PASS verdict record to EXIST before a weekday
//! primitive may be deleted; this one requires that WHEN the queue cutover verdict
//! (`queue/cutover-verdict.json`) is PASS, the legacy TODO staging files
//! (`TODO.ATTENDED.md`, `TODO.OFFLINE.md`, `**/TODO-*.md` outside `docs/` and
//! `target/`) must NOT exist — `queue/items.jsonl` is then the sole staging location.
//!
//! Through the Shadow phase (no verdict file, or a non-PASS verdict) the coupling is
//! inert and every test here is green even though legacy TODO files are present in the
//! tree — so this file is deliberately `#[ignore]`-free and runs in the default
//! `make adapter-check` / adapter CI lane (cheap: one tree walk + one script self-test).
//!
//! The verdict is read by a tolerant STRING SCAN (no serde on the verdict, mirroring
//! merge_block.rs's manual scan): strip all whitespace, look for `"verdict":"PASS"`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The repo root, via the lab crate's shared ascent helper.
fn repo_root() -> PathBuf {
    nautilus_ls_lab::queue::repo_root().expect("repo root (.git) reachable from CARGO_MANIFEST_DIR")
}

/// Tolerant verdict scan (KTD5): strip ALL whitespace, then look for
/// `"verdict":"PASS"` — accepts `"verdict": "PASS"`, `"verdict":"PASS"`, etc.
/// Anything else (absent file, HOLD, malformed) is NOT a pass → guard inert.
fn verdict_text_is_pass(text: &str) -> bool {
    let squashed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    squashed.contains("\"verdict\":\"PASS\"")
}

/// Whether the repo's committed cutover verdict is PASS right now.
fn tree_verdict_is_pass(root: &Path) -> bool {
    fs::read_to_string(root.join("queue/cutover-verdict.json"))
        .map(|t| verdict_text_is_pass(&t))
        .unwrap_or(false)
}

/// A legacy TODO staging file by NAME: the two fixed staging files plus the dated
/// `TODO-*.md` convention the queue cutover retires.
fn is_legacy_todo_name(name: &str) -> bool {
    name == "TODO.ATTENDED.md"
        || name == "TODO.OFFLINE.md"
        || (name.starts_with("TODO-") && name.ends_with(".md"))
}

/// Directory names the walk skips: the guard's own exclusions (`docs/`, `target/`
/// at any depth) plus `.git` and the repo's gitignored data homes — the script
/// side gets those for free from `git ls-files --exclude-standard`; the walk skips
/// them by name so both sides see the same scope.
const SKIP_DIRS: &[&str] = &[
    ".git", "target", "docs", "data", "probes", "state", ".gate-run", "node_modules",
];

/// Collect legacy TODO files under `dir` (repo-relative paths), skipping SKIP_DIRS.
fn collect_legacy_todos(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_legacy_todos(root, &path, out);
        } else if is_legacy_todo_name(&name) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            out.push(rel);
        }
    }
}

/// The coupling RULE (inverted merge_block polarity): while the cutover verdict is
/// absent or not PASS (Shadow phase), legacy TODO files are fine — nothing has been
/// cut over. Once the verdict is PASS, any surviving legacy TODO file is a violation.
fn legacy_todos_allowed(verdict_pass: bool, offenders: &[String]) -> Result<(), String> {
    if !verdict_pass {
        return Ok(());
    }
    if offenders.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "cutover verdict is PASS but {} legacy TODO staging file(s) remain: {}",
            offenders.len(),
            offenders.join(", ")
        ))
    }
}

#[test]
fn coupling_rule_blocks_legacy_todos_only_after_pass() {
    let none: Vec<String> = vec![];
    let some = vec!["TODO.ATTENDED.md".to_string()];

    // Shadow phase (verdict absent / not PASS) → always allowed, files or not.
    assert!(legacy_todos_allowed(false, &none).is_ok());
    assert!(legacy_todos_allowed(false, &some).is_ok());

    // Enforced phase (verdict PASS) → allowed ONLY with zero legacy TODO files.
    assert!(legacy_todos_allowed(true, &none).is_ok());
    assert!(
        legacy_todos_allowed(true, &some).is_err(),
        "a legacy TODO file surviving a PASS cutover verdict must be blocked"
    );
}

#[test]
fn verdict_scan_is_whitespace_tolerant_and_pass_only() {
    assert!(verdict_text_is_pass("{\"verdict\":\"PASS\"}"));
    assert!(verdict_text_is_pass("{\n  \"verdict\": \"PASS\"\n}\n"));
    assert!(verdict_text_is_pass("{ \"verdict\" :\t\"PASS\" }"));
    assert!(!verdict_text_is_pass("{\"verdict\": \"HOLD\"}"));
    assert!(!verdict_text_is_pass("{\"verdict\": \"pass\"}"));
    assert!(!verdict_text_is_pass(""));
}

#[test]
fn legacy_todo_name_matcher_matches_the_guard_patterns() {
    assert!(is_legacy_todo_name("TODO.ATTENDED.md"));
    assert!(is_legacy_todo_name("TODO.OFFLINE.md"));
    assert!(is_legacy_todo_name("TODO-2026-07-28-B-mount-prechecks.md"));
    assert!(!is_legacy_todo_name("TODO.md"));
    assert!(!is_legacy_todo_name("TODO-notes.txt"));
    assert!(!is_legacy_todo_name("NOT-TODO-2026-01-01.md"));
}

/// The REAL-tree coupling check. NOT `#[ignore]`d (unlike merge_block.rs's tree
/// check): through Shadow it passes trivially, and after the U7 cutover it goes
/// red in `make adapter-check` / CI if any legacy TODO file is re-introduced.
#[test]
fn tree_respects_the_todo_cutover_coupling() {
    let root = repo_root();
    let verdict_pass = tree_verdict_is_pass(&root);
    let mut offenders = Vec::new();
    collect_legacy_todos(&root, &root, &mut offenders);
    offenders.sort();
    if let Err(reason) = legacy_todos_allowed(verdict_pass, &offenders) {
        panic!(
            "TODO-file merge-block violated: {reason} — either delete the legacy \
             files (their content belongs in queue/items.jsonl) or the cutover \
             verdict (queue/cutover-verdict.json) must not be PASS"
        );
    }
}

/// The script IS the gate check (`make todo-check`); its `--self-test` runs the
/// real script against mktemp fixture repos (real-recipe-shim pattern, no
/// re-implemented logic). Running it here keeps script and test coupled in the
/// same `make adapter-check` lane.
#[test]
fn todo_file_check_script_self_test_passes() {
    let root = repo_root();
    let script = root.join("scripts/todo-file-check.sh");
    assert!(
        script.is_file(),
        "scripts/todo-file-check.sh missing at {}",
        script.display()
    );
    let output = Command::new("bash")
        .arg(&script)
        .arg("--self-test")
        .current_dir(&root)
        .output()
        .expect("failed to spawn bash for todo-file-check.sh --self-test");
    assert!(
        output.status.success(),
        "todo-file-check.sh --self-test failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
