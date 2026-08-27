//! `turn governed` orchestrator tests (U7, R6/R9; AE5 command side). The stage
//! machine is proven with stubbed build + child seams (`LS_GOVERNED_BUILD_CMD` /
//! `LS_GOVERNED_CHILD_BIN`); the real `cargo build` + real child path is exercised
//! attended, not by CI. Every governed run is the compiled bin as a subprocess so
//! env is isolated (no global `set_var` races).

#[path = "support/fingerprint_fixture.rs"]
mod fingerprint_fixture;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use nautilus_ls_lab::trials::TrialsLedger;
use nautilus_ls_lab::{fingerprint, runner::governed};
use tempfile::TempDir;

use fingerprint_fixture::FingerprintFixture;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lab-research"))
}

/// The real binary's embedded fingerprint (what a fresh, non-stale build reports).
fn real_fingerprint() -> String {
    let out = bin().arg("fingerprint").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .find_map(|l| l.trim().strip_prefix("fingerprint: ").map(str::to_string))
        .unwrap()
}

/// Write an executable `/bin/sh` stub child that branches on its first arg. Driven
/// by env inherited from the parent: `STUB_FP` (fingerprint line), `STUB_VERDICT`
/// (turn last line), `STUB_EXIT` (turn exit code), `STUB_STAGES` (space-separated
/// stage names appended to `LS_GOVERNED_STAGELOG`), `STUB_TRIAL` (a JSON trial
/// line appended to `LS_TRIALS_LEDGER`).
fn stub_child(dir: &Path) -> PathBuf {
    let path = dir.join("stub-child.sh");
    std::fs::write(
        &path,
        r#"#!/bin/sh
case "$1" in
  fingerprint) echo "fingerprint: $STUB_FP" ;;
  turn)
    if [ -n "$STUB_STAGES" ]; then
      for s in $STUB_STAGES; do echo "$s" >> "$LS_GOVERNED_STAGELOG"; done
    fi
    if [ -n "$STUB_TRIAL" ]; then
      printf '%s\n' "$STUB_TRIAL" >> "$LS_TRIALS_LEDGER"
    fi
    echo "$STUB_VERDICT"
    exit "${STUB_EXIT:-0}"
    ;;
esac
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

/// Write a candidate dir with just a gate-verdict of the given decision.
fn write_verdict(home: &Path, slug: &str, decision: &str, stop_gate: Option<&str>) -> PathBuf {
    let dir = home.join("candidates").join(slug);
    std::fs::create_dir_all(&dir).unwrap();
    let mut v = serde_json::json!({
        "schema_version": 1, "slug": slug, "family": "class-b", "decision": decision,
        "diagnostic_readings": {}, "twin_readings": {}, "agreed_readings": {},
        "pre_register_hash": "h", "catalog_fingerprint": "fp",
        "recorded_utc": "2026-07-16T00:00:00+00:00"
    });
    if let Some(gate) = stop_gate {
        v["stop_gate"] = serde_json::json!(gate);
    }
    std::fs::write(
        dir.join("gate-verdict.json"),
        serde_json::to_string_pretty(&v).unwrap(),
    )
    .unwrap();
    home.join("candidates")
}

#[test]
fn production_anchor_ignores_removed_path_overrides() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_verdict(home, "c", "GO", None);

    let out = bin()
        .args(["turn", "governed"])
        .env("LS_CANDIDATES_HOME", home.join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env("LS_GOVERNED_SRC_DIR", home.join("redirected-src"))
        .env("LS_GOVERNED_CARGO_TOML", home.join("redirected-Cargo.toml"))
        .env("LS_GOVERNED_BUILD_CMD", "true")
        .env("LS_GOVERNED_CHILD_BIN", stub_child(home))
        .env("STUB_FP", real_fingerprint())
        .env("STUB_VERDICT", "REVERT ror-negative")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("parent fingerprint OK"));
}

#[test]
fn a_reused_stop_verdict_short_circuits_before_the_build() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_verdict(home, "c", "STOP", Some("twin-mismatch"));
    let marker = home.join("built.marker");

    let out = bin()
        .args(["turn", "governed"])
        .env("LS_CANDIDATES_HOME", home.join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env(
            "LS_GOVERNED_BUILD_CMD",
            format!("touch {}", marker.display()),
        )
        .env("LS_GOVERNED_CHILD_BIN", stub_child(home))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(10),
        "TwinMismatch exit (STOP short-circuit)"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.lines().last().unwrap().starts_with("STOP"),
        "{stdout}"
    );
    assert!(
        !marker.exists(),
        "a STOP verdict short-circuits before the build"
    );
}

