//! Open-window WS track-flip wave (plan 2026-06-29-001): 39 connection-reachable-only rows.
//!
//! Wave-2a split out of `realtime/frame.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use ws_rows_open::*;` from `frame.rs`, so the explicit `pub use frame::{…}`
//! name list in `realtime/mod.rs` resolves transitively.
use super::*;


// === Open-window WS track-flip wave (plan 2026-06-29-001): 39
// connection-reachable-only realtime push rows. Structurally-unverified,
// provisional; metadata stays implemented:false until the U6 live sweep. ===

/// Decoded `AFR` (API사용자조건검색실시간) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AfrRow {
    /// `gsJobFlag` (raw wire field).
    #[serde(rename = "gsJobFlag", deserialize_with = "ls_core::string_or_number")]
    pub gs_job_flag: String,
    /// `gsVolume` (raw wire field).
    #[serde(rename = "gsVolume", deserialize_with = "ls_core::string_or_number")]
    pub gs_volume: String,
    /// `gsPrice` (raw wire field).
    #[serde(rename = "gsPrice", deserialize_with = "ls_core::string_or_number")]
    pub gs_price: String,
    /// `gsSign` (raw wire field).
    #[serde(rename = "gsSign", deserialize_with = "ls_core::string_or_number")]
    pub gs_sign: String,
    /// `gshname` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gshname: String,
    /// `gsChange` (raw wire field).
    #[serde(rename = "gsChange", deserialize_with = "ls_core::string_or_number")]
    pub gs_change: String,
    /// `gsChgRate` (raw wire field).
    #[serde(rename = "gsChgRate", deserialize_with = "ls_core::string_or_number")]
    pub gs_chg_rate: String,
    /// `gsCode` (raw wire field).
    #[serde(rename = "gsCode", deserialize_with = "ls_core::string_or_number")]
    pub gs_code: String,
}

/// Decoded `B7_` (ETF호가잔량) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct B7Row {
    /// `offerho4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho4: String,
    /// `offerho3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho3: String,
    /// `offerho6` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho6: String,
    /// `offerho5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho5: String,
    /// `offerho8` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho8: String,
    /// `offerho7` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho7: String,
    /// `offerho9` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho9: String,
    /// `lp_bidho5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lp_bidho5: String,
}

/// Decoded `C02` (KRX야간파생 선물체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct C02Row {
    /// `mem_filler` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mem_filler: String,
    /// `sihogagb` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sihogagb: String,
    /// `trcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trcode: String,
    /// `spdprc1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub spdprc1: String,
    /// `boardid` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub boardid: String,
    /// `spdprc2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub spdprc2: String,
    /// `seq` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub seq: String,
    /// `yakseq` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yakseq: String,
}

/// Decoded `CD0` (상품선물실시간상하한가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Cd0Row {
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `dy_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_gubun: String,
    /// `dy_uplmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_uplmtprice: String,
    /// `dy_dnlmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_dnlmtprice: String,
    /// `gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
}

/// Decoded `DBM` (KRX야간파생 투자자매매현황) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct DbmRow {
    /// `p_msval` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_msval: String,
    /// `tjjtime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjtime: String,
    /// `p_msvol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_msvol: String,
    /// `mdvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue: String,
    /// `fottjjcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fottjjcode: String,
    /// `msvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
    /// `tjjcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjcode: String,
    /// `msvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvalue: String,
}

/// Decoded `DBT` (KRX야간파생 투자자별현황) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct DbtRow {
    /// `mdvalue0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue0: String,
    /// `mdvalue1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue1: String,
    /// `msvolume8` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume8: String,
    /// `msvolume9` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume9: String,
    /// `msvolume4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume4: String,
    /// `mdvalue6` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue6: String,
    /// `msvolume5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume5: String,
    /// `mdvalue7` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue7: String,
}

/// Decoded `DC0` (KRX야간파생 체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Dc0Row {
    /// `date` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `mdchecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdchecnt: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `mschecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mschecnt: String,
    /// `ibasis` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ibasis: String,
    /// `mdvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// `cpower` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cpower: String,
}

