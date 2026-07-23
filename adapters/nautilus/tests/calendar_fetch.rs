//! U4: resumable bulk-fetch driver + checkpoint + path confinement. All offline through an
//! INJECTED fetch closure — no real endpoint is ever hit; pacing is a no-op.

use std::cell::Cell;
use std::path::Path;

use chrono::NaiveDate;
use nautilus_ls::calendar_refresh::{
    confine, fetch_inputs, DateRange, FetchConfig, MaintainerCredentials, RefreshInputs,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

const APPKEY: &str = "APPKEY_SECRET_abc123";
const SVCKEY: &str = "SVCKEY_SECRET_xyz789";

fn creds() -> MaintainerCredentials {
    MaintainerCredentials {
        krx_appkey: Some(APPKEY.to_string()),
        kasi_service_key: Some(SVCKEY.to_string()),
    }
}

fn cfg() -> FetchConfig {
    // 2026-06-15 (Mon) .. 2026-06-19 (Fri) — five weekdays, no weekends, one KASI year.
    FetchConfig {
        window: DateRange::new(d(2026, 6, 15), d(2026, 6, 19)),
        krx_through: d(2026, 6, 19),
        pace: std::time::Duration::from_millis(0),
    }
}

/// The KRX date requested in a `stk_bydd_trd` URL (`basDd=YYYYMMDD`).
fn krx_bas_dd(url: &str) -> &str {
    let tail = url.split("basDd=").nth(1).expect("krx url has basDd");
    &tail[..8]
}

/// A KRX witness body for `yyyymmdd`, or empty (a holiday/closed day) for 2026-06-17.
fn krx_body(yyyymmdd: &str) -> String {
    if yyyymmdd == "20260617" {
        r#"{"OutBlock_1":[]}"#.to_string()
    } else {
        format!(r#"{{"OutBlock_1":[{{"BAS_DD":"{yyyymmdd}","MKT_NM":"KOSPI"}}]}}"#)
    }
}

/// KASI 2026 with one holiday (2026-06-17).
fn kasi_body() -> String {
    r#"<response><header><resultCode>00</resultCode></header><body><items>
      <item><isHoliday>Y</isHoliday><locdate>20260617</locdate></item>
    </items><numOfRows>100</numOfRows><pageNo>1</pageNo><totalCount>1</totalCount></body></response>"#
        .to_string()
}

/// A fetch that succeeds for every unit — the uninterrupted baseline.
fn ok_fetch(url: &str) -> Result<String, String> {
    if url.contains("stk_bydd_trd") {
        Ok(krx_body(krx_bas_dd(url)))
    } else if url.contains("getRestDeInfo") {
        Ok(kasi_body())
    } else {
        Err(format!("unexpected url {url}"))
    }
}

#[test]
fn uninterrupted_run_produces_the_expected_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("fetch.ckpt");
    let inputs = fetch_inputs(&cfg(), &creds(), &state, ok_fetch, |_| {}).expect("fetch completes");

    // 4 witnesses (06-17 is empty) + 1 KASI holiday fact + 1 paired rule = 6 records.
    assert_eq!(inputs.evidence.len(), 6);
    let krx = inputs.outcomes.iter().find(|o| o.source_id == "krx-daily").unwrap();
    assert!(krx.is_ok());
    assert_eq!(krx.covered(), Some(&[DateRange::new(d(2026, 6, 15), d(2026, 6, 19))][..]));
    let kasi = inputs.outcomes.iter().find(|o| o.source_id == "kasi").unwrap();
    assert!(kasi.is_ok());
    // KASI coverage is clamped to the window end (we don't claim coverage past the window).
    assert_eq!(kasi.covered(), Some(&[DateRange::new(d(2026, 6, 15), d(2026, 6, 19))][..]));
}

#[test]
fn an_interrupted_run_resumes_and_reproduces_the_uninterrupted_artifact() {
    // Baseline: a single clean run.
    let base_dir = tempfile::tempdir().unwrap();
    let baseline: RefreshInputs =
        fetch_inputs(&cfg(), &creds(), &base_dir.path().join("s.ckpt"), ok_fetch, |_| {}).unwrap();

    // Interrupted: fail the 3rd KRX call (quota), then resume to completion against the same
    // checkpoint. The completed artifact must equal the uninterrupted one (KTD8 determinism).
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("s.ckpt");

    let krx_calls = Cell::new(0usize);
    let flaky = |url: &str| -> Result<String, String> {
        if url.contains("stk_bydd_trd") {
            krx_calls.set(krx_calls.get() + 1);
            if krx_calls.get() == 3 {
                return Err(format!("GET {url} timed out (quota)"));
            }
            Ok(krx_body(krx_bas_dd(url)))
        } else if url.contains("getRestDeInfo") {
            Ok(kasi_body())
        } else {
            Err("unexpected".to_string())
        }
    };
    let partial = fetch_inputs(&cfg(), &creds(), &state, flaky, |_| {}).expect("partial run returns");
    let krx = partial.outcomes.iter().find(|o| o.source_id == "krx-daily").unwrap();
    assert!(!krx.is_ok(), "the interrupted KRX source is honestly partial");

    // Resume with a clean fetch — continues from the checkpoint, not from scratch.
    let resumed = fetch_inputs(&cfg(), &creds(), &state, ok_fetch, |_| {}).expect("resume completes");
    assert_eq!(resumed, baseline, "the resumed artifact reproduces the uninterrupted run");
}

#[test]
fn a_quota_failure_leaves_an_honestly_partial_artifact_with_covered_ranges() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("s.ckpt");
    // Fail the 3rd KRX weekday (2026-06-17): dates 06-15, 06-16 completed → covered ends 06-16.
    let krx_calls = Cell::new(0usize);
    let flaky = |url: &str| -> Result<String, String> {
        if url.contains("stk_bydd_trd") {
            krx_calls.set(krx_calls.get() + 1);
            if krx_calls.get() == 3 {
                return Err(format!("GET {url} refused"));
            }
            Ok(krx_body(krx_bas_dd(url)))
        } else {
            Ok(kasi_body())
        }
    };
    let inputs = fetch_inputs(&cfg(), &creds(), &state, flaky, |_| {}).unwrap();
    let krx = inputs.outcomes.iter().find(|o| o.source_id == "krx-daily").unwrap();
    assert!(!krx.is_ok(), "partial KRX is Failed, not silently Ok");
    assert_eq!(
        krx.covered(),
        Some(&[DateRange::new(d(2026, 6, 15), d(2026, 6, 16))][..]),
        "covered range ends at the last completed date (AE9 producer side)"
    );
    assert!(krx.failure_reason().is_some(), "the failure reason is recorded");
}