#[test]
fn a_build_failure_halts() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_verdict(home, "c", "GO", None);
    let out = bin()
        .args(["turn", "governed"])
        .env("LS_CANDIDATES_HOME", home.join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env("LS_GOVERNED_BUILD_CMD", "false")
        .env("LS_GOVERNED_CHILD_BIN", stub_child(home))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(31), "BuildFailure exit");
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .last()
        .unwrap()
        .starts_with("HELD"));
}

#[test]
fn a_built_binary_whose_fingerprint_mismatches_halts_before_the_flip() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_verdict(home, "c", "GO", None);
    let ledger = home.join("trials/trials.jsonl");

    let out = bin()
        .args(["turn", "governed"])
        .env("LS_CANDIDATES_HOME", home.join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env("LS_TRIALS_LEDGER", &ledger)
        .env("LS_GOVERNED_BUILD_CMD", "true")
        .env("LS_GOVERNED_CHILD_BIN", stub_child(home))
        .env("STUB_FP", "wrong_fingerprint") // != the recomputed tree hash
        .env("STUB_TRIAL", "{\"should\":\"never-run\"}")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(30),
        "StaleBinary on a built-binary fingerprint mismatch"
    );
    assert!(!ledger.exists(), "the flip never ran → nothing appended");
}

#[test]
fn a_child_flip_refusal_surfaces_as_held_with_the_childs_code() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_verdict(home, "c", "GO", None);
    let out = bin()
        .args(["turn", "governed"])
        .env("LS_CANDIDATES_HOME", home.join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env("LS_GOVERNED_BUILD_CMD", "true")
        .env("LS_GOVERNED_CHILD_BIN", stub_child(home))
        .env("STUB_FP", real_fingerprint())
        .env("STUB_VERDICT", "HELD pre-register edited")
        .env("STUB_EXIT", "21") // PreRegisterHashMismatch
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(21),
        "the child's typed gate code is preserved"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.lines().last().unwrap().starts_with("HELD"),
        "{stdout}"
    );
}

#[test]
fn ae5_a_completed_flip_adopts_the_childs_verdict_and_exits_zero() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_verdict(home, "c", "GO", None);
    for (verdict, shape) in [
        ("KEEP v9 deadbeef", "KEEP"),
        ("REVERT ror-negative", "REVERT"),
    ] {
        let out = bin()
            .args(["turn", "governed"])
            .env("LS_CANDIDATES_HOME", home.join("candidates"))
            .env("LS_TURN_CANDIDATE", "c")
            .env("LS_GOVERNED_BUILD_CMD", "true")
            .env("LS_GOVERNED_CHILD_BIN", stub_child(home))
            .env("STUB_FP", real_fingerprint())
            .env("STUB_VERDICT", verdict)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "a completed {shape} evaluation exits 0"
        );
        let last = String::from_utf8_lossy(&out.stdout)
            .lines()
            .last()
            .unwrap()
            .to_string();
        assert!(
            last.starts_with(shape),
            "last line is the {shape} verdict: {last}"
        );
    }
}

#[test]
fn exactly_one_flip_trial_lands_and_the_parent_appends_nothing() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_verdict(home, "c", "GO", None);
    let ledger_path = home.join("trials/trials.jsonl");
    std::fs::create_dir_all(ledger_path.parent().unwrap()).unwrap();
    // The child (stub) writes exactly one flip trial line.
    let trial = r#"{"schema_version":1,"recorded_utc":"2026-07-16T00:00:00+00:00","candidate":"c","family":"class-b","look":"flip","lineage":{"catalog_fingerprint":"fp"},"readings":{},"verdict":"KEEP v9"}"#;

    let out = bin()
        .args(["turn", "governed"])
        .env("LS_CANDIDATES_HOME", home.join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env("LS_TRIALS_LEDGER", &ledger_path)
        .env("LS_GOVERNED_BUILD_CMD", "true")
        .env("LS_GOVERNED_CHILD_BIN", stub_child(home))
        .env("STUB_FP", real_fingerprint())
        .env("STUB_VERDICT", "KEEP v9 deadbeef")
        .env("STUB_TRIAL", trial)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    // Exactly one flip trial — the child wrote it, the parent adds nothing.
    let records = TrialsLedger::new(&ledger_path).read_all().unwrap();
    assert_eq!(
        records.len(),
        1,
        "exactly one flip trial per governed run: {records:?}"
    );
    assert!(matches!(
        records[0].look,
        nautilus_ls_lab::trials::LookKind::Flip
    ));
}

