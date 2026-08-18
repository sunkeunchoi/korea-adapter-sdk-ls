use std::path::{Path, PathBuf};
use std::process::Command;

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
    assert_eq!(authored.package.declared_capability_contracts.len(), 1);
    assert_eq!(authored.package.declared_worker_roles.len(), 1);
    assert_eq!(authored.capability_contracts.len(), 1);
    assert_eq!(authored.worker_role_contracts.len(), 1);
    assert_eq!(
        authored.capability_contracts[0].capability_id.0,
        "audit-carried-rows"
    );
    assert_eq!(
        authored.worker_role_contracts[0].role_id.0,
        "decommission-row-auditor"
    );

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
fn planned_and_unported_rows_follow_the_closed_pre_parity_state_table() {
    let root = repository_root();
    let authored = load_authored_package(&root).unwrap();
    let inventory = discover_inventory(&root, &authored.discovery_policy).unwrap();

    let mut missing_replacement = authored.ledger.clone();
    missing_replacement
        .rows
        .iter_mut()
        .find(|row| row.logical_id.0 == "capability--audit-carried-rows")
        .unwrap()
        .replacement_contract = None;
    assert!(reconcile_inventory(&missing_replacement, &inventory)
        .iter()
        .any(|finding| finding.code == "inventory.planned_state.invalid"));

    let mut wrong_absence = authored.ledger.clone();
    wrong_absence
        .rows
        .iter_mut()
        .find(|row| row.logical_id.0 == "capability--audit-carried-rows")
        .unwrap()
        .absence_reason = Some(ls_repository_engineering::schema::StableId(
        "successor_not_implemented".to_owned(),
    ));
    assert!(reconcile_inventory(&wrong_absence, &inventory)
        .iter()
        .any(|finding| finding.code == "inventory.planned_state.invalid"));

    let mut false_parity = authored.ledger.clone();
    false_parity
        .rows
        .iter_mut()
        .find(|row| row.logical_id.0 == "capability--audit-carried-rows")
        .unwrap()
        .parity_reference = Some(authored.capability_contracts[0].knowledge_references[0].clone());
    assert!(reconcile_inventory(&false_parity, &inventory)
        .iter()
        .any(|finding| finding.code == "inventory.planned_state.invalid"));

    let mut false_unported_successor = authored.ledger.clone();
    let audit_row = false_unported_successor
        .rows
        .iter_mut()
        .find(|row| row.logical_id.0 == "capability--audit-row")
        .unwrap();
    audit_row.replacement_contract = Some(ls_repository_engineering::schema::StableId(
        "audit-row".to_owned(),
    ));
    assert!(reconcile_inventory(&false_unported_successor, &inventory)
        .iter()
        .any(|finding| finding.code == "inventory.unported_state.invalid"));
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

#[test]
fn registered_contract_inventory_and_digests_fail_closed() {
    let root = loader_fixture();
    load_authored_package(&root).expect("fixture starts valid");

    let orphan =
        root.join(".repository-engineering/contracts/capabilities/unregistered-contract.toml");
    std::fs::write(&orphan, "schema_version = \"v0\"\n").unwrap();
    refresh_fixture_index(&root);
    assert_eq!(
        load_authored_package(&root).unwrap_err().code,
        "authored.contract_inventory_mismatch"
    );
    std::fs::remove_file(&orphan).unwrap();

    let capability =
        root.join(".repository-engineering/contracts/capabilities/audit-carried-rows.toml");
    let worker =
        root.join(".repository-engineering/contracts/workers/decommission-row-auditor.toml");
    std::fs::copy(&worker, &capability).unwrap();
    refresh_fixture_index(&root);
    assert_eq!(
        load_authored_package(&root).unwrap_err().code,
        "authored.contract_kind_mismatch"
    );
    copy_fixture_path(
        &repository_root(),
        &root,
        ".repository-engineering/contracts/capabilities/audit-carried-rows.toml",
    );

    let knowledge = root.join(".agents/skills/audit-row/SKILL.md");
    std::fs::write(&knowledge, "changed tracked knowledge\n").unwrap();
    refresh_fixture_index(&root);
    assert_eq!(
        load_authored_package(&root).unwrap_err().code,
        "authored.artifact_digest_mismatch"
    );
    copy_fixture_path(
        &repository_root(),
        &root,
        ".agents/skills/audit-row/SKILL.md",
    );

    let evidence = root.join("docs/migration-source/audit/records/L1.yaml");
    std::fs::write(&evidence, "changed tracked evidence\n").unwrap();
    refresh_fixture_index(&root);
    assert_eq!(
        load_authored_package(&root).unwrap_err().code,
        "authored.artifact_set_digest_mismatch"
    );
    copy_fixture_path(
        &repository_root(),
        &root,
        "docs/migration-source/audit/records/L1.yaml",
    );

    std::fs::write(&capability, vec![b'x'; 2 * 1024 * 1024 + 1]).unwrap();
    refresh_fixture_index(&root);
    assert_eq!(
        load_authored_package(&root).unwrap_err().code,
        "authored.file_too_large"
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn contract_knowledge_and_evidence_symlinks_fail_without_following_external_bytes() {
    use std::os::unix::fs::symlink;

    for relative in [
        ".repository-engineering/contracts/capabilities/audit-carried-rows.toml",
        ".agents/skills/audit-row/SKILL.md",
        "docs/migration-source/audit/records/L1.yaml",
    ] {
        let root = loader_fixture();
        let outside = root.join("outside-bytes");
        std::fs::write(&outside, "must not be parsed or hashed\n").unwrap();
        let target = root.join(relative);
        std::fs::remove_file(&target).unwrap();
        symlink(&outside, &target).unwrap();
        refresh_fixture_index(&root);
        assert_eq!(
            load_authored_package(&root).unwrap_err().code,
            "authored.tracked_file_unsafe",
            "{relative}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    let root = loader_fixture();
    let outside = root.join("outside-parent");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("SKILL.md"), "must not be read\n").unwrap();
    let parent = root.join(".agents/skills/audit-row");
    std::fs::remove_dir_all(&parent).unwrap();
    symlink(&outside, &parent).unwrap();
    refresh_fixture_index(&root);
    assert_eq!(
        load_authored_package(&root).unwrap_err().code,
        "authored.tracked_file_missing"
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn loader_fixture() -> PathBuf {
    let source = repository_root();
    let root = std::env::temp_dir().join(format!(
        "ls-repository-engineering-loader-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    for relative in [
        ".repository-engineering/package.toml",
        ".repository-engineering/discovery-policy.toml",
        ".repository-engineering/migration-ledger.toml",
        ".repository-engineering/contracts/capabilities/audit-carried-rows.toml",
        ".repository-engineering/contracts/workers/decommission-row-auditor.toml",
        ".agents/skills/audit-carried-rows/SKILL.md",
        ".agents/skills/audit-row/SKILL.md",
        ".agents/skills/audit-carried-rows/references/record-format.md",
        ".claude/agents/decommission-row-auditor.md",
        "docs/migration-source/audit/manifest.yaml",
        "docs/migration-source/audit/decommission-audit-report.md",
        "crates/ls-trackers/tests/decommission_audit.rs",
    ] {
        copy_fixture_path(&source, &root, relative);
    }
    for entry in std::fs::read_dir(source.join("docs/migration-source/audit/records")).unwrap() {
        let entry = entry.unwrap();
        copy_fixture_path(
            &source,
            &root,
            &format!(
                "docs/migration-source/audit/records/{}",
                entry.file_name().to_string_lossy()
            ),
        );
    }
    assert!(Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .unwrap()
        .success());
    refresh_fixture_index(&root);
    root
}

fn copy_fixture_path(source: &Path, root: &Path, relative: &str) {
    let target = root.join(relative);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::copy(source.join(relative), target).unwrap();
}

fn refresh_fixture_index(root: &Path) {
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .status()
        .unwrap()
        .success());
}
