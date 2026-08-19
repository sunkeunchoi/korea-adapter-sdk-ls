#[path = "support/fingerprint_fixture.rs"]
mod fingerprint_fixture;

use std::collections::BTreeSet;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use fingerprint_fixture::FingerprintFixture;
use nautilus_ls_lab::fingerprint::{
    compute_from_inventory, declared_inventory, recompute, recompute_from_root,
    watch_paths_from_root, FingerprintInput,
};

#[test]
fn production_recomputation_equals_the_embedded_digest() {
    assert_eq!(recompute().unwrap(), nautilus_ls_lab::fingerprint::EMBEDDED);
}

#[test]
fn every_declared_input_class_moves_the_digest() {
    let cases = [
        "adapters/nautilus/lab/src/lib.rs",
        "adapters/nautilus/lab/Cargo.toml",
        "adapters/nautilus/lab/build.rs",
        "adapters/nautilus/lab/fingerprint_core.rs",
        "Cargo.toml",
        "crates/ls-sdk/src/lib.rs",
        "crates/ls-sdk/Cargo.toml",
        "crates/ls-core/src/lib.rs",
        "crates/ls-core/Cargo.toml",
        "crates/ls-core/build.rs",
        "metadata/error-catalog.yaml",
        "metadata/constraints/t1101.yaml",
        "adapters/nautilus/Cargo.toml",
        "adapters/nautilus/Cargo.lock",
        "adapters/nautilus/rust-toolchain.toml",
    ];

    for relative in cases {
        let fixture = FingerprintFixture::new();
        let unchanged = recompute_from_root(fixture.root()).unwrap();
        fixture.append(relative, b"x");
        let changed = recompute_from_root(fixture.root()).unwrap();
        assert_ne!(
            changed, unchanged,
            "one-byte-class mutation must move {relative}"
        );
    }
}

#[test]
fn membership_changes_move_each_declared_tree() {
    for tree in [
        "adapters/nautilus/lab/src",
        "crates/ls-sdk/src",
        "crates/ls-core/src",
        "metadata/constraints",
    ] {
        let fixture = FingerprintFixture::new();
        let unchanged = recompute_from_root(fixture.root()).unwrap();
        let added = fixture.path(&format!("{tree}/added.rs"));
        std::fs::write(&added, "added\n").unwrap();
        let after_add = recompute_from_root(fixture.root()).unwrap();
        assert_ne!(after_add, unchanged, "adding a member must move {tree}");

        let renamed = fixture.path(&format!("{tree}/renamed.rs"));
        std::fs::rename(&added, &renamed).unwrap();
        let after_rename = recompute_from_root(fixture.root()).unwrap();
        assert_ne!(
            after_rename, after_add,
            "renaming a member must move {tree}"
        );

        std::fs::remove_file(&renamed).unwrap();
        assert_eq!(recompute_from_root(fixture.root()).unwrap(), unchanged);
    }
}

#[test]
fn digest_is_declaration_order_and_checkout_location_independent() {
    let first = FingerprintFixture::new();
    let second = FingerprintFixture::new();
    let expected = recompute_from_root(first.root()).unwrap();
    assert_eq!(recompute_from_root(second.root()).unwrap(), expected);

    let mut reversed = declared_inventory();
    reversed.reverse();
    assert_eq!(
        compute_from_inventory(first.root(), &reversed).unwrap(),
        expected
    );
}

#[test]
fn excluded_inputs_are_negative_controls() {
    for relative in [
        "adapters/nautilus/src/lib.rs",
        "adapters/nautilus/nautilus-ls-calendar/src/lib.rs",
        "adapters/nautilus/nautilus-ls-calendar/Cargo.toml",
        "Cargo.lock",
        "target/debug/generated.rs",
        "crates/ls-sdk-test-support/src/lib.rs",
    ] {
        let fixture = FingerprintFixture::new();
        let unchanged = recompute_from_root(fixture.root()).unwrap();
        fixture.append(relative, b"excluded mutation");
        assert_eq!(
            recompute_from_root(fixture.root()).unwrap(),
            unchanged,
            "declared residual must stay outside this prerequisite: {relative}"
        );
    }
}