#[test]
fn a_repeatedly_failing_source_surfaces_as_failed_not_deferred_forever() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("s.ckpt");
    // KRX always errors; the run must terminate (Failed), never loop.
    let always_fail = |url: &str| -> Result<String, String> {
        if url.contains("stk_bydd_trd") {
            Err("permanently down".to_string())
        } else {
            Ok(kasi_body())
        }
    };
    let inputs = fetch_inputs(&cfg(), &creds(), &state, always_fail, |_| {}).expect("terminates");
    let krx = inputs.outcomes.iter().find(|o| o.source_id == "krx-daily").unwrap();
    assert!(!krx.is_ok(), "a permanently-down source is Failed (liveness — no infinite retry)");
    // KASI still succeeds independently (separate quota).
    let kasi = inputs.outcomes.iter().find(|o| o.source_id == "kasi").unwrap();
    assert!(kasi.is_ok());
}

#[test]
fn no_credential_material_reaches_the_checkpoint_or_inputs_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("s.ckpt");
    let inputs_out = dir.path().join("inputs.json");

    // A failure whose message leaks the raw credential value — the scrub must remove it before
    // it can reach the checkpoint or the inputs artifact (KTD9 defense-in-depth).
    let krx_calls = Cell::new(0usize);
    let leaky = |url: &str| -> Result<String, String> {
        if url.contains("stk_bydd_trd") {
            krx_calls.set(krx_calls.get() + 1);
            if krx_calls.get() == 2 {
                return Err(format!("KRX call failed (AUTH_KEY={APPKEY})"));
            }
            Ok(krx_body(krx_bas_dd(url)))
        } else {
            Err(format!("KASI call failed (serviceKey={SVCKEY})"))
        }
    };
    let inputs = fetch_inputs(&cfg(), &creds(), &state, leaky, |_| {}).unwrap();
    std::fs::write(&inputs_out, serde_json::to_vec_pretty(&inputs).unwrap()).unwrap();

    for path in [&state, &inputs_out] {
        let bytes = std::fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(APPKEY), "the appkey must never persist in {}", path.display());
        assert!(!text.contains(SVCKEY), "the service key must never persist in {}", path.display());
    }
    // The failure reasons ARE recorded (credential-safe).
    assert!(inputs.outcomes.iter().any(|o| o.failure_reason().is_some()));
}

#[test]
fn missing_credentials_refuse_before_any_fetch() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("s.ckpt");
    let no_creds = MaintainerCredentials::default();
    let never = |_: &str| -> Result<String, String> { panic!("must not fetch without credentials") };
    let err = fetch_inputs(&cfg(), &no_creds, &state, never, |_| {}).unwrap_err();
    assert!(err.to_string().contains("credential"), "{err}");
}

#[test]
fn output_paths_are_confined_beneath_the_state_root() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();

    // A path directly under the root is accepted (returned canonicalized).
    let ok = confine(root.path(), &root.path().join("inputs.json")).expect("in-root path is confined");
    assert!(ok.starts_with(root.path().canonicalize().unwrap()));
    assert!(ok.ends_with("inputs.json"));

    // An absolute path outside the root is refused.
    assert!(confine(root.path(), &outside.path().join("escape.json")).is_err());

    // A `..` traversal that climbs out of the root is refused.
    assert!(confine(root.path(), &root.path().join("../escape.json")).is_err());

    // A symlink inside the root that points OUTSIDE is refused (parent canonicalizes out).
    let link = root.path().join("link");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    assert!(
        confine(root.path(), &link.join("escape.json")).is_err(),
        "a symlinked parent pointing outside the root is refused"
    );
}

// Sanity: the module compiles against `Path` (keeps the import used on all cfgs).
#[allow(dead_code)]
fn _uses_path(_p: &Path) {}
