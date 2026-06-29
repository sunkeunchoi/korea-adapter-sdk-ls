//! Order/account realtime event push rows (`AS*`/`SC*`/`TC*`/`C01`/`H01`/`O01`).
//!
//! Wave-2a split out of `realtime/frame.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use ws_events::*;` from `frame.rs`, so the explicit `pub use frame::{…}`
//! name list in `realtime/mod.rs` resolves transitively.
use super::*;


// =============================================================================
// P2 order-event lane: 주문/체결 event push rows (observation-only).
//
// These channels register with tr_type "1" (계좌등록) / deregister "2"
// (WsLane::OrderEvent). Per KTD6 they are NOT-OBSERVABLE on bare paper, so a
// clean lifecycle proves connection reachability only. Every <Base>Event struct
// is a SINGLE-object out-block (verified against its migration-source
// <Base>Response — each is a bare struct, not a Vec) and carries a
// representative, spec-grounded subset of order/execution fields with the real
// LS wire names (+ any #[serde(rename)] the source used). Account number and
// password fields are DELIBERATELY OMITTED (never test-surface a secret-shaped
// field). Every field is string_or_number-coerced and #[serde(default)] so a
// sparse registration-ACK body decodes without aborting the stream.
// =============================================================================

/// Decoded SC0 (주식 주문접수 / stock order-accept) realtime push row.
///
/// **Structurally-verified** against the migration source's `SC0Response`
/// (`korea-broker-sdk-ls` `generated/stock.rs`): a single-object out-block.
/// Account/password header fields are omitted; this is the order-identity subset.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC0Event {
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Original order number / 원주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub orgordno: String,
    /// Order/execution classification / 주문체결구분 (01:주문 11:체결 14:거부 …).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordchegb: String,
    /// Order classification / 주문구분 (01:현금매도 02:현금매수 …).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordgb: String,
    /// Short stock code / 단축종목번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shtcode: String,
    /// Stock name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Order qty / 주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordqty: String,
    /// Order price / 주문가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordprice: String,
}

/// Decoded SC1 (주식 주문체결 / stock order-fill) realtime push row.
///
/// **Structurally-verified** against the migration source's `SC1Response`
/// (`generated/stock.rs`): a single-object out-block. Preserves the source's
/// `shtnIsuno`/`Isunm` wire renames. Account fields omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC1Event {
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Execution number / 체결번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub execno: String,
    /// Short stock code / 단축종목번호.
    #[serde(rename = "shtnIsuno", deserialize_with = "ls_core::string_or_number")]
    pub shtnisuno: String,
    /// Stock name / 종목명.
    #[serde(rename = "Isunm", deserialize_with = "ls_core::string_or_number")]
    pub isunm: String,
    /// Order qty / 주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordqty: String,
    /// Order price / 주문가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordprc: String,
    /// Execution qty / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub execqty: String,
    /// Execution price / 체결가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub execprc: String,
}

/// Decoded SC2 (주식 주문정정 / stock order-amend) realtime push row.
///
/// **Structurally-verified, no documented body fields** — the migration source's
/// `SC2Response` is a bare struct (single-object out-block; the gateway carries
/// the registration-ACK header only). Decodes any sparse body without aborting.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC2Event {}

/// Decoded SC3 (주식 주문취소 / stock order-cancel) realtime push row.
///
/// **Structurally-verified, no documented body fields** — bare-struct sibling of
/// `SC2Event`, mirroring the migration source's empty `SC3Response`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC3Event {}

/// Decoded SC4 (주식 주문거부 / stock order-reject) realtime push row.
///
/// **Structurally-verified, no documented body fields** — bare-struct sibling of
/// `SC2Event`, mirroring the migration source's empty `SC4Response`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC4Event {}

/// Decoded C01 (선물 주문체결 / F-O order-fill) realtime push row.
///
/// **Structurally-verified** against the migration source's `C01Response`
/// (`generated/futures_options.rs`): a single-object out-block. Account fields
/// omitted; this is the order/execution-identity subset.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct C01Event {
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Original order number / 원주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordordno: String,
    /// Item code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Execution price / 체결가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cheprice: String,
    /// Execution qty / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chevol: String,
    /// Execution date / 체결일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chedate: String,
    /// Execution time / 체결시각.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Sell/buy classification / 매도수구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dosugb: String,
}

/// Decoded O01 (선물 접수 / F-O order-accept) realtime push row.
///
/// **Structurally-verified, no documented body fields** — the migration source's
/// `O01Response` is a bare struct (single-object out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct O01Event {}

/// Decoded H01 (선물 주문정정취소 / F-O order-amend-cancel) realtime push row.
///
/// **Structurally-verified, no documented body fields** — the migration source's
/// `H01Response` is a bare struct (single-object out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct H01Event {}

/// Decoded AS0 (해외주식 주문접수(미국) / overseas-stock order-accept) push row.
///
/// **Structurally-verified** against the migration source's `AS0Response`
/// (`generated/overseas_stock.rs`): a single-object out-block carrying `s`-prefixed
/// wire names (renames preserved). Account fields omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS0Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Original order number / 원주문번호.
    #[serde(rename = "sOrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sorgordno: String,
    /// Short item number / 단축종목번호.
    #[serde(rename = "sShtnIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sshtnisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
}