#[test]
fn inventory_rejects_duplicate_and_overlapping_declarations() {
    let fixture = FingerprintFixture::new();
    let cases = [
        vec![
            FingerprintInput::file("same", "Cargo.toml"),
            FingerprintInput::file("same", "adapters/nautilus/Cargo.toml"),
        ],
        vec![
            FingerprintInput::file("one", "Cargo.toml"),
            FingerprintInput::file("two", "./Cargo.toml"),
        ],
        vec![
            FingerprintInput::tree("sdk", "crates/ls-sdk"),
            FingerprintInput::file("sdk-manifest", "crates/ls-sdk/Cargo.toml"),
        ],
    ];
    for inventory in cases {
        assert!(compute_from_inventory(fixture.root(), &inventory).is_err());
    }
}

#[test]
fn inventory_fails_closed_for_missing_wrong_type_and_symlink_inputs() {
    let fixture = FingerprintFixture::new();
    assert!(compute_from_inventory(
        fixture.root(),
        &[FingerprintInput::file("missing", "missing.txt")]
    )
    .is_err());
    assert!(compute_from_inventory(
        fixture.root(),
        &[FingerprintInput::tree("wrong-tree", "Cargo.toml")]
    )
    .is_err());
    assert!(compute_from_inventory(
        fixture.root(),
        &[FingerprintInput::file("wrong-file", "metadata/constraints")]
    )
    .is_err());

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(fixture.path("Cargo.toml"), fixture.path("linked")).unwrap();
        assert!(compute_from_inventory(
            fixture.root(),
            &[FingerprintInput::file("linked", "linked")]
        )
        .is_err());

        let unreadable = fixture.path("unreadable");
        std::fs::write(&unreadable, "private").unwrap();
        let mut permissions = std::fs::metadata(&unreadable).unwrap().permissions();
        permissions.set_mode(0o000);
        std::fs::set_permissions(&unreadable, permissions).unwrap();
        let unreadable_result = compute_from_inventory(
            fixture.root(),
            &[FingerprintInput::file("unreadable", "unreadable")],
        );
        let mut permissions = std::fs::metadata(&unreadable).unwrap().permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(&unreadable, permissions).unwrap();
        assert!(unreadable_result.is_err());

        let socket_path = fixture.path("special.sock");
        let _socket = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        assert!(compute_from_inventory(
            fixture.root(),
            &[FingerprintInput::file("special", "special.sock")]
        )
        .is_err());
    }
}