/// Decoded `DD0` (KRX야간파생 실시간상하한가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Dd0Row {
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `dy_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_gubun: String,
    /// `dy_uplmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_uplmtprice: String,
    /// `dy_dnlmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_dnlmtprice: String,
    /// `gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
}

/// Decoded `DH0` (KRX야간파생 호가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Dh0Row {
    /// `offerrem2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem2: String,
    /// `offerho4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho4: String,
    /// `bidho5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho5: String,
    /// `offerho3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho3: String,
    /// `offerrem3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem3: String,
    /// `bidho4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho4: String,
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `offerrem4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem4: String,
}

/// Decoded `DH1` (KOSPI시간외단일가호가잔량) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Dh1Row {
    /// `dan_bidrem2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_bidrem2: String,
    /// `dan_bidrem1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_bidrem1: String,
    /// `dan_preychange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_preychange: String,
    /// `dan_totbidrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_totbidrem: String,
    /// `dan_jnilychange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_jnilychange: String,
    /// `dan_bidrem5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_bidrem5: String,
    /// `dan_totofferrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_totofferrem: String,
    /// `dan_bidrem4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_bidrem4: String,
}

/// Decoded `DHA` (KOSDAQ시간외단일가호가잔량) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct DhaRow {
    /// `dan_bidrem2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_bidrem2: String,
    /// `dan_bidrem1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_bidrem1: String,
    /// `dan_preychange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_preychange: String,
    /// `dan_totbidrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_totbidrem: String,
    /// `dan_jnilychange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_jnilychange: String,
    /// `dan_bidrem5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_bidrem5: String,
    /// `dan_totofferrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_totofferrem: String,
    /// `dan_bidrem4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_bidrem4: String,
}

/// Decoded `DK3` (KOSDAQ시간외단일가체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Dk3Row {
    /// `dan_value` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_value: String,
    /// `dan_high` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_high: String,
    /// `dan_mdvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_mdvolume: String,
    /// `dan_hightime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_hightime: String,
    /// `dan_mdchecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_mdchecnt: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `dan_precvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_precvolume: String,
    /// `dan_price` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_price: String,
}

/// Decoded `DS3` (KOSPI시간외단일가체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Ds3Row {
    /// `dan_value` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_value: String,
    /// `dan_high` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_high: String,
    /// `dan_mdvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_mdvolume: String,
    /// `dan_hightime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_hightime: String,
    /// `dan_mdchecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_mdchecnt: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `dan_precvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_precvolume: String,
    /// `dan_price` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_price: String,
}

/// Decoded `DVI` (시간외단일가VI발동해제) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct DviRow {
    /// `svi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svi_recprice: String,
    /// `vi_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vi_gubun: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `time` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// `vi_trgprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vi_trgprice: String,
    /// `dvi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dvi_recprice: String,
    /// `ref_shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ref_shcode: String,
}

/// Decoded `ESN` (뉴ELW투자지표민감도) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct EsnRow {
    /// `date` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// `ceta` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ceta: String,
    /// `elwclose` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwclose: String,
    /// `delt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub delt: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `rhox` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rhox: String,
}

/// Decoded `FX9` (KOSPI200선물가격제한폭확대) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Fx9Row {
    /// `upstep` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upstep: String,
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `uplmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// `dnstep` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnstep: String,
    /// `dnlmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
}

/// Decoded `H02` (KRX야간파생 선물정정취소) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct H02Row {
    /// `creditcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub creditcode: String,
    /// `mem_filler` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mem_filler: String,
    /// `qty2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub qty2: String,
    /// `trcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trcode: String,
    /// `mocagb` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mocagb: String,
    /// `price` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// `boardid` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub boardid: String,
    /// `accgb` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub accgb: String,
}

