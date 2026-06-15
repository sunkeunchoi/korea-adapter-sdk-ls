//! Change-Scoped test planner — the facet-driven verification selector.
//!
//! This is the architectural thesis of the maintained SDK: given a set of
//! changed TR codes (and/or changed `owner_class` values), the planner resolves
//! each change through the **validated metadata** to its `owner_class` + facets
//! and emits the concrete set of test groups to run. Selection is driven by
//! metadata **facets**, never by which crate a file lives in (R5) — a shape
//! change to a paginated TR pulls in pagination tests because the metadata says
//! `self_paginated: true`, not because the diff happened to touch a pagination
//! module.
//!
//! The facet → test-group mapping mirrors the "Testing And Evidence" section of
//! `docs/plans/maintained-sdk-migration-plan.md`:
//!
//! - A TR is changed (shape change)        → that TR's **serde** group.
//! - `facets.self_paginated == true`       → that TR's **pagination** group
//!                                            **+ the shared pagination** group.
//! - `facets.date_sensitive == true`       → **date/default-handling** group.
//! - `facets.account_state == true`         → **credential-free construction**
//!                                            group (NOT credentialed evidence).
//! - `facets.protocol == websocket`        → **subscribe / reconnect / frame**
//!                                            groups.
//! - A touched `owner_class`               → that **class's unit-test** group.
//!
//! A TR carrying multiple facets selects the **UNION** of every facet's groups
//! (e.g. `t8412` is `self_paginated` + `date_sensitive`, so it pulls serde +
//! pagination + shared-pagination + date groups, plus the paginated class unit
//! group). Routing **composes**: each facet contributes its groups independently
//! into one set, rather than the first matching facet winning.
//!
//! The output is a *declarative* plan — a deterministic, sorted set of group
//! identifiers. It is not a test runner and it does not execute anything.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::schema::{OwnerClass, Protocol, TrMetadata};
use crate::validator::ValidationReport;

/// One selected test group in a verification plan.
///
/// Variants that scope to a single TR carry the TR code; the class unit group
/// carries the [`OwnerClass`]; shared groups carry nothing. The type derives
/// `Ord` so a [`BTreeSet`] of these is deterministically ordered, making exact
/// selected-set equality trivial to assert in tests.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TestGroup {
    /// Request/response serde tests for a TR whose shape changed.
    Serde(String),
    /// The TR's own pagination tests (selected by `self_paginated`).
    Pagination(String),
    /// The shared pagination test group (selected once by any `self_paginated`
    /// TR; carries no TR code because it is cross-cutting).
    SharedPagination,
    /// Date / default-handling tests (selected by `date_sensitive`).
    DateHandling(String),
    /// Credential-free request-construction tests (selected by `account_state`).
    /// Deliberately distinct from credentialed evidence, which is scheduled
    /// separately and never selected here.
    CredentialFreeConstruction(String),
    /// WebSocket subscribe / reconnect / frame lifecycle tests (selected by
    /// `protocol: websocket`).
    WebsocketLifecycle(String),
    /// The owning dependency class's unit-test group (selected by a touched
    /// `owner_class`, and by every changed TR for its own class).
    ClassUnit(OwnerClass),
}

impl fmt::Display for TestGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestGroup::Serde(tr) => write!(f, "serde({tr})"),
            TestGroup::Pagination(tr) => write!(f, "pagination({tr})"),
            TestGroup::SharedPagination => write!(f, "shared_pagination"),
            TestGroup::DateHandling(tr) => write!(f, "date_handling({tr})"),
            TestGroup::CredentialFreeConstruction(tr) => {
                write!(f, "credential_free_construction({tr})")
            }
            TestGroup::WebsocketLifecycle(tr) => write!(f, "websocket_lifecycle({tr})"),
            TestGroup::ClassUnit(class) => write!(f, "class_unit({class:?})"),
        }
    }
}

