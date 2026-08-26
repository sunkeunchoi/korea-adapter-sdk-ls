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
        "adapters/nautilus/src/lib.rs",
        "adapters/nautilus/nautilus-ls-calendar/src/lib.rs",
        "adapters/nautilus/nautilus-ls-calendar/Cargo.toml",
        "adapters/nautilus/Cargo.toml",
        "adapters/nautilus/Cargo.lock",
        "adapters/nautilus/rust-toolchain.toml",
    ];
    // A declared entry added without a mutation case here would leave the new class
    // unproven while the suite stayed green.
    assert_eq!(cases.len(), declared_inventory().len());

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
        "adapters/nautilus/src",
        "adapters/nautilus/nautilus-ls-calendar/src",
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

/// The tree walk reads each entry's node kind to reject symlinks and special nodes.
/// Without this case that refusal is unexercised, and a walk that silently followed a
/// link would hash the target's bytes under the link's path — a digest that moves when
/// something outside the declared tree changes.
#[cfg(unix)]
#[test]
fn a_symlink_inside_a_declared_tree_is_refused() {
    let fixture = FingerprintFixture::new();
    recompute_from_root(fixture.root()).expect("the unchanged fixture must hash");
    std::os::unix::fs::symlink(
        fixture.path("Cargo.toml"),
        fixture.path("adapters/nautilus/src/linked.rs"),
    )
    .unwrap();
    let error = recompute_from_root(fixture.root())
        .expect_err("a symlink inside a declared tree must fail closed");
    assert!(
        error.to_string().contains("contains a symlink"),
        "refusal must name the symlink, not fail for some other reason: {error}"
    );
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
        "Cargo.lock",
        "target/debug/generated.rs",
        "adapters/nautilus/target/debug/generated.rs",
        "crates/ls-sdk-test-support/src/lib.rs",
        "adapters/nautilus/state/krx.calendar.json",
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
    for (_, package_root_relative, _) in LINKED_REPOSITORY_PACKAGES {
        // The linked tree, not the package root: a bare `adapters/nautilus` prefix
        // would be satisfied by any path in the adapter workspace.
        let expected = Path::new(package_root_relative).join("src");
        assert!(
            selected_sources.iter().any(|path| path.starts_with(&expected)),
            "Cargo dep-info must expose repository-local sources under {}",
            expected.display()
        );
    }
    let uncovered = uncovered_repository_dependencies(&selected_sources);
    assert!(
        uncovered.is_empty(),
        "Cargo compiled inputs outside the declared boundary: {uncovered:?}"
    );
}

/// A build script at a linked package root is a compiled input that never appears in
/// that package's lib dep-info, so the dependency-evidence oracle above cannot see it.
/// `crates/ls-core/build.rs` is declared for exactly this reason; this test is what
/// makes that class fail closed instead of silently escaping the boundary.
#[test]
fn no_linked_package_has_an_undeclared_build_script() {
    let repo_root = nautilus_ls_lab::fingerprint::compiled_repo_root();
    let inventory = declared_inventory();
    for (package, package_root_relative, _) in LINKED_REPOSITORY_PACKAGES {
        let relative = Path::new(package_root_relative).join("build.rs");
        if !repo_root.join(&relative).exists() {
            continue;
        }
        assert!(
            inventory.iter().any(|input| input.covers(&relative)),
            "{package} has a build script no declared entry covers: {}",
            relative.display()
        );
    }
}

/// `LINKED_REPOSITORY_PACKAGES` is a hand-written table, so on its own it cannot
/// notice a fifth repository-local crate — the fallback decoder would skip the new
/// package and the completeness assertion would compare the table to itself. Pin it
/// to an independent source of truth: the adapter lockfile, which lists every
/// repository-local package as a `[[package]]` block carrying no `source` key. A new
/// local crate reds this test until someone classifies it as linked or not.
#[test]
fn the_linked_package_table_accounts_for_every_repository_local_package() {
    let repo_root = nautilus_ls_lab::fingerprint::compiled_repo_root();
    let lock = std::fs::read_to_string(repo_root.join("adapters/nautilus/Cargo.lock"))
        .expect("read the adapter lockfile");
    let classified: BTreeSet<&str> = LINKED_REPOSITORY_PACKAGES
        .iter()
        .map(|(package, _, _)| *package)
        .chain(UNLINKED_REPOSITORY_PACKAGES.iter().map(|(package, _)| *package))
        .collect();
    assert_eq!(
        local_lockfile_packages(&lock),
        classified,
        "every repository-local package must be classified as linked or unlinked"
    );
}

