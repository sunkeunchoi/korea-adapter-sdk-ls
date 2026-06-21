//! Drift gate (U5 ⇄ U8): the hand-authored `{TR}_POLICY` runtime consts in
//! `endpoint_policy.rs` are the runtime mirror of the maintained metadata index.
//! This test cross-checks each slice TR's const against the authored
//! `metadata/` set so the two cannot silently diverge — the const's `tr_code`
//! must be indexed, its `protocol` must match the index, and its rate-limit
//! `category` must match the per-TR `facets.rate_bucket`.

use std::path::PathBuf;

use ls_core::endpoint_policy::{
    CSPAQ12200_POLICY, REVOKE_POLICY, S3_POLICY, T1101_POLICY, T1102_POLICY, T1452_POLICY,
    T1531_POLICY, T1537_POLICY, T8412_POLICY, T8425_POLICY, T8436_POLICY, TOKEN_POLICY,
};
use ls_core::{EndpointPolicy, Protocol, RateLimitCategory};
use ls_metadata::{validate_dir, Protocol as MetaProtocol, RateBucket};

fn metadata_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("metadata")
}

fn protocol_matches(runtime: Protocol, meta: MetaProtocol) -> bool {
    matches!(
        (runtime, meta),
        (Protocol::Rest, MetaProtocol::Rest) | (Protocol::WebSocket, MetaProtocol::Websocket)
    )
}

fn category_matches(runtime: RateLimitCategory, meta: RateBucket) -> bool {
    matches!(
        (runtime, meta),
        (RateLimitCategory::MarketData, RateBucket::MarketData)
            | (RateLimitCategory::Orders, RateBucket::Orders)
            | (RateLimitCategory::Account, RateBucket::Account)
            | (RateLimitCategory::Auth, RateBucket::Auth)
    )
}

#[test]
fn slice_policies_mirror_metadata_index() {
    let report = validate_dir(&metadata_root())
        .unwrap_or_else(|e| panic!("metadata failed to validate: {e:?}"));

    let policies: &[EndpointPolicy] = &[
        TOKEN_POLICY,
        REVOKE_POLICY,
        T1101_POLICY,
        T1102_POLICY,
        T8412_POLICY,
        T8425_POLICY,
        T8436_POLICY,
        T1531_POLICY,
        T1537_POLICY,
        T1452_POLICY,
        CSPAQ12200_POLICY,
        S3_POLICY,
    ];

    for policy in policies {
        let entry = report.index.trs.get(policy.tr_code).unwrap_or_else(|| {
            panic!(
                "runtime const {}_POLICY has tr_code `{}` that is not in tr-index.yaml",
                policy.tr_code.to_uppercase(),
                policy.tr_code
            )
        });
        assert!(
            protocol_matches(policy.protocol, entry.protocol),
            "TR `{}`: runtime protocol {:?} disagrees with index protocol {:?}",
            policy.tr_code,
            policy.protocol,
            entry.protocol
        );

        let meta = report
            .trs
            .get(policy.tr_code)
            .expect("validated TR must have a per-TR record");
        assert!(
            category_matches(policy.category, meta.facets.rate_bucket),
            "TR `{}`: runtime rate category {:?} disagrees with facets.rate_bucket {:?}",
            policy.tr_code,
            policy.category,
            meta.facets.rate_bucket
        );

        // self_paginated ⟹ has_pagination: a TR whose result self-paginates MUST
        // have the policy thread the `tr_cont`/`tr_cont_key` continuation, else a
        // paginated TR ships with single-page dispatch silently. This is a one-way
        // implication, NOT equality: `CSPAQ12200` threads the header cursor
        // defensively (has_pagination=true) while its balance result is single-page
        // (self_paginated=false), and both are intentional.
        if meta.facets.self_paginated {
            assert!(
                policy.has_pagination,
                "TR `{}`: facets.self_paginated is true but runtime has_pagination is false \
                 — a self-paginating TR must thread continuation",
                policy.tr_code
            );
        }
    }
}