/// Decoded `H2_` (KOSPI장전시간외호가잔량) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct H2Row {
    /// `tmbidrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmbidrem: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `pretmoffercha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pretmoffercha: String,
    /// `pretmbidcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pretmbidcha: String,
    /// `tmofferrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmofferrem: String,
    /// `hotime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
}

/// Decoded `HB_` (KOSDAQ장전시간외호가잔량) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct HbRow {
    /// `tmbidrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmbidrem: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `pretmoffercha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pretmoffercha: String,
    /// `pretmbidcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pretmbidcha: String,
    /// `tmofferrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmofferrem: String,
    /// `hotime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
}

/// Decoded `I5_` (코스피ETF종목실시간NAV) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct I5Row {
    /// `jirate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jirate: String,
    /// `nav` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nav: String,
    /// `navchange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub navchange: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `grate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub grate: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `navdiff` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub navdiff: String,
}

/// Decoded `JX0` (주식선물가격제한폭확대) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Jx0Row {
    /// `upstep` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upstep: String,
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `uplmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// `dnstep` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnstep: String,
    /// `dnlmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
}

/// Decoded `NBM` ((NXT)업종별투자자별매매현황) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct NbmRow {
    /// `p_msval` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_msval: String,
    /// `tjjtime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjtime: String,
    /// `p_msvol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_msvol: String,
    /// `mdvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue: String,
    /// `msvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
    /// `ex_upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_upcode: String,
    /// `tjjcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjcode: String,
}

/// Decoded `NPM` ((NXT)프로그램매매전체집계) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct NpmRow {
    /// `sjvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sjvalue: String,
    /// `ex_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_gubun: String,
    /// `p_bdvalcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_bdvalcha: String,
    /// `p_cdvalcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_cdvalcha: String,
    /// `cwval` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cwval: String,
    /// `csjvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub csjvolume: String,
    /// `k200basis` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub k200basis: String,
    /// `p_cvolcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_cvolcha: String,
}

/// Decoded `NVI` ((NXT)VI 발동 해제) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct NviRow {
    /// `svi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svi_recprice: String,
    /// `vi_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vi_gubun: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `time` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// `vi_trgprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vi_trgprice: String,
    /// `exchname` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub exchname: String,
    /// `ex_shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
    /// `dvi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dvi_recprice: String,
}

/// Decoded `O02` (KRX야간파생 선물접수) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct O02Row {
    /// `grpId` (raw wire field).
    #[serde(rename = "grpId", deserialize_with = "ls_core::string_or_number")]
    pub grp_id: String,
    /// `execprc2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub execprc2: String,
    /// `execprc1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub execprc1: String,
    /// `trchno` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trchno: String,
    /// `fnoIsuptntp` (raw wire field).
    #[serde(rename = "fnoIsuptntp", deserialize_with = "ls_core::string_or_number")]
    pub fno_isuptntp: String,
    /// `trcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trcode: String,
    /// `fnobalevaltp` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fnobalevaltp: String,
    /// `avrprc_2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub avrprc_2: String,
}

/// Decoded `OX0` (KOSPI200옵션가격제한폭확대) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Ox0Row {
    /// `upstep` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upstep: String,
    /// `opttcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub opttcode: String,
    /// `uplmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// `dnstep` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnstep: String,
    /// `dnlmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
}

/// Decoded `SHC` (상/하한가근접진입) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ShcRow {
    /// `wgubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub wgubun: String,
    /// `dishonest` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dishonest: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `tgubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tgubun: String,
    /// `volume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// `sijanggubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sijanggubun: String,
}

/// Decoded `SHD` (상/하한가근접이탈) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ShdRow {
    /// `wgubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub wgubun: String,
    /// `dishonest` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dishonest: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `tgubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tgubun: String,
    /// `volume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// `sijanggubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sijanggubun: String,
}

