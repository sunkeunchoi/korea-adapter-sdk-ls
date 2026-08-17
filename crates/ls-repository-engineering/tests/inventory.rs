use std::path::PathBuf;

use ls_repository_engineering::inventory::{
    discover_inventory, load_authored_package, reconcile_inventory,
};
use ls_repository_engineering::schema::{ActivationEligibility, AuthorityState};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn real_authored_package_is_inert_and_inventory_is_exact() {
    let root = repository_root();
    let authored = load_authored_package(&root).expect("authored package parses");
    assert_eq!(
        authored.package.activation_eligibility,
        ActivationEligibility::None
    );
    assert!(authored.package.active_capability_contracts.is_empty());
    assert!(authored.package.active_worker_roles.is_empty());

    let inventory =
        discover_inventory(&root, &authored.discovery_policy).expect("tracked tree is classified");
    assert!(inventory.unclassified_paths.is_empty());
    let findings = reconcile_inventory(&authored.ledger, &inventory);
    assert!(findings.is_empty(), "{findings:?}");
    assert!(authored
        .ledger
        .rows
        .iter()
        .all(|row| row.current_authority == AuthorityState::Legacy));
}

#[test]
fn research_snapshot_counts_remain_orientation_only() {
    let root = repository_root();
    let authored = load_authored_package(&root).unwrap();
    let inventory = discover_inventory(&root, &authored.discovery_policy).unwrap();
    assert_eq!(inventory.count_kind("capability"), 36);
    assert_eq!(inventory.count_kind("claude_alias"), 25);
    assert_eq!(inventory.count_kind("worker_role"), 2);
}

#[test]
fn omission_reports_a_stable_candidate_without_inventing_a_disposition() {
    let root = repository_root();
    let mut authored = load_authored_package(&root).unwrap();
    let inventory = discover_inventory(&root, &authored.discovery_policy).unwrap();
    let removed = authored.ledger.rows.remove(0);
    let findings = reconcile_inventory(&authored.ledger, &inventory);
    assert!(findings.iter().any(|finding| {
        finding.code == "inventory.ledger.missing"
            && finding.logical_id.as_deref() == Some(removed.logical_id.0.as_str())
    }));
}

#[test]
fn every_claude_alias_targets_the_same_named_legacy_capability() {
    let root = repository_root();
    let authored = load_authored_package(&root).unwrap();
    let inventory = discover_inventory(&root, &authored.discovery_policy).unwrap();
    for alias in inventory
        .obligations
        .iter()
        .filter(|obligation| obligation.source_kind.0 == "claude_alias")
    {
        let id = alias.logical_id.0.strip_prefix("claude-alias--").unwrap();
        assert_eq!(
            std::fs::read_link(root.join(&alias.source_locator.0)).unwrap(),
            PathBuf::from(format!("../../.agents/skills/{id}"))
        );
    }
}

#[cfg(unix)]
#[test]
fn authored_manifest_symlinks_fail_closed() {
    use std::os::unix::fs::symlink;

    let source_root = repository_root();
    let root = std::env::temp_dir().join(format!(
        "ls-repository-engineering-authored-symlink-{}",
        std::process::id()
    ));
    let package_root = root.join(".repository-engineering");
    std::fs::create_dir_all(&package_root).unwrap();
    for name in ["discovery-policy.toml", "migration-ledger.toml"] {
        std::fs::copy(
            source_root.join(".repository-engineering").join(name),
            package_root.join(name),
        )
        .unwrap();
    }
    let outside = root.join("outside-package.toml");
    std::fs::copy(
        source_root.join(".repository-engineering/package.toml"),
        &outside,
    )
    .unwrap();
    symlink(&outside, package_root.join("package.toml")).unwrap();

    let error = load_authored_package(&root).unwrap_err();
    assert_eq!(error.code, "authored.input_unsafe");
    std::fs::remove_dir_all(root).unwrap();
}