#[test]
fn a_code_turn_orders_bump_rebaseline_reconcile_compare() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_verdict(home, "c", "GO", None);
    let stagelog = home.join("stages.log");

    let out = bin()
        .args(["turn", "governed"])
        .env("LS_CANDIDATES_HOME", home.join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env("LS_TURN_CODE_BUMP", "1")
        .env("LS_GOVERNED_STAGELOG", &stagelog)
        .env("LS_GOVERNED_BUILD_CMD", "true")
        .env("LS_GOVERNED_CHILD_BIN", stub_child(home))
        .env("STUB_FP", real_fingerprint())
        .env("STUB_VERDICT", "KEEP v9 deadbeef")
        .env("STUB_STAGES", "bump rebaseline reconcile compare")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stages: Vec<String> = std::fs::read_to_string(&stagelog)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(
        stages,
        ["bump", "rebaseline", "reconcile", "compare"],
        "code-turn stage order"
    );
}

#[test]
fn a_build_mutation_halts_before_reporter_or_decider() {
    let fixture = FingerprintFixture::new();
    let approved = fingerprint::recompute_from_root(fixture.root()).unwrap();
    let state = TempDir::new().unwrap();
    write_verdict(state.path(), "c", "GO", None);
    let reporter_marker = state.path().join("reporter-called");
    let child = state.path().join("reporter.sh");
    write_executable(
        &child,
        &format!(
            "#!/bin/sh\ntouch {}\necho 'fingerprint: {approved}'\n",
            reporter_marker.display()
        ),
    );
    let build = state.path().join("mutate-build.sh");
    write_executable(
        &build,
        &format!(
            "#!/bin/sh\nprintf mutation >> {}\n",
            fixture.path("crates/ls-sdk/src/lib.rs").display()
        ),
    );

    let out = fixture_parent_command(fixture.root(), &approved)
        .env("LS_CANDIDATES_HOME", state.path().join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env("LS_GOVERNED_BUILD_CMD", &build)
        .env("LS_GOVERNED_CHILD_BIN", &child)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(30));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("changed during the foreground build"),
        "{stdout}"
    );
    assert!(
        !reporter_marker.exists(),
        "post-build mismatch stops before reporter"
    );
}

#[test]
fn a_post_report_mutation_is_caught_before_decider_side_effects() {
    let fixture = FingerprintFixture::new();
    let approved = fingerprint::recompute_from_root(fixture.root()).unwrap();
    let state = TempDir::new().unwrap();
    write_verdict(state.path(), "c", "GO", None);
    let stage_log = state.path().join("stages.log");
    let ledger = state.path().join("trials.jsonl");
    let child = state.path().join("mutating-child.sh");
    write_executable(
        &child,
        &format!(
            r#"#!/bin/sh
case "$1" in
  fingerprint) echo "fingerprint: {approved}" ;;
  turn)
    printf mutation >> "{}"
    FINGERPRINT_HELPER_MODE=decider exec "$FINGERPRINT_HELPER_BIN" --exact fingerprint_process_helper --nocapture
    ;;
esac
"#,
            fixture.path("crates/ls-core/src/lib.rs").display()
        ),
    );

    let out = fixture_parent_command(fixture.root(), &approved)
        .env("LS_CANDIDATES_HOME", state.path().join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env("LS_GOVERNED_BUILD_CMD", "true")
        .env("LS_GOVERNED_CHILD_BIN", &child)
        .env("LS_GOVERNED_STAGELOG", &stage_log)
        .env("LS_TRIALS_LEDGER", &ledger)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(30));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("HELD decider fingerprint")),
        "{stdout}"
    );
    assert!(
        !stage_log.exists(),
        "decider validates before stage logging"
    );
    assert!(
        !ledger.exists(),
        "decider validates before strategy trial effects"
    );
}