/// Decoded AS1 (해외주식 주문체결(미국) / overseas-stock order-fill) push row.
///
/// **Structurally-verified** against the migration source's `AS1Response`
/// (`generated/overseas_stock.rs`): a single-object out-block; `s`-prefixed wire
/// renames preserved. Adds the execution fields over `AS0Event`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS1Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Short item number / 단축종목번호.
    #[serde(rename = "sShtnIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sshtnisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
    /// Execution number / 체결번호.
    #[serde(rename = "sExecNO", deserialize_with = "ls_core::string_or_number")]
    pub sexecno: String,
    /// Execution qty / 체결수량.
    #[serde(rename = "sExecQty", deserialize_with = "ls_core::string_or_number")]
    pub sexecqty: String,
    /// Execution price / 체결가.
    #[serde(rename = "sExecPrc", deserialize_with = "ls_core::string_or_number")]
    pub sexecprc: String,
}

/// Decoded AS2 (해외주식 주문정정(미국) / overseas-stock order-amend) push row.
///
/// **Structurally-verified** against the migration source's `AS2Response`
/// (`generated/overseas_stock.rs`): a single-object out-block; `s`-prefixed renames
/// preserved (note `sIsuNo`, not `sShtnIsuNo`, on the amend/cancel/reject feeds).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS2Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Original order number / 원주문번호.
    #[serde(rename = "sOrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sorgordno: String,
    /// Item number / 종목번호.
    #[serde(rename = "sIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
}

/// Decoded AS3 (해외주식 주문취소(미국) / overseas-stock order-cancel) push row.
///
/// **Structurally-verified** against the migration source's `AS3Response`
/// (`generated/overseas_stock.rs`): a single-object out-block; schema-sibling of
/// `AS2Event` (`sIsuNo` short-code rename).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS3Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Original order number / 원주문번호.
    #[serde(rename = "sOrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sorgordno: String,
    /// Item number / 종목번호.
    #[serde(rename = "sIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
}

/// Decoded AS4 (해외주식 주문거부(미국) / overseas-stock order-reject) push row.
///
/// **Structurally-verified** against the migration source's `AS4Response`
/// (`generated/overseas_stock.rs`): a single-object out-block; schema-sibling of
/// `AS2Event` (`sIsuNo` short-code rename).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS4Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Original order number / 원주문번호.
    #[serde(rename = "sOrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sorgordno: String,
    /// Item number / 종목번호.
    #[serde(rename = "sIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
}

/// Decoded TC1 (해외선물 주문접수 / overseas-futures order-accept) push row.
///
/// **Structurally-verified** against the migration source's `TC1Response`
/// (`generated/overseas_futures.rs`): a single-object out-block with snake_case
/// wire names (no renames). Account number (`ac_no`) omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct TC1Event {
    /// Service id / 서비스ID (HO01:주문ACK HO04:주문Pending).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svc_id: String,
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_no: String,
    /// Original order number / 원주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub orgn_ordr_no: String,
    /// Item code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub is_cd: String,
    /// Sell/buy type / 매도매수유형 (1:매도 2:매수).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub s_b_ccd: String,
    /// Amend/cancel type / 정정취소유형 (1:신규 2:정정 3:취소).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_ccd: String,
    /// Order price / 주문가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_prc: String,
    /// Order qty / 주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_q: String,
    /// Order time / 주문시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_tm: String,
}

/// Decoded TC2 (해외선물 주문응답 / overseas-futures order-response) push row.
///
/// **Structurally-verified** against the migration source's `TC2Response`
/// (`generated/overseas_futures.rs`): a single-object out-block; adds the
/// confirm/reject fields over `TC1Event`. Account number omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct TC2Event {
    /// Service id / 서비스ID (HO02:확인 HO03:거부).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svc_id: String,
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_no: String,
    /// Original order number / 원주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub orgn_ordr_no: String,
    /// Item code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub is_cd: String,
    /// Amend/cancel type / 정정취소유형.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_ccd: String,
    /// Order price / 주문가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_prc: String,
    /// Order qty / 주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_q: String,
    /// Quote-confirm qty / 호가확인수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnfr_q: String,
    /// Quote-reject reason code / 호가거부사유코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rfsl_cd: String,
}

/// Decoded TC3 (해외선물 주문체결 / overseas-futures order-fill) push row.
///
/// **Structurally-verified** against the migration source's `TC3Response`
/// (`generated/overseas_futures.rs`): a single-object out-block; the execution
/// (체결) subset. Account number omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct TC3Event {
    /// Service id / 서비스ID (CH01).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svc_id: String,
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_no: String,
    /// Item code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub is_cd: String,
    /// Sell/buy type / 매도매수유형.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub s_b_ccd: String,
    /// Execution qty / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ccls_q: String,
    /// Execution price / 체결가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ccls_prc: String,
    /// Execution number / 체결번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ccls_no: String,
    /// Execution time / 체결시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ccls_tm: String,
    /// Current price / 현재가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub now_prc: String,
}
