//! Account dependency-class tests (`CSPAQ12200`, `CSPAQ12300`, `CSPAQ22200`,
//! `CFOBQ10500`).
//!
//! All four are read-only account-state reads sharing the same discipline: the
//! request is built from the CONFIG-supplied account, never a caller identifier,
//! and the account number never appears in the body. They differ only in request
//! shape — `CSPAQ12200` (single `BalCreTp`), `CSPAQ12300` (four query-shape
//! enums), `CSPAQ22200` (single `BalCreTp`), and `CFOBQ10500` (empty in-block,
//! three out-blocks) — and endpoint (`/stock/accno` vs `CFOBQ10500`'s
//! `/futureoption/accno`).
//!
//! The defining facet is `account_state: true`, so the Change-Scoped Gate selects
//! ONLY credential-free request-construction tests for these TRs. These tests prove:
//!   - the request constructs from the CONFIG-supplied account (never a caller
//!     identifier) with `BalCreTp`, serializing to `{"CSPAQ12200InBlock1":{...}}`
//!     WITHOUT a network call,
//!   - the response deserializes from the spec-derived, SYNTHETIC fixture with the
//!     key balance fields (`MnyOrdAbleAmt`, `BalEvalAmt`, …) asserted,
//!   - `CSPAQ12200OutBlock2` tolerates a single object via `de_vec_or_single`,
//!   - and that `01715` (date) and `01900` (paper-incompatible) classify DISTINCTLY
//!     via the structured `rsp_cd`.
//!
//! No credentialed live call is attempted: credentialed evidence is scheduled
//! separately and is out of the unit suite. The wiremock-backed deserialize test
//! exercises real `ls-core` dispatch against a MOCK token + MOCK response — it uses
//! the dummy `TEST_ACCOUNT_NO` from `mock_config`, never a real account.

use std::sync::Arc;

use ls_core::{Inner, LsError};
use ls_sdk::account::{
    CCENQ10100Request, CCENQ10100Response, CCENQ90200Request, CCENQ90200Response, CFOAQ10100Request,
    CFOAQ10100Response, CFOBQ10500Request, CFOBQ10500Response, CSPAQ12200Request, CSPAQ12200Response,
    CSPAQ12300Request, CSPAQ12300Response, CSPAQ22200Request, CSPAQ22200Response,
    CSPBQ00200Request, CSPBQ00200Response, T0424Request, T0424Response,
    CLNAQ00100Request, CLNAQ00100Response, CFOEQ11100Request, CFOEQ11100Response,
    T0441Request, T0441Response, CIDBQ01400Request, CIDBQ01400Response,
    CIDBQ03000Request, CIDBQ03000Response, CIDBQ05300Request, CIDBQ05300Response,
};
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{mock_config, mount_token, TEST_ACCOUNT_NO};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The spec-derived, SYNTHETIC `CSPAQ12200` response fixture.
const CSPAQ12200_FIXTURE: &str = include_str!("fixtures/CSPAQ12200_resp.json");

/// The spec-derived, SYNTHETIC `CSPAQ12300` response fixture.
const CSPAQ12300_FIXTURE: &str = include_str!("fixtures/CSPAQ12300_resp.json");

/// The spec-derived, SYNTHETIC `CSPAQ22200` response fixture.
const CSPAQ22200_FIXTURE: &str = include_str!("fixtures/CSPAQ22200_resp.json");

/// The spec-derived, SYNTHETIC `CFOBQ10500` response fixture.
const CFOBQ10500_FIXTURE: &str = include_str!("fixtures/CFOBQ10500_resp.json");

/// The spec-derived, SYNTHETIC `CCENQ90200` response fixture.
const CCENQ90200_FIXTURE: &str = include_str!("fixtures/CCENQ90200_resp.json");

/// The spec-derived, SYNTHETIC `CFOAQ10100` response fixture.
const CFOAQ10100_FIXTURE: &str = include_str!("fixtures/CFOAQ10100_resp.json");

/// The spec-derived, SYNTHETIC `CCENQ10100` response fixture.
const CCENQ10100_FIXTURE: &str = include_str!("fixtures/CCENQ10100_resp.json");

/// The spec-derived, SYNTHETIC `t0424` response fixture (cash summary + one holding).
const T0424_FIXTURE: &str = include_str!("fixtures/t0424_resp.json");

/// The spec-derived, SYNTHETIC `CSPBQ00200` response fixture (capacity block).
const CSPBQ00200_FIXTURE: &str = include_str!("fixtures/CSPBQ00200_resp.json");

/// The spec-derived, SYNTHETIC `CLNAQ00100` response fixture (loanable-stock list).
const CLNAQ00100_FIXTURE: &str = include_str!("fixtures/CLNAQ00100_resp.json");

/// The spec-derived, SYNTHETIC `CFOEQ11100` response fixture (F/O deposit detail).
const CFOEQ11100_FIXTURE: &str = include_str!("fixtures/CFOEQ11100_resp.json");

/// The spec-derived, SYNTHETIC `t0441` response fixture (F/O balance valuation).
const T0441_FIXTURE: &str = include_str!("fixtures/t0441_resp.json");

/// The spec-derived, SYNTHETIC `CIDBQ01400` response fixture (overseas order-qty).
const CIDBQ01400_FIXTURE: &str = include_str!("fixtures/CIDBQ01400_resp.json");

/// The spec-derived, SYNTHETIC `CIDBQ03000` response fixture (overseas deposit/balance).
const CIDBQ03000_FIXTURE: &str = include_str!("fixtures/CIDBQ03000_resp.json");

/// The spec-derived, SYNTHETIC `CIDBQ05300` response fixture (overseas deposited assets).
const CIDBQ05300_FIXTURE: &str = include_str!("fixtures/CIDBQ05300_resp.json");

/// The shared REST path for the `/futureoption/accno` account TRs (`CFOBQ10500`,
/// `CCENQ90200`, `CFOAQ10100`, `CCENQ10100`) — they mount the same endpoint and
/// discriminate on the `tr_cd` header.
const FUTUREOPTION_ACCNO_PATH: &str = "/futureoption/accno";

/// The shared REST path for the `/stock/accno` account TRs (`CSPAQ12200`,
/// `CSPAQ12300`, `CSPAQ22200`) — they mount the same endpoint and discriminate
/// on the `tr_cd` header. (`CFOBQ10500` uses `/futureoption/accno`, spelled
/// inline in its test.)
const STOCK_ACCNO_PATH: &str = "/stock/accno";

/// Build an `LsSdk` whose dispatch is pointed at the mock server.
fn sdk_for(server: &MockServer) -> LsSdk {
    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    LsSdk::from_inner(inner)
}

#[path = "account/balance.rs"]
mod balance;
#[path = "account/capacity.rs"]
mod capacity;
#[path = "account/holdings.rs"]
mod holdings;