#[test]
fn governed_build_ignores_a_root_workspace_cwd_override() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_verdict(home, "c", "GO", None);
    let capture = home.join("build-cwd");
    let build = home.join("capture-cwd.sh");
    write_executable(&build, "#!/bin/sh\npwd > \"$BUILD_CWD_CAPTURE\"\n");
    let root_workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();

    let out = bin()
        .args(["turn", "governed"])
        .env("LS_CANDIDATES_HOME", home.join("candidates"))
        .env("LS_TURN_CANDIDATE", "c")
        .env("LS_GOVERNED_BUILD_CMD", &build)
        .env("BUILD_CWD_CAPTURE", &capture)
        .env("LS_GOVERNED_BUILD_CWD", root_workspace)
        .env("LS_GOVERNED_CHILD_BIN", stub_child(home))
        .env("STUB_FP", real_fingerprint())
        .env("STUB_VERDICT", "REVERT ror-negative")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let actual = std::fs::read_to_string(capture).unwrap();
    assert_eq!(
        Path::new(actual.trim()),
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
    );
}

#[test]
fn an_invalid_declared_input_holds_before_governed_side_effects() {
    let fixture = FingerprintFixture::new();
    let approved = fingerprint::recompute_from_root(fixture.root()).unwrap();
    std::fs::remove_file(fixture.path("crates/ls-core/Cargo.toml")).unwrap();
    let state = TempDir::new().unwrap();
    let build_marker = state.path().join("build-called");
    let out = fixture_parent_command(fixture.root(), &approved)
        .env(
            "LS_GOVERNED_BUILD_CMD",
            format!("touch {}", build_marker.display()),
        )
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(30));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("inventory is not trustworthy"), "{stdout}");
    assert!(
        !build_marker.exists(),
        "invalid inventory stops before build"
    );
}

#[test]
fn stale_parent_mutations_hold_before_diagnosis_or_build() {
    for relative in [
        "crates/ls-sdk/src/lib.rs",
        "adapters/nautilus/src/lib.rs",
        "adapters/nautilus/Cargo.lock",
        "metadata/error-catalog.yaml",
    ] {
        let fixture = FingerprintFixture::new();
        let embedded = fingerprint::recompute_from_root(fixture.root()).unwrap();
        fixture.append(relative, b"x");
        let state = TempDir::new().unwrap();
        let build_marker = state.path().join("build-called");

        let out = fixture_parent_command(fixture.root(), &embedded)
            .env(
                "LS_GOVERNED_BUILD_CMD",
                format!("touch {}", build_marker.display()),
            )
            .output()
            .unwrap();

        assert_eq!(out.status.code(), Some(30), "stale class {relative}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("parent binary is stale"),
            "{relative}: {stdout}"
        );
        assert!(!build_marker.exists(), "{relative} stops before build");
    }
}

#[test]
fn fingerprint_process_helper() {
    let Ok(mode) = std::env::var("FINGERPRINT_HELPER_MODE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("FINGERPRINT_HELPER_ROOT").unwrap());
    let embedded = std::env::var("FINGERPRINT_HELPER_EMBEDDED").unwrap();
    let exit = match mode.as_str() {
        "parent" => {
            let outcome = governed::run_governed_with_fingerprint_root(&root, &embedded).unwrap();
            for line in outcome.lines {
                println!("{line}");
            }
            outcome.exit.code() as i32
        }
        "decider" => {
            let code =
                governed::run_governed_child_with_fingerprint_root(&root, &embedded).unwrap();
            if code == nautilus_ls_lab::runner::diagnose::GateExit::StaleBinary.exit_code() {
                30
            } else if code == std::process::ExitCode::SUCCESS {
                0
            } else {
                1
            }
        }
        other => panic!("unknown fingerprint helper mode {other}"),
    };
    std::process::exit(exit);
}

fn fixture_parent_command(root: &Path, embedded: &str) -> Command {
    let current = std::env::current_exe().unwrap();
    let mut command = Command::new(&current);
    command
        .args(["--exact", "fingerprint_process_helper", "--nocapture"])
        .env("FINGERPRINT_HELPER_MODE", "parent")
        .env("FINGERPRINT_HELPER_ROOT", root)
        .env("FINGERPRINT_HELPER_EMBEDDED", embedded)
        .env("FINGERPRINT_HELPER_BIN", current);
    command
}

fn write_executable(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}