/// Decoded `SHI` (상/하한가진입) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ShiRow {
    /// `wgubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub wgubun: String,
    /// `dishonest` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dishonest: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `updnlmtstime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub updnlmtstime: String,
    /// `tgubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tgubun: String,
    /// `volume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// Decoded `SHO` (상/하한가이탈) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ShoRow {
    /// `wgubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub wgubun: String,
    /// `dishonest` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dishonest: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `tgubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tgubun: String,
    /// `volume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// `sijanggubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sijanggubun: String,
}

/// Decoded `UBM` ((통합) 업종별투자자별매매현황) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct UbmRow {
    /// `p_msval` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_msval: String,
    /// `tjjtime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjtime: String,
    /// `p_msvol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_msvol: String,
    /// `mdvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue: String,
    /// `msvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
    /// `ex_upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_upcode: String,
    /// `tjjcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjcode: String,
}

/// Decoded `UBT` ((통합)시간대별투자자매매추이) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct UbtRow {
    /// `mdvalue0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue0: String,
    /// `mdvalue1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue1: String,
    /// `msvolume8` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume8: String,
    /// `msvolume9` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume9: String,
    /// `msvolume4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume4: String,
    /// `mdvalue6` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue6: String,
    /// `msvolume5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume5: String,
    /// `mdvalue7` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue7: String,
}

/// Decoded `UK1` ((통합)거래원) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Uk1Row {
    /// `tradmdrate1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate1: String,
    /// `tradmdvol5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol5: String,
    /// `tradmdvol3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol3: String,
    /// `tradmdrate3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate3: String,
    /// `tradmdrate2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate2: String,
    /// `tradmdvol4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol4: String,
    /// `offerno2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerno2: String,
    /// `tradmdrate5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate5: String,
}

/// Decoded `UVI` ((통합)VI발동해제) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct UviRow {
    /// `krx_time` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub krx_time: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `krx_svi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub krx_svi_recprice: String,
    /// `nxt_svi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nxt_svi_recprice: String,
    /// `ex_shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
    /// `krx_vi_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub krx_vi_gubun: String,
    /// `krx_dvi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub krx_dvi_recprice: String,
    /// `krx_vi_trgprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub krx_vi_trgprice: String,
}

/// Decoded `UYS` ((통합)예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct UysRow {
    /// `jnilysign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilysign: String,
    /// `ybidho0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ybidho0: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `yevolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yevolume: String,
    /// `ex_shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
    /// `ybidrem0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ybidrem0: String,
    /// `jnilydrate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilydrate: String,
    /// `yofferho0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yofferho0: String,
}

/// Decoded `YC3` (상품선물예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Yc3Row {
    /// `ychetime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ychetime: String,
    /// `jnilysign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilysign: String,
    /// `jnilchange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilchange: String,
    /// `yeprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yeprice: String,
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `yevolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yevolume: String,
    /// `jnilydrate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilydrate: String,
}

/// Decoded `YJC` (주식선물예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct YjcRow {
    /// `ychetime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ychetime: String,
    /// `jnilysign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilysign: String,
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `jnilchange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilchange: String,
    /// `yeprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yeprice: String,
    /// `jnilydrate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilydrate: String,
    /// `expct_ccls_q` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expct_ccls_q: String,
}

/// Decoded `YJ_` (예상지수) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct YjRow {
    /// `jisu` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
    /// `volume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// `drate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub drate: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `time` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// `value` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// Decoded `h3_` (ELW호가잔량) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object `body`) for the open-window WS track-flip wave; this channel
/// is connection-reachable-only, so no live row is observed. A representative,
/// spec-grounded subset of the push row. Every field is `string_or_number`-coerced
/// and `#[serde(default)]` so both wire shapes — and a sparse registration-ACK
/// body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct H3Row {
    /// `offerho4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho4: String,
    /// `offerho3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho3: String,
    /// `offerho6` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho6: String,
    /// `offerho5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho5: String,
    /// `offerho8` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho8: String,
    /// `offerho7` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho7: String,
    /// `offerho9` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho9: String,
    /// `lp_bidho5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lp_bidho5: String,
}