/// A located planning failure. Mirrors the validator's located-error
/// convention: a changed TR with no metadata record names the missing TR code
/// rather than silently selecting nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// A changed TR code has no metadata record in the [`ValidationReport`].
    UnknownTr { tr_code: String },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::UnknownTr { tr_code } => write!(
                f,
                "changed TR `{tr_code}`: no metadata record found (cannot select tests)"
            ),
        }
    }
}

impl std::error::Error for PlanError {}

/// The set of changes to plan a verification run for.
///
/// `tr_codes` are TRs whose shape/behaviour changed; `owner_classes` are
/// dependency classes touched directly (e.g. a shared change inside a class
/// module). Both feed the selection; either may be empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSet {
    pub tr_codes: BTreeSet<String>,
    pub owner_classes: BTreeSet<OwnerClass>,
}

impl ChangeSet {
    /// A change set from changed TR codes only.
    pub fn from_trs<I, S>(tr_codes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        ChangeSet {
            tr_codes: tr_codes.into_iter().map(Into::into).collect(),
            owner_classes: BTreeSet::new(),
        }
    }

    /// Add a touched `owner_class` to the change set (builder-style).
    pub fn with_owner_class(mut self, class: OwnerClass) -> Self {
        self.owner_classes.insert(class);
        self
    }
}

/// Resolve the facet-driven groups a single changed TR contributes, inserting
/// every group into `selected`. This is the composition core: each facet that
/// is set contributes its own groups, so a multi-facet TR yields the UNION.
fn select_for_tr(meta: &TrMetadata, selected: &mut BTreeSet<TestGroup>) {
    let tr = meta.tr_code.clone();

    // A changed TR is always a (potential) shape change → serde group.
    selected.insert(TestGroup::Serde(tr.clone()));

    // A changed TR exercises its owning class's unit tests.
    selected.insert(TestGroup::ClassUnit(meta.owner_class));

    let facets = &meta.facets;

    if facets.self_paginated {
        selected.insert(TestGroup::Pagination(tr.clone()));
        selected.insert(TestGroup::SharedPagination);
    }
    if facets.date_sensitive {
        selected.insert(TestGroup::DateHandling(tr.clone()));
    }
    if facets.account_state {
        selected.insert(TestGroup::CredentialFreeConstruction(tr.clone()));
    }
    if matches!(facets.protocol, Protocol::Websocket) {
        selected.insert(TestGroup::WebsocketLifecycle(tr.clone()));
    }
}

/// Plan the Change-Scoped verification set for a [`ChangeSet`] against the
/// validated metadata in a [`ValidationReport`].
///
/// Each changed TR is resolved through `report.trs` to its `owner_class` +
/// facets and contributes the UNION of all its facets' groups. Each touched
/// `owner_class` contributes that class's unit-test group. The returned set is
/// deterministic (a sorted [`BTreeSet`]).
///
/// A changed TR with no metadata record surfaces a located
/// [`PlanError::UnknownTr`] naming the missing TR — it is never silently
/// dropped. (Touched `owner_class` values are a closed enum and need no such
/// lookup.) When several TRs are unknown, every one is reported.
pub fn plan_changes(
    report: &ValidationReport,
    changes: &ChangeSet,
) -> Result<BTreeSet<TestGroup>, Vec<PlanError>> {
    plan_with_metadata(&report.trs, changes)
}

