//! Cross-check the Consuming Project's SDK references against the Verification Bar.
//!
//! A reference is a typed SDK REST identifier (`T8410Request`, `CSPAT00601Request`,
//! `{TR}_POLICY`, and response carriers) or an executable lower-case REST TR literal such
//! as the `source_tr` provenance value `"t1463"`. Executable test modules count because
//! the contract is about references; documentation and raw WebSocket subscription strings
//! are not typed SDK TR references and are excluded.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

const FROZEN_MEL_CEILING: &[&str] = &[
    "t8430", "t1444", "t8407", "t1404", "t1405", "t9945", "t8410", "t1904", "t2522", "t0424",
    "t8450", "t1463",
];

const ACTIVE_MEL: &[&str] = &[
    "t8407", "t1404", "t1405", "t9945", "t1904", "t2522", "t0424", "t8450", "t1463",
];

const RETIRED_MEL: &[&str] = &["t8430", "t1444", "t8410"];

#[test]
fn adapter_tr_references_meet_the_verification_bar_or_the_active_mel() {
    let referenced = discover_adapter_tr_references();
    let recommendation_statuses = recommendation_statuses();
    let below_bar = referenced
        .iter()
        .filter(|tr| {
            !recommendation_statuses
                .get(*tr)
                .unwrap_or_else(|| panic!("referenced TR `{tr}` is absent from metadata"))
        })
        .cloned()
        .collect::<BTreeSet<_>>();

    assert_eq!(
        below_bar,
        tr_set(ACTIVE_MEL),
        "every below-bar adapter TR must be on the frozen, shrinking MEL; newly consumed TRs \
         must already be Recommended, and promoted TRs must leave the active list"
    );
}

#[test]
fn the_mel_can_only_shrink_from_its_frozen_ceiling() {
    assert_mel_partition(ACTIVE_MEL, RETIRED_MEL, FROZEN_MEL_CEILING);
}

#[test]
fn a_retired_mel_entry_cannot_be_resurrected() {
    let resurrected = ACTIVE_MEL
        .iter()
        .copied()
        .chain(["t8430"])
        .collect::<Vec<_>>();

    let error = mel_partition_error(&resurrected, RETIRED_MEL, FROZEN_MEL_CEILING)
        .expect("resurrecting a retired TR must invalidate the MEL partition");
    assert!(
        error.contains("both active and retired"),
        "unexpected validation error: {error}"
    );
}

#[test]
fn source_discovery_ignores_comments_but_keeps_typed_and_runtime_references() {
    let source = r#"
        //! Deferred comparisons mention t8418 and t8465 without consuming them.
        use ls_sdk::paginated::T8410Request;
        use ls_sdk::orders::CSPAT00601Request;
        type OrderInput = CSPAT00701InBlock;
        type OrderOutput = CSPAT00801OutBlock2;
        fn provenance() {
            let source_tr = "t1463";
        }
        register_tr!(T8450Response, nested!("t1904", CSPBQ00200InBlock1));
    "#;

    assert_eq!(
        tr_references_in_source(source),
        BTreeSet::from([
            "cspat00601".to_string(),
            "cspat00701".to_string(),
            "cspat00801".to_string(),
            "cspbq00200".to_string(),
            "t1463".to_string(),
            "t1904".to_string(),
            "t8410".to_string(),
            "t8450".to_string(),
        ])
    );
}

#[test]
fn source_universe_covers_adapter_and_lab_execution_roots_without_self_counting() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let relative_paths = adapter_source_paths(&manifest_dir)
        .into_iter()
        .map(|path| {
            path.strip_prefix(&manifest_dir)
                .expect("source path must be under the adapter manifest")
                .to_path_buf()
        })
        .collect::<BTreeSet<_>>();

    for expected in [
        "src/lib.rs",
        "tests/execution_client.rs",
        "lab/src/lib.rs",
        "lab/tests/live_driver.rs",
        "lab/build.rs",
        "lab/fingerprint_core.rs",
    ] {
        assert!(
            relative_paths.contains(Path::new(expected)),
            "source universe omitted {expected}"
        );
    }
    assert!(
        !relative_paths.contains(Path::new("tests/verification_bar.rs")),
        "the guard must not discover references from its own constants and fixtures"
    );
}

fn tr_set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn assert_mel_partition(active: &[&str], retired: &[&str], ceiling: &[&str]) {
    if let Some(error) = mel_partition_error(active, retired, ceiling) {
        panic!("invalid MEL partition: {error}");
    }
}

fn mel_partition_error(active: &[&str], retired: &[&str], ceiling: &[&str]) -> Option<String> {
    let active_set = tr_set(active);
    let retired_set = tr_set(retired);
    let ceiling_set = tr_set(ceiling);

    if ceiling_set.len() != ceiling.len() {
        return Some("the frozen MEL ceiling contains duplicates".to_string());
    }
    if active_set.len() != active.len() {
        return Some("the active MEL contains duplicates".to_string());
    }
    if retired_set.len() != retired.len() {
        return Some("the retired MEL contains duplicates".to_string());
    }
    let overlap = active_set
        .intersection(&retired_set)
        .cloned()
        .collect::<Vec<_>>();
    if !overlap.is_empty() {
        return Some(format!("TRs are both active and retired: {overlap:?}"));
    }

    let partition = active_set
        .union(&retired_set)
        .cloned()
        .collect::<BTreeSet<_>>();
    if partition != ceiling_set {
        return Some(format!(
            "active and retired MEL entries must partition the frozen ceiling; partition={partition:?}, ceiling={ceiling_set:?}"
        ));
    }
    None
}