#[test]
fn ls_core_build_watch_evidence_has_no_undeclared_repository_input() {
    let repo_root = nautilus_ls_lab::fingerprint::compiled_repo_root();
    let output = ls_core_build_outputs(&lab_dependency_evidence());
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
fn dependency_evidence_falls_back_to_cargo_fingerprints_without_a_binary_sidecar() {
    let temp = tempfile::TempDir::new().unwrap();
    let profile_dir = temp.path().join("debug");
    let fingerprint_dir = profile_dir.join(".fingerprint");
    for (directory, dep_info_name, paths) in [
        (
            "ls-sdk-current",
            "dep-lib-ls_sdk",
            vec![(0, "src/lib.rs"), (0, "src/client.rs")],
        ),
        (
            "ls-core-current",
            "dep-lib-ls_core",
            vec![
                (0, "src/lib.rs"),
                (1, "debug/build/ls-core-current/out/embedded_metadata.rs"),
            ],
        ),
        (
            "nautilus-ls-current",
            "dep-lib-nautilus_ls",
            vec![(0, "src/lib.rs"), (0, "src/constraints.rs")],
        ),
        (
            "nautilus-ls-calendar-current",
            "dep-lib-nautilus_ls_calendar",
            vec![(0, "src/lib.rs")],
        ),
        // Prefix-sharing siblings the table must NOT resolve. Each carries a source
        // path unique to it, so a decoder that mis-attributed the directory would emit
        // that path and red the negative assertions below — a shared `src/lib.rs`
        // would collide with a linked package's own path and prove nothing.
        (
            "nautilus-ls-lab-current",
            "dep-lib-nautilus_ls_lab",
            vec![(0, "src/lab_only.rs")],
        ),
        (
            "ls-sdk-test-support-current",
            "dep-lib-ls_sdk_test_support",
            vec![(0, "src/support_only.rs")],
        ),
        // A second fingerprint directory for one package: the lib dep-info is absent
        // here, so this exercises the NotFound-continue arm that every bin-target
        // directory hits in a real build tree.
        (
            "nautilus-ls-4f2a91c0",
            "dep-bin-ls_ingest",
            vec![(0, "src/bin/ls-ingest.rs")],
        ),
        // A third, carrying a valid lib dep-info: evidence from every matching
        // directory accumulates rather than the last one winning.
        (
            "nautilus-ls-9b7d33e1",
            "dep-lib-nautilus_ls",
            vec![(0, "src/instrument.rs")],
        ),
    ] {
        let package_dir = fingerprint_dir.join(directory);
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(
            package_dir.join(dep_info_name),
            encoded_cargo_dep_info(&paths),
        )
        .unwrap();
    }

    let evidence = dependency_evidence_for_binary(
        &profile_dir.join("lab-research"),
        &temp.path().join("repo"),
    );

    assert!(evidence.contains("crates/ls-sdk/src/client.rs"));
    assert!(evidence.contains("crates/ls-core/src/lib.rs"));
    assert!(evidence.contains("adapters/nautilus/src/constraints.rs"));
    assert!(evidence.contains("adapters/nautilus/nautilus-ls-calendar/src/lib.rs"));
    assert!(evidence.contains("embedded_metadata.rs"));
    // Accumulated across both nautilus-ls fingerprint directories.
    assert!(evidence.contains("adapters/nautilus/src/instrument.rs"));
    // A package that merely shares a prefix with a linked one contributes nothing:
    // the lab itself is not a dependency of itself, and test-support is dev-only.
    assert!(!evidence.contains("lab_only.rs"));
    assert!(!evidence.contains("support_only.rs"));
    // The bin-target directory carries no lib dep-info, so it is skipped entirely.
    assert!(!evidence.contains("ls-ingest.rs"));
}

/// The other falsifiers hand `uncovered_repository_dependencies` a fabricated evidence
/// string, so a broken fallback decoder is never on their execution path. This one
/// plants an undeclared input as a real Cargo dep-info record and drives the whole
/// degraded chain — fingerprint-directory matching, dep-info decoding, package-root
/// resolution, coverage subtraction — so the fallback is proven to *report* a gap
/// rather than merely to be clean against today's tree.
#[test]
fn the_cargo_fingerprint_fallback_reports_an_undeclared_input() {
    let fixture = FingerprintFixture::new();
    let profile_dir = fixture.path("adapters/nautilus/target/debug");
    // Every linked package must be present or the helper's completeness assertion fires
    // first; only nautilus-ls carries the undeclared extra input.
    for (package, _, dep_info_name) in LINKED_REPOSITORY_PACKAGES {
        let package_dir = profile_dir.join(".fingerprint").join(format!("{package}-1c0ffee0"));
        std::fs::create_dir_all(&package_dir).unwrap();
        let paths = if package == "nautilus-ls" {
            // `src/lib.rs` is declared; the generated sibling at the package root is not.
            vec![(0, "src/lib.rs"), (0, "generated_data.rs")]
        } else {
            vec![(0, "src/lib.rs")]
        };
        std::fs::write(
            package_dir.join(dep_info_name),
            encoded_cargo_dep_info(&paths),
        )
        .unwrap();
    }

    let evidence =
        cargo_fingerprint_dependency_evidence(&profile_dir.join("lab-research"), fixture.root());
    let paths = repository_dependency_paths(&evidence, fixture.root());
    let uncovered = uncovered_repository_dependencies(&paths);
    assert_eq!(
        uncovered,
        BTreeSet::from([PathBuf::from("adapters/nautilus/generated_data.rs")]),
        "the fallback path must surface an undeclared repository-local input"
    );
}

/// The synthetic test above proves the decoder's shape; this one proves the closure
/// actually holds on the degraded evidence path against the real build directory, so
/// "the boundary does not depend on which evidence format survived" is an executed
/// assertion rather than a documented intention.
#[test]
fn the_real_cargo_fingerprint_tree_yields_the_same_closed_boundary() {
    let repo_root = nautilus_ls_lab::fingerprint::compiled_repo_root();
    let evidence = cargo_fingerprint_dependency_evidence(
        &PathBuf::from(env!("CARGO_BIN_EXE_lab-research")),
        &repo_root,
    );
    let selected_sources = repository_dependency_paths(&evidence, &repo_root);
    for (_, package_root_relative, _) in LINKED_REPOSITORY_PACKAGES {
        let expected = Path::new(package_root_relative).join("src");
        assert!(
            selected_sources.iter().any(|path| path.starts_with(&expected)),
            "Cargo fingerprint evidence must expose sources under {}",
            expected.display()
        );
    }
    let uncovered = uncovered_repository_dependencies(&selected_sources);
    assert!(
        uncovered.is_empty(),
        "Cargo fingerprint inputs outside the declared boundary: {uncovered:?}"
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

/// Deliberate sibling of the root-source falsifier, and not redundant with it. This one
/// plants *inside* the adapter workspace, where the calendar package's `src` tree and
/// manifest are declared but nothing else in that package is. Any per-package exemption
/// predicate would wrongly subtract this path while leaving the root falsifier green, so
/// the two tests must not be merged or parameterized into one.
///
/// It falsifies the *predicate*, not a reachable evidence shape: a build script at a
/// package root does not appear in that package's lib dep-info, so real evidence would
/// not carry this path. `no_linked_package_has_an_undeclared_build_script` is what
/// covers the reachable form of this class.
#[test]
fn synthetic_dependency_evidence_exposes_an_undeclared_adapter_workspace_input() {
    let fixture = FingerprintFixture::new();
    let undeclared = fixture.path("adapters/nautilus/nautilus-ls-calendar/build.rs");
    std::fs::write(&undeclared, "fn main() {}\n").unwrap();
    let evidence = format!(
        "{}: {} {} {}\n",
        fixture
            .path("adapters/nautilus/target/debug/lab-research")
            .display(),
        fixture.path("adapters/nautilus/src/lib.rs").display(),
        fixture
            .path("adapters/nautilus/nautilus-ls-calendar/src/lib.rs")
            .display(),
        undeclared.display()
    );
    let paths = repository_dependency_paths(&evidence, fixture.root());
    let uncovered = uncovered_repository_dependencies(&paths);
    assert_eq!(
        uncovered,
        BTreeSet::from([PathBuf::from(
            "adapters/nautilus/nautilus-ls-calendar/build.rs"
        )])
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
    dependency_evidence_for_binary(&binary, &nautilus_ls_lab::fingerprint::compiled_repo_root())
}

fn dependency_evidence_for_binary(binary: &Path, repo_root: &Path) -> String {
    let dependency_file = binary.with_extension("d");
    if std::env::var_os("LAB_TEST_FORCE_CARGO_FINGERPRINT_EVIDENCE").is_none() {
        match std::fs::read_to_string(&dependency_file) {
            Ok(evidence) => return evidence,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("read {}: {error}", dependency_file.display()),
        }
    }
    cargo_fingerprint_dependency_evidence(binary, repo_root)
}

/// Every repository-local package the lab links, paired with the repository-relative
/// root its dep-info paths resolve against and the dep-info file Cargo writes for its
/// lib target. This is the single place the package set is stated: the dispatch below
/// and the completeness assertion both derive from it, so a fifth package cannot be
/// added to one and forgotten in the other.
const LINKED_REPOSITORY_PACKAGES: [(&str, &str, &str); 4] = [
    ("ls-sdk", "crates/ls-sdk", "dep-lib-ls_sdk"),
    ("ls-core", "crates/ls-core", "dep-lib-ls_core"),
    ("nautilus-ls", "adapters/nautilus", "dep-lib-nautilus_ls"),
    (
        "nautilus-ls-calendar",
        "adapters/nautilus/nautilus-ls-calendar",
        "dep-lib-nautilus_ls_calendar",
    ),
];

/// The repository-local packages the adapter lockfile carries that the lab binary does
/// NOT link, each with the reason it contributes no compiled input. Kept beside the
/// linked table so the two together account for every local package in the lockfile —
/// that is what turns a new repository-local crate into a red test rather than a
/// silently skipped one on the fallback evidence path.
const UNLINKED_REPOSITORY_PACKAGES: [(&str, &str); 3] = [
    (
        "nautilus-ls-lab",
        "the crate under test; it is not a dependency of itself",
    ),
    (
        "ls-sdk-test-support",
        "dev-only wiremock helper, so it cannot change a shipped binary",
    ),
    (
        "ls-metadata",
        "outside the lab binary's dependency closure; reachable only from root tooling",
    ),
];

/// Every `[[package]]` block in a Cargo lockfile that carries no `source` key — the
/// lockfile's own way of saying the package comes from this repository rather than a
/// registry.
fn local_lockfile_packages(lock: &str) -> BTreeSet<&str> {
    lock.split("[[package]]")
        .skip(1)
        .filter(|block| {
            !block
                .lines()
                .any(|line| line.trim_start().starts_with("source = "))
        })
        .filter_map(|block| {
            block.lines().find_map(|line| {
                line.trim_start()
                    .strip_prefix("name = \"")?
                    .strip_suffix('"')
            })
        })
        .collect()
}

fn cargo_fingerprint_dependency_evidence(binary: &Path, repo_root: &Path) -> String {
    let profile_dir = binary.parent().expect("lab binary has a profile directory");
    let target_dir = profile_dir
        .parent()
        .expect("profile has a target directory");
    let fingerprint_dir = profile_dir.join(".fingerprint");
    let mut evidence = String::new();
    let mut found_packages = BTreeSet::new();

    for entry in std::fs::read_dir(&fingerprint_dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", fingerprint_dir.display()))
    {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Cargo names each fingerprint directory `<package>-<hash>`. Match the package
        // name exactly, so a sibling that merely shares a prefix — `nautilus-ls-lab`,
        // `ls-sdk-test-support` — is never resolved against the wrong package root.
        let package_name = name.rsplit_once('-').map_or(&*name, |(package, _)| package);
        let Some(&(package, package_root_relative, dep_info_name)) = LINKED_REPOSITORY_PACKAGES
            .iter()
            .find(|(candidate, _, _)| *candidate == package_name)
        else {
            continue;
        };
        let dep_info_path = entry.path().join(dep_info_name);
        let raw = match std::fs::read(&dep_info_path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("read {}: {error}", dep_info_path.display()),
        };
        found_packages.insert(package);
        let package_root = repo_root.join(package_root_relative);
        for (path_type, path) in decode_cargo_dep_info(&raw, &dep_info_path) {
            let absolute = if path.is_absolute() {
                path
            } else if path_type == 1 {
                target_dir.join(path)
            } else {
                package_root.join(path)
            };
            evidence.push_str(&absolute.to_string_lossy());
            evidence.push('\n');
        }
    }

    assert_eq!(
        found_packages,
        LINKED_REPOSITORY_PACKAGES
            .iter()
            .map(|(package, _, _)| *package)
            .collect::<BTreeSet<_>>(),
        "Cargo fingerprint evidence must include every repository-local package the lab links"
    );
    evidence
}

fn decode_cargo_dep_info(raw: &[u8], source: &Path) -> Vec<(u8, PathBuf)> {
    let mut remaining = raw;
    let decoded = (|| {
        if take_u32(&mut remaining)? != 1 || take_u8(&mut remaining)? != u8::MAX {
            return None;
        }
        if take_u8(&mut remaining)? != 1 {
            return None;
        }
        let file_count = take_u32(&mut remaining)?;
        let mut paths = Vec::with_capacity(file_count);
        for _ in 0..file_count {
            let path_type = take_u8(&mut remaining)?;
            if path_type > 1 {
                return None;
            }
            let path = std::str::from_utf8(take_bytes(&mut remaining)?).ok()?;
            let has_checksum = take_u8(&mut remaining)? != 0;
            if has_checksum {
                take_u64(&mut remaining)?;
                take_bytes(&mut remaining)?;
            }
            paths.push((path_type, PathBuf::from(path)));
        }
        let environment_count = take_u32(&mut remaining)?;
        for _ in 0..environment_count {
            std::str::from_utf8(take_bytes(&mut remaining)?).ok()?;
            if take_u8(&mut remaining)? != 0 {
                std::str::from_utf8(take_bytes(&mut remaining)?).ok()?;
            }
        }
        remaining.is_empty().then_some(paths)
    })();
    decoded.unwrap_or_else(|| {
        panic!(
            "unsupported or corrupt Cargo dep-info: {}",
            source.display()
        )
    })
}

fn take_u8(bytes: &mut &[u8]) -> Option<u8> {
    let value = *bytes.first()?;
    *bytes = &bytes[1..];
    Some(value)
}

fn take_u32(bytes: &mut &[u8]) -> Option<usize> {
    let value = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    *bytes = &bytes[4..];
    Some(value)
}

fn take_u64(bytes: &mut &[u8]) -> Option<u64> {
    let value = u64::from_le_bytes(bytes.get(..8)?.try_into().ok()?);
    *bytes = &bytes[8..];
    Some(value)
}

fn take_bytes<'a>(bytes: &mut &'a [u8]) -> Option<&'a [u8]> {
    let length = take_u32(bytes)?;
    let value = bytes.get(..length)?;
    *bytes = &bytes[length..];
    Some(value)
}

fn encoded_cargo_dep_info(paths: &[(u8, &str)]) -> Vec<u8> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(&1_u32.to_le_bytes());
    encoded.push(u8::MAX);
    encoded.push(1);
    encoded.extend_from_slice(&(paths.len() as u32).to_le_bytes());
    for (path_type, path) in paths {
        encoded.push(*path_type);
        encoded.extend_from_slice(&(path.len() as u32).to_le_bytes());
        encoded.extend_from_slice(path.as_bytes());
        encoded.push(0);
    }
    encoded.extend_from_slice(&0_u32.to_le_bytes());
    encoded
}

fn repository_dependency_paths(evidence: &str, repo_root: &Path) -> BTreeSet<PathBuf> {
    evidence
        .replace("\\\n", " ")
        .split_whitespace()
        .map(|token| PathBuf::from(token.trim_end_matches(':')))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|token| repository_relative_path(&token, repo_root))
        .collect()
}

fn uncovered_repository_dependencies(paths: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
    let inventory = declared_inventory();
    paths
        .iter()
        .filter(|path| {
            !inventory.iter().any(|input| input.covers(path))
                && !is_generated_artifact_dependency(path)
        })
        .cloned()
        .collect()
}

/// Generated build output is the only permitted subtraction: it has no source form
/// to declare, and build-script output such as `ls-core`'s embedded metadata
/// legitimately appears in dependency evidence. There is deliberately no
/// package-specific exception — a repository-local compiled input outside the
/// declared inventory is a gap, not a deferral.
fn is_generated_artifact_dependency(path: &Path) -> bool {
    path.starts_with("adapters/nautilus/target")
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

fn ls_core_build_outputs(dependency_evidence: &str) -> String {
    let outputs = dependency_evidence
        .replace("\\\n", " ")
        .split_whitespace()
        .map(|token| PathBuf::from(token.trim_end_matches(':')))
        .filter(|path| path.ends_with("out/embedded_metadata.rs"))
        .filter_map(|generated| generated.parent()?.parent().map(|dir| dir.join("output")))
        .collect::<BTreeSet<_>>();
    let mut evidence = String::new();
    for output_path in outputs {
        match std::fs::read_to_string(&output_path) {
            Ok(output) => {
                evidence.push_str(&output);
                evidence.push('\n');
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("read ls-core output {}: {error}", output_path.display()),
        }
    }
    assert!(
        !evidence.is_empty(),
        "Cargo dependency evidence must identify at least one available ls-core build output"
    );
    evidence
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