/// Lower-level entry point operating directly on a TR-code → metadata map, so
/// callers (and tests) can drive the planner from inline fixtures without
/// constructing a full [`ValidationReport`]. [`plan_changes`] delegates here.
pub fn plan_with_metadata(
    trs: &BTreeMap<String, TrMetadata>,
    changes: &ChangeSet,
) -> Result<BTreeSet<TestGroup>, Vec<PlanError>> {
    let mut selected: BTreeSet<TestGroup> = BTreeSet::new();
    let mut errors: Vec<PlanError> = Vec::new();

    for tr_code in &changes.tr_codes {
        match trs.get(tr_code) {
            Some(meta) => select_for_tr(meta, &mut selected),
            None => errors.push(PlanError::UnknownTr {
                tr_code: tr_code.clone(),
            }),
        }
    }

    // A touched dependency class pulls in that class's unit tests directly.
    for class in &changes.owner_classes {
        selected.insert(TestGroup::ClassUnit(*class));
    }

    if errors.is_empty() {
        Ok(selected)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::validate_dir;
    use std::path::PathBuf;

    /// The authored metadata root (`<repo>/metadata`), resolved from this
    /// crate's manifest dir so the tests double as an integration check over the
    /// real authored set.
    fn authored_metadata_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("metadata")
    }

    fn authored_report() -> ValidationReport {
        validate_dir(&authored_metadata_root())
            .expect("authored metadata under metadata/ must validate clean")
    }

    fn plan(report: &ValidationReport, trs: &[&str]) -> BTreeSet<TestGroup> {
        plan_changes(report, &ChangeSet::from_trs(trs.iter().copied()))
            .expect("known TRs must plan without error")
    }

    /// Changing `t8412` selects the multi-facet UNION: serde + pagination +
    /// shared-pagination (self_paginated) + date (date_sensitive) + the
    /// paginated class unit group. This is the key composition property.
    #[test]
    fn t8412_selects_multi_facet_union() {
        let report = authored_report();
        let selected = plan(&report, &["t8412"]);

        let expected: BTreeSet<TestGroup> = [
            TestGroup::Serde("t8412".to_string()),
            TestGroup::Pagination("t8412".to_string()),
            TestGroup::SharedPagination,
            TestGroup::DateHandling("t8412".to_string()),
            TestGroup::ClassUnit(OwnerClass::Paginated),
        ]
        .into_iter()
        .collect();

        assert_eq!(selected, expected, "exact selected-group set for t8412");
    }

    /// The composition is a UNION, not first-facet-wins: both the pagination
    /// AND the date groups are present together for the multi-facet TR.
    #[test]
    fn t8412_routing_composes_not_fires_once() {
        let report = authored_report();
        let selected = plan(&report, &["t8412"]);

        assert!(
            selected.contains(&TestGroup::Pagination("t8412".to_string())),
            "self_paginated facet must contribute its pagination group"
        );
        assert!(
            selected.contains(&TestGroup::SharedPagination),
            "self_paginated facet must contribute the shared pagination group"
        );
        assert!(
            selected.contains(&TestGroup::DateHandling("t8412".to_string())),
            "date_sensitive facet must compose in alongside pagination, not be skipped"
        );
    }

    /// Changing `CSPAQ12200` selects credential-free construction (account_state)
    /// and NEVER any credentialed-evidence group.
    #[test]
    fn cspaq12200_selects_credential_free_only() {
        let report = authored_report();
        let selected = plan(&report, &["CSPAQ12200"]);

        let expected: BTreeSet<TestGroup> = [
            TestGroup::Serde("CSPAQ12200".to_string()),
            TestGroup::CredentialFreeConstruction("CSPAQ12200".to_string()),
            TestGroup::ClassUnit(OwnerClass::Account),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            selected, expected,
            "exact selected-group set for CSPAQ12200"
        );

        // There is no credentialed-evidence variant at all, and the
        // credential-free group is present — the gate runs construction only.
        assert!(selected.contains(&TestGroup::CredentialFreeConstruction(
            "CSPAQ12200".to_string()
        )));
        assert!(
            !selected
                .iter()
                .any(|g| matches!(g, TestGroup::WebsocketLifecycle(_))),
            "an account TR must not pull realtime groups"
        );
    }

    /// Changing `S3_` selects the websocket subscribe/reconnect/frame group
    /// (protocol: websocket).
    #[test]
    fn s3_selects_websocket_lifecycle() {
        let report = authored_report();
        let selected = plan(&report, &["S3_"]);

        let expected: BTreeSet<TestGroup> = [
            TestGroup::Serde("S3_".to_string()),
            TestGroup::WebsocketLifecycle("S3_".to_string()),
            TestGroup::ClassUnit(OwnerClass::Realtime),
        ]
        .into_iter()
        .collect();

        assert_eq!(selected, expected, "exact selected-group set for S3_");
    }

    /// A plain standalone TR (no special facets) selects only serde + its class
    /// unit group.
    #[test]
    fn token_selects_serde_and_class_unit_only() {
        let report = authored_report();
        let selected = plan(&report, &["token"]);

        let expected: BTreeSet<TestGroup> = [
            TestGroup::Serde("token".to_string()),
            TestGroup::ClassUnit(OwnerClass::Standalone),
        ]
        .into_iter()
        .collect();

        assert_eq!(selected, expected, "exact selected-group set for token");
    }

    /// A market_session TR (t1102) — no facets beyond rest — selects serde +
    /// its class unit group, and no date/pagination/websocket groups.
    #[test]
    fn t1102_selects_serde_and_class_unit_only() {
        let report = authored_report();
        let selected = plan(&report, &["t1102"]);

        let expected: BTreeSet<TestGroup> = [
            TestGroup::Serde("t1102".to_string()),
            TestGroup::ClassUnit(OwnerClass::MarketSession),
        ]
        .into_iter()
        .collect();

        assert_eq!(selected, expected, "exact selected-group set for t1102");
    }

    /// Multiple changed TRs union their selections across TRs as well as across
    /// facets; SharedPagination appears once.
    #[test]
    fn multiple_trs_union_across_changes() {
        let report = authored_report();
        let selected = plan(&report, &["t8412", "S3_"]);

        let expected: BTreeSet<TestGroup> = [
            TestGroup::Serde("t8412".to_string()),
            TestGroup::Pagination("t8412".to_string()),
            TestGroup::SharedPagination,
            TestGroup::DateHandling("t8412".to_string()),
            TestGroup::ClassUnit(OwnerClass::Paginated),
            TestGroup::Serde("S3_".to_string()),
            TestGroup::WebsocketLifecycle("S3_".to_string()),
            TestGroup::ClassUnit(OwnerClass::Realtime),
        ]
        .into_iter()
        .collect();

        assert_eq!(selected, expected);
    }

    /// A touched `owner_class` (no TR change) pulls in that class's unit group.
    #[test]
    fn touched_owner_class_selects_class_unit() {
        let report = authored_report();
        let changes = ChangeSet::default().with_owner_class(OwnerClass::Paginated);
        let selected = plan_changes(&report, &changes).expect("owner_class plan must succeed");

        let expected: BTreeSet<TestGroup> = [TestGroup::ClassUnit(OwnerClass::Paginated)]
            .into_iter()
            .collect();
        assert_eq!(selected, expected);
    }

    /// A changed TR with no metadata record surfaces a located error naming the
    /// missing TR, rather than silently selecting nothing.
    #[test]
    fn unknown_tr_surfaces_located_error() {
        let report = authored_report();
        let errors = plan_changes(&report, &ChangeSet::from_trs(["t9999_nope"]))
            .expect_err("an unknown TR must fail");

        assert_eq!(errors.len(), 1);
        let msg = errors[0].to_string();
        assert!(
            msg.contains("t9999_nope"),
            "error names the missing TR: {msg}"
        );
        assert!(matches!(
            &errors[0],
            PlanError::UnknownTr { tr_code } if tr_code == "t9999_nope"
        ));
    }

    /// Every unknown TR among the changes is reported, not just the first.
    #[test]
    fn multiple_unknown_trs_all_reported() {
        let report = authored_report();
        let errors = plan_changes(&report, &ChangeSet::from_trs(["nope_a", "nope_b"]))
            .expect_err("unknown TRs must fail");
        assert_eq!(errors.len(), 2);
    }

    /// The returned set is deterministic — planning the same change twice yields
    /// an identical ordered sequence.
    #[test]
    fn selection_is_deterministic() {
        let report = authored_report();
        let a: Vec<_> = plan(&report, &["t8412", "S3_", "CSPAQ12200"])
            .into_iter()
            .collect();
        let b: Vec<_> = plan(&report, &["CSPAQ12200", "S3_", "t8412"])
            .into_iter()
            .collect();
        assert_eq!(
            a, b,
            "BTreeSet iteration order is stable regardless of input order"
        );
    }
}
