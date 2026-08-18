use std::path::Path;

use serde_yaml::{Mapping, Value};

const WORKFLOW: &str = ".github/workflows/repository-engineering-check.yml";

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
}

#[test]
fn workflow_is_pull_request_only_read_only_and_immutable() {
    let text = std::fs::read_to_string(repository_root().join(WORKFLOW)).unwrap();
    let document: Value = serde_yaml::from_str(&text).unwrap();
    let root = document.as_mapping().unwrap();

    let triggers = mapping(root, "on");
    assert_eq!(triggers.len(), 1);
    assert!(triggers.contains_key(string("pull_request")));

    let permissions = mapping(root, "permissions");
    assert_eq!(permissions.len(), 1);
    assert_eq!(permissions.get(string("contents")), Some(&string("read")));

    let jobs = mapping(root, "jobs");
    let check = mapping(jobs, "check");
    assert_eq!(check.get(string("runs-on")), Some(&string("ubuntu-latest")));
    let steps = check.get(string("steps")).unwrap().as_sequence().unwrap();
    assert!(!steps.is_empty());
    for step in steps {
        let step = step.as_mapping().unwrap();
        if let Some(action) = step.get(string("uses")).and_then(Value::as_str) {
            let (_, revision) = action.rsplit_once('@').unwrap();
            assert_eq!(revision.len(), 40);
            assert!(revision.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }

    let checkout = steps[0].as_mapping().unwrap();
    assert_eq!(
        checkout.get(string("uses")),
        Some(&string(
            "actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683"
        ))
    );
    assert_eq!(
        mapping(checkout, "with").get(string("persist-credentials")),
        Some(&Value::Bool(false))
    );
    assert!(text.contains("cargo +1.96.0 test --locked"));
    assert!(text.contains("make repository-engineering-check"));
    assert!(text.contains("cargo +1.96.0 clippy --locked -p ls-repository-engineering"));
    assert!(text.contains(
        "cargo +1.96.0 clippy --locked --manifest-path tools/repository-engineering-runtime/Cargo.toml"
    ));
    assert!(text.contains(
        "rustup toolchain install 1.96.0 --profile minimal --component clippy"
    ));
    assert!(!text.contains("push:"));
    assert!(!text.contains("id-token"));
    assert!(!text.contains("secrets."));
    assert!(!text.contains("cache"));
    assert!(!text.contains(": write"));
}

fn mapping<'a>(mapping: &'a Mapping, key: &str) -> &'a Mapping {
    mapping.get(string(key)).unwrap().as_mapping().unwrap()
}

fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}