#[test]
fn watch_projection_is_exactly_the_declared_inventory() {
    let fixture = FingerprintFixture::new();
    let actual: BTreeSet<_> = watch_paths_from_root(fixture.root())
        .unwrap()
        .into_iter()
        .collect();
    let expected: BTreeSet<_> = declared_inventory()
        .into_iter()
        .map(|input| fixture.root().join(input.relative_path()))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn built_lab_watch_evidence_matches_the_declared_inventory() {
    let repo_root = nautilus_ls_lab::fingerprint::compiled_repo_root();
    let marker = format!(
        "cargo:rustc-env=LAB_SRC_FINGERPRINT={}",
        nautilus_ls_lab::fingerprint::EMBEDDED
    );
    let output = find_build_output_with_line("nautilus-ls-lab", &marker);
    let actual: BTreeSet<_> = rerun_paths(&output, &repo_root.join("adapters/nautilus/lab"));
    let expected: BTreeSet<_> = watch_paths_from_root(&repo_root)
        .unwrap()
        .into_iter()
        .map(|path| path.canonicalize().unwrap())
        .collect();
    assert_eq!(
        actual, expected,
        "the build script must project only the shared inventory"
    );
}

#[test]
fn cargo_dependency_evidence_has_no_undeclared_repository_input() {
    let repo_root = nautilus_ls_lab::fingerprint::compiled_repo_root();
    let evidence = lab_dependency_evidence();
    let selected_sources = repository_dependency_paths(&evidence, &repo_root);
    assert!(
        selected_sources
            .iter()
            .any(|path| path.starts_with("crates/ls-sdk/src")),
        "Cargo dep-info must expose root SDK sources"
    );
    assert!(
        selected_sources
            .iter()
            .any(|path| path.starts_with("crates/ls-core/src")),
        "Cargo dep-info must expose root core sources"
    );
    let uncovered = uncovered_repository_dependencies(&selected_sources);
    assert!(
        uncovered.is_empty(),
        "Cargo compiled inputs outside the declared or explicitly deferred boundary: {uncovered:?}"
    );
}

#[test]
fn ls_core_build_watch_evidence_has_no_undeclared_repository_input() {
    let repo_root = nautilus_ls_lab::fingerprint::compiled_repo_root();
    let output = current_ls_core_build_output(&lab_dependency_evidence());
    let watched = rerun_paths(&output, &repo_root.join("crates/ls-core"));
    assert!(
        !watched.is_empty(),
        "ls-core build output must expose its rebuild inputs"
    );
    let uncovered = uncovered_repository_watches(&watched, &repo_root).unwrap();
    assert!(
        uncovered.is_empty(),
        "ls-core embeds undeclared repository inputs: {uncovered:?}"
    );
}

#[test]
fn synthetic_dependency_evidence_exposes_an_undeclared_root_source() {
    let fixture = FingerprintFixture::new();
    let undeclared = fixture.path("crates/new/src/lib.rs");
    std::fs::create_dir_all(undeclared.parent().unwrap()).unwrap();
    std::fs::write(&undeclared, "pub fn newly_compiled() {}\n").unwrap();
    let evidence = format!(
        "{}: {} {} {}\n",
        fixture
            .path("adapters/nautilus/target/debug/lab-research")
            .display(),
        fixture.path("crates/ls-sdk/src/lib.rs").display(),
        fixture.path("crates/ls-core/src/lib.rs").display(),
        undeclared.display()
    );
    let paths = repository_dependency_paths(&evidence, fixture.root());
    let uncovered = uncovered_repository_dependencies(&paths);
    assert_eq!(
        uncovered,
        BTreeSet::from([PathBuf::from("crates/new/src/lib.rs")])
    );
}

#[test]
fn synthetic_build_script_watch_evidence_exposes_an_undeclared_data_input() {
    let fixture = FingerprintFixture::new();
    std::fs::write(fixture.path("metadata/new-input.json"), "{}\n").unwrap();
    let output = concat!(
        "cargo:rerun-if-changed=../../metadata/error-catalog.yaml\n",
        "cargo:rerun-if-changed=../../metadata/constraints/t1101.yaml\n",
        "cargo:rerun-if-changed=../../metadata/new-input.json\n",
    );
    let watched = rerun_paths(output, &fixture.path("crates/ls-core"));
    let uncovered = uncovered_repository_watches(&watched, fixture.root()).unwrap();
    assert_eq!(
        uncovered,
        BTreeSet::from([PathBuf::from("metadata/new-input.json")])
    );
}

fn lab_dependency_evidence() -> String {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_lab-research"));
    let dependency_file = binary.with_extension("d");
    std::fs::read_to_string(&dependency_file)
        .unwrap_or_else(|error| panic!("read {}: {error}", dependency_file.display()))
}

fn repository_dependency_paths(evidence: &str, repo_root: &Path) -> BTreeSet<PathBuf> {
    evidence
        .replace("\\\n", " ")
        .split_whitespace()
        .filter_map(|token| {
            repository_relative_path(Path::new(token.trim_end_matches(':')), repo_root)
        })
        .collect()
}

fn uncovered_repository_dependencies(paths: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
    let inventory = declared_inventory();
    paths
        .iter()
        .filter(|path| {
            !inventory.iter().any(|input| input.covers(path))
                && !is_explicitly_deferred_dependency(path)
        })
        .cloned()
        .collect()
}

fn is_explicitly_deferred_dependency(path: &Path) -> bool {
    [
        Path::new("adapters/nautilus/src"),
        Path::new("adapters/nautilus/nautilus-ls-calendar"),
        Path::new("adapters/nautilus/target"),
    ]
    .iter()
    .any(|root| path.starts_with(root))
}

fn repository_relative_path(path: &Path, repo_root: &Path) -> Option<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    };
    if let Ok(canonical) = absolute.canonicalize() {
        let canonical_root = repo_root.canonicalize().ok()?;
        return canonical
            .strip_prefix(canonical_root)
            .ok()
            .map(Path::to_path_buf);
    }

    let relative = absolute.strip_prefix(repo_root).ok()?;
    let mut normalized = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(normalized)
}