fn discover_adapter_tr_references() -> BTreeSet<String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let paths = adapter_source_paths(&manifest_dir);

    let mut referenced = BTreeSet::new();
    for path in paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read adapter source {}: {error}", path.display()));
        let found = tr_references_in_source(&source);
        referenced.extend(found);
    }
    referenced
}

fn adapter_source_paths(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for relative_root in ["src", "tests", "lab/src", "lab/tests"] {
        collect_rust_sources(&manifest_dir.join(relative_root), &mut paths);
    }
    for relative_file in ["lab/build.rs", "lab/fingerprint_core.rs"] {
        let path = manifest_dir.join(relative_file);
        assert!(
            path.is_file(),
            "declared adapter source {} is absent",
            path.display()
        );
        paths.push(path);
    }

    let this_guard = manifest_dir.join("tests/verification_bar.rs");
    paths.retain(|path| path != &this_guard);
    paths.sort();
    paths
}

fn collect_rust_sources(directory: &Path, paths: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read source directory {}: {error}", directory.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!("read source entry under {}: {error}", directory.display())
        });
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!("read file type for {}: {error}", entry.path().display())
        });
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_rust_sources(&entry.path(), paths);
        } else if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "rs")
        {
            paths.push(entry.path());
        }
    }
}

fn tr_references_in_source(source: &str) -> BTreeSet<String> {
    let syntax = syn::parse_file(source).expect("adapter source must parse as Rust");
    let mut visitor = TrReferenceVisitor::default();
    visitor.visit_file(&syntax);
    visitor.references
}

#[derive(Default)]
struct TrReferenceVisitor {
    references: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for TrReferenceVisitor {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if attribute.path().is_ident("doc") {
            return;
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        if let Some(tr) = tr_from_ident(&ident.to_string()) {
            self.references.insert(tr);
        }
        syn::visit::visit_ident(self, ident);
    }

    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        collect_string_references(&literal.value(), &mut self.references);
        syn::visit::visit_lit_str(self, literal);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        collect_token_stream_references(node.tokens.clone(), &mut self.references);
        syn::visit::visit_macro(self, node);
    }
}

fn collect_token_stream_references(
    tokens: proc_macro2::TokenStream,
    references: &mut BTreeSet<String>,
) {
    for token in tokens {
        match token {
            proc_macro2::TokenTree::Group(group) => {
                collect_token_stream_references(group.stream(), references);
            }
            proc_macro2::TokenTree::Ident(ident) => {
                if let Some(tr) = tr_from_ident(&ident.to_string()) {
                    references.insert(tr);
                }
            }
            proc_macro2::TokenTree::Literal(literal) => {
                if let Ok(syn::Lit::Str(literal)) = syn::parse_str::<syn::Lit>(&literal.to_string())
                {
                    collect_string_references(&literal.value(), references);
                }
            }
            proc_macro2::TokenTree::Punct(_) => {}
        }
    }
}

fn collect_string_references(value: &str, references: &mut BTreeSet<String>) {
    for token in
        value.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
    {
        if is_runtime_tr_code(token) {
            references.insert(token.to_ascii_lowercase());
        }
    }
}

fn tr_from_ident(ident: &str) -> Option<String> {
    let bytes = ident.as_bytes();
    if bytes.len() >= 5
        && bytes[0] == b'T'
        && bytes[1..5].iter().all(u8::is_ascii_digit)
        && bytes.get(5).is_none_or(|byte| !byte.is_ascii_digit())
    {
        return Some(ident[..5].to_ascii_lowercase());
    }

    for block_marker in ["InBlock", "OutBlock"] {
        let Some((candidate, index)) = ident.split_once(block_marker) else {
            continue;
        };
        if index.chars().all(|character| character.is_ascii_digit()) && is_csp_code(candidate) {
            return Some(candidate.to_ascii_lowercase());
        }
    }

    for suffix in ["Request", "Response", "_POLICY"] {
        let Some(candidate) = ident.strip_suffix(suffix) else {
            continue;
        };
        if is_csp_code(candidate) {
            return Some(candidate.to_ascii_lowercase());
        }
    }
    None
}

fn is_csp_code(candidate: &str) -> bool {
    candidate.starts_with("CSP")
        && candidate
            .chars()
            .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
        && candidate
            .chars()
            .any(|character| character.is_ascii_digit())
}

fn is_runtime_tr_code(token: &str) -> bool {
    let bytes = token.as_bytes();
    (bytes.len() == 5 && bytes[0] == b't' && bytes[1..].iter().all(u8::is_ascii_digit))
        || (token.starts_with("csp")
            && token
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
            && token.chars().any(|character| character.is_ascii_digit()))
}

fn recommendation_statuses() -> BTreeMap<String, bool> {
    let metadata_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../metadata");
    let report = ls_metadata::validate_dir(&metadata_root).unwrap_or_else(|errors| {
        panic!(
            "validate metadata at {}: {errors:#?}",
            metadata_root.display()
        )
    });
    report
        .trs
        .into_iter()
        .map(|(tr, metadata)| (tr.to_ascii_lowercase(), metadata.support.recommended))
        .collect()
}
