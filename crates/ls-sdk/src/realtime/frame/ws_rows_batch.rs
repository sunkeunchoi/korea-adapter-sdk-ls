//! Closure-flip WS batch (plan -004): 31 connection-reachable-only realtime rows.
//!
//! Wave-2a split out of `realtime/frame.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use ws_rows_batch::*;` from `frame.rs`, so the explicit `pub use frame::{…}`
//! name list in `realtime/mod.rs` resolves transitively.
use super::*;


// === Closure-flip WS batch (plan -004): 31 connection-reachable-only realtime
// push rows. Structurally-unverified, provisional — modelled from raw res_example
// single-object bodies; no live row observed (KTD1). ===

/// Decoded `NS3` ((NXT)체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Ns3Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `mdchecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdchecnt: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `mschecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mschecnt: String,
    /// `mdvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// `w_avrg` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub w_avrg: String,
    /// `cpower` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cpower: String,
    /// `offerho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
}

/// Decoded `NH1` ((NXT)호가잔량) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Nh1Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
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
}

/// Decoded `NS2` ((NXT)우선호가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Ns2Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `bidho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho: String,
    /// `offerho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
    /// `ex_shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
}

/// Decoded `NK1` ((NXT)거래원) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Nk1Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
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
}

/// Decoded `NBT` ((NXT)시간대별투자자매매추이) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct NbtRow {
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
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
}

/// Decoded `KS_` (KOSDAQ우선호가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct KsRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `bidho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho: String,
    /// `offerho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
}

/// Decoded `OK_` (KOSDAQ거래원) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct OkRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
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
}

/// Decoded `KH_` (KOSDAQ프로그램매매종목별) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct KhRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `bshrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bshrem: String,
    /// `cshvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cshvolume: String,
    /// `swcvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub swcvolume: String,
    /// `tsvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tsvolume: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `dwcvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dwcvolume: String,
    /// `djcvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub djcvalue: String,
}

/// Decoded `KM_` (KOSDAQ프로그램매매전체집계) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct KmRow {
    /// `gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
    /// `sjvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sjvalue: String,
    /// `p_bdvalcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_bdvalcha: String,
    /// `p_cdvalcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_cdvalcha: String,
    /// `k50sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub k50sign: String,
    /// `cwval` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cwval: String,
    /// `csjvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub csjvolume: String,
    /// `p_cvolcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_cvolcha: String,
}

/// Decoded `PH_` (KOSPI프로그램매매종목별) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct PhRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `bshrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bshrem: String,
    /// `cshvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cshvolume: String,
    /// `swcvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub swcvolume: String,
    /// `tsvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tsvolume: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `dwcvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dwcvolume: String,
    /// `djcvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub djcvalue: String,
}

/// Decoded `K1_` (KOSPI거래원) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct K1Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
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
}

/// Decoded `IJ_` (지수) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct IjRow {
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `cvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// `jisu` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
    /// `highjisu` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highjisu: String,
    /// `upjo` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upjo: String,
    /// `highjo` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highjo: String,
    /// `value` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// Decoded `YS3` (KOSPI예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Ys3Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `jnilysign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilysign: String,
    /// `yofferrem0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yofferrem0: String,
    /// `jnilchange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilchange: String,
    /// `yeprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yeprice: String,
    /// `ybidho0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ybidho0: String,
    /// `yevolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yevolume: String,
    /// `hotime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
}

/// Decoded `YK3` (KOSDAQ예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Yk3Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `jnilysign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilysign: String,
    /// `yofferrem0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yofferrem0: String,
    /// `jnilchange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilchange: String,
    /// `yeprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yeprice: String,
    /// `ybidho0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ybidho0: String,
    /// `yevolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yevolume: String,
    /// `hotime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
}

/// Decoded `VI_` (VI발동해제) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ViRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `svi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svi_recprice: String,
    /// `vi_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vi_gubun: String,
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

/// Decoded `JC0` (주식선물체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Jc0Row {
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
    /// `cvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
}

/// Decoded `JH0` (주식선물호가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Jh0Row {
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
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
}

/// Decoded `JD0` (주식선물실시간상하한가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Jd0Row {
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

/// Decoded `FD0` (KOSPI200선물실시간상하한가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Fd0Row {
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

/// Decoded `OD0` (KOSPI200옵션실시간상하한가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Od0Row {
    /// `opttcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub opttcode: String,
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

/// Decoded `OMG` (KOSPI200옵션민감도) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct OmgRow {
    /// `optcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
    /// `ceta` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ceta: String,
    /// `bidimpv` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidimpv: String,
    /// `fut200jisu` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fut200jisu: String,
    /// `delt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub delt: String,
    /// `rhox` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rhox: String,
    /// `chetime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// `price` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
}

/// Decoded `YF9` (지수선물예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Yf9Row {
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
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
    /// `jnilydrate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilydrate: String,
    /// `expct_ccls_q` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expct_ccls_q: String,
}

/// Decoded `YOC` (지수옵션예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct YocRow {
    /// `optcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
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
    /// `jnilydrate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilydrate: String,
    /// `expct_ccls_q` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expct_ccls_q: String,
}

/// Decoded `BM_` (업종별투자자별매매현황) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct BmRow {
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
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
    /// `tjjcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjcode: String,
    /// `msvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvalue: String,
}

/// Decoded `WOC` (해외옵션 체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct WocRow {
    /// `symbol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// `chgrate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chgrate: String,
    /// `kordate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub kordate: String,
    /// `trdtm` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trdtm: String,
    /// `curpr` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub curpr: String,
    /// `ovsdate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ovsdate: String,
    /// `mdvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// `ydiffpr` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ydiffpr: String,
}

/// Decoded `WOH` (해외옵션 호가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct WohRow {
    /// `symbol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
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
    /// `bidno1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidno1: String,
}

/// Decoded `JIF` (장운영정보) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct JifRow {
    /// `jangubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jangubun: String,
    /// `jstatus` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jstatus: String,
}

/// Decoded `NWS` (실시간뉴스제목패킷) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct NwsRow {
    /// `code` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub code: String,
    /// `date` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// `realkey` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub realkey: String,
    /// `bodysize` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bodysize: String,
    /// `time` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// `id` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub id: String,
    /// `title` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub title: String,
}

/// Decoded `BMT` (시간대별투자자매매추이) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct BmtRow {
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
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
}

/// Decoded `CUR` (현물정보USD실시간) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct CurRow {
    /// `base_id` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub base_id: String,
    /// `offer` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer: String,
    /// `high` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// `drate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub drate: String,
    /// `low` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// `price` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
}

/// Decoded `MK2` (US지수) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Mk2Row {
    /// `xsymbol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub xsymbol: String,
    /// `date` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `bidrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem: String,
    /// `offerho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
    /// `cvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// `offerrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem: String,
}