fn current_ls_core_build_output(dependency_evidence: &str) -> String {
    let mut outputs = dependency_evidence
        .replace("\\\n", " ")
        .split_whitespace()
        .map(|token| PathBuf::from(token.trim_end_matches(':')))
        .filter(|path| path.ends_with("out/embedded_metadata.rs"))
        .filter_map(|generated| generated.parent()?.parent().map(|dir| dir.join("output")))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        outputs.len(),
        1,
        "current lab dep-info must identify exactly one ls-core build output: {outputs:?}"
    );
    let output_path = outputs.pop_first().unwrap();
    std::fs::read_to_string(&output_path).unwrap_or_else(|error| {
        panic!(
            "read current ls-core output {}: {error}",
            output_path.display()
        )
    })
}

fn find_build_output_with_line(package_prefix: &str, required_line: &str) -> String {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_lab-research"));
    let build_dir = binary.parent().unwrap().join("build");
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&build_dir).unwrap() {
        let entry = entry.unwrap();
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(package_prefix)
        {
            continue;
        }
        let output_path = entry.path().join("output");
        let Ok(output) = std::fs::read_to_string(&output_path) else {
            continue;
        };
        if output.lines().any(|line| line == required_line) {
            let modified = std::fs::metadata(&output_path).unwrap().modified().unwrap();
            candidates.push((modified, output));
        }
    }
    candidates.sort_by_key(|(modified, _)| *modified);
    candidates
        .pop()
        .map(|(_, output)| output)
        .unwrap_or_else(|| {
            panic!(
                "no {package_prefix} build output under {} contained line {required_line}",
                build_dir.display()
            )
        })
}

fn uncovered_repository_watches(
    watched: &BTreeSet<PathBuf>,
    repo_root: &Path,
) -> Result<BTreeSet<PathBuf>, String> {
    let canonical_root = repo_root.canonicalize().map_err(|error| {
        format!(
            "canonicalize repository root {}: {error}",
            repo_root.display()
        )
    })?;
    let inventory = declared_inventory();
    watched
        .iter()
        .map(|path| {
            path.strip_prefix(&canonical_root)
                .map(Path::to_path_buf)
                .map_err(|_| {
                    format!(
                        "build script watches outside repository: {}",
                        path.display()
                    )
                })
        })
        .filter_map(|relative| match relative {
            Ok(relative) if inventory.iter().any(|input| input.covers(&relative)) => None,
            other => Some(other),
        })
        .collect()
}

fn rerun_paths(output: &str, package_root: &Path) -> BTreeSet<PathBuf> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("cargo:rerun-if-changed="))
        .map(Path::new)
        .map(|path| {
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                package_root.join(path)
            };
            absolute.canonicalize().unwrap_or_else(|error| {
                panic!("canonicalize watch {}: {error}", absolute.display())
            })
        })
        .collect()
}
