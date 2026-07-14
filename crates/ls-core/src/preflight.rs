//! Preflight request validation (error-resilience gate U2/U3, R6/R7).
//!
//! A TR earns a preflight schema by carrying a `metadata/constraints/<tr>.yaml`
//! file. Those files are embedded at build time ([`crate::embedded`]) and parsed
//! once here into a lookup keyed by `tr_code`. Before any network call, the
//! dispatch seam ([`crate::inner::Inner`]) serializes the typed request to a
//! `serde_json::Value` and runs [`validate_request`] against the TR's schema; a
//! violation short-circuits with [`LsError::Invalid`] and issues **no** HTTP
//! request.
//!
//! ## Confirmed-vs-permissive (R6)
//!
//! Preflight blocks only constraints whose accepted bound is *positively
//! confirmed*. Type and required-ness are grounded structurally against the
//! normalized baseline offline (KTD5), so they always block. Enum / range /
//! format bounds carry a `confirmed` flag defaulting to `false`: until the
//! differential live probe (R10) confirms the bound, the field is **permissive**
//! — the request proceeds and any rejection surfaces as an explained gateway
//! error rather than a false local reject. A false-reject silently breaks a
//! caller's valid request with no detector, so blocking is the earned state.
//!
//! This module holds ls-core's own copy of the constraint types. `ls-metadata`
//! carries a parallel `ConstraintSchema` used for offline grounding, validation,
//! and docgen; the shared YAML file is the contract between them. ls-core cannot
//! depend on ls-metadata at runtime (it ships to consumers), so the duplication
//! is deliberate.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::LsError;

/// The declared wire type of a request field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// Free-text / opaque string (the LS default model).
    String,
    /// Integer-valued (baseline `Number`, whole).
    Integer,
    /// Fractional-valued (baseline `Number`, decimal — e.g. a price).
    Number,
}

/// The allowed-enum input class for a field (R7). `applicable: false` is the
/// explicit N/A marker (R5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumRule {
    /// Whether an enum constraint applies to this field at all.
    pub applicable: bool,
    /// The accepted value set. Empty when `applicable` is false.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// Whether the accepted set is positively confirmed (R10). Permissive until so.
    #[serde(default)]
    pub confirmed: bool,
}

/// The out-of-range input class for a field (R7). Bounds are inclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeRule {
    /// Whether a range constraint applies to this field at all.
    pub applicable: bool,
    /// Inclusive lower bound, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    /// Inclusive upper bound, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    /// Whether the bound is positively confirmed (R10). Permissive until so.
    #[serde(default)]
    pub confirmed: bool,
}

/// A recognised value format for the malformed-symbol/date class (R7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatKind {
    /// A non-empty instrument symbol (alphanumeric).
    Symbol,
    /// An 8-digit `YYYYMMDD` date.
    Date,
}

/// The malformed-format input class for a field (R7). `applicable: false` is the
/// explicit N/A marker (R5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatRule {
    /// Whether a format constraint applies to this field at all.
    pub applicable: bool,
    /// The required format, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<FormatKind>,
    /// Whether the format is positively confirmed (R10). Permissive until so.
    #[serde(default)]
    pub confirmed: bool,
}

/// One request field's declared constraints across every input class. `enum`,
/// `range`, and `format` are always present (non-optional) so an inapplicable
/// class must be explicitly marked N/A (`applicable: false`) — exhaustiveness is
/// auditable rather than inferred from silence (R5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldConstraint {
    /// The request field name as it appears on the wire.
    pub name: String,
    /// The declared wire type (grounded against the baseline — always blocks).
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// Whether the field must be present and non-empty (grounded — always blocks).
    pub required: bool,
    /// Allowed-enum class (R7).
    #[serde(rename = "enum")]
    pub enum_rule: EnumRule,
    /// Out-of-range class (R7).
    pub range: RangeRule,
    /// Malformed-symbol/date class (R7).
    pub format: FormatRule,
    /// Input classes (`"required"`, `"format"`, …) whose accepted violation the
    /// gateway is known to tolerate for this field. The differential probe treats
    /// an accepted violation of a listed class as an expected outcome, not a
    /// divergence — it does **not** relax preflight (a `required:true` field still
    /// fails preflight when omitted, regardless of this facet). Empty = none
    /// (backward-compatible default). See plan 2026-07-06-002 / KTD2-KTD3.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gateway_tolerant: Vec<String>,
    /// Per-input-class → gateway-code **placed-nothing** allowlist for the ORDER
    /// negative probe (Route B, plan 2026-07-14-001). A field may declare that a
    /// given class's violation, when the gateway answers a specific `rsp_cd` at
    /// `http=500`, structurally **placed nothing** — admitting an undocumented,
    /// success-shaped code (`IGW00000`) as a placed-nothing reject for that exact
    /// `(field, class, code)` triple only. The order-path analogue of
    /// [`Self::gateway_tolerant`], but on the may-rest→placed-nothing axis rather
    /// than the divergence axis. Deliberately scoped and dormant: it is consulted
    /// only at the order-probe fire site (`negative_probe.rs`), and it does **not**
    /// touch the runtime seam ([`crate::error_catalog::is_ingress_validation_reject`]),
    /// so a live caller still gets the may-rest `AmbiguousOrder → reconcile via
    /// t0425` treatment (KTD4). Empty/absent = none (backward-compatible default);
    /// every existing constraint file deserializes unchanged.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub placed_nothing_codes: BTreeMap<String, Vec<String>>,
}

/// A cross-field / combination-invalidity rule (R7): fields individually valid
/// but jointly rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CrossFieldRule {
    /// `start` must be chronologically <= `end` (both `YYYYMMDD`). Blocks only
    /// when `confirmed`.
    DateOrder {
        /// The start-date field name.
        start: String,
        /// The end-date field name.
        end: String,
        /// Whether the ordering is positively confirmed (R10).
        #[serde(default)]
        confirmed: bool,
        /// Whether the gateway is known to tolerate an accepted violation of this
        /// cross-field ordering (the `cross_field` analogue of a field's
        /// `gateway_tolerant` list). The differential probe treats an accepted
        /// start>end as an expected outcome, not a divergence — it does **not**
        /// relax preflight (a `confirmed` ordering still blocks locally). Defaults
        /// to `false` (backward-compatible). See §30 (t8412 `sdate/edate`).
        #[serde(default)]
        gateway_tolerant: bool,
    },
}

/// A per-TR declarative request-field constraint schema
/// (`metadata/constraints/<tr>.yaml`). The single source from which preflight
/// validation, the negative probe, and the Reference "Errors & validation"
/// section are derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintSchema {
    /// The TR this schema constrains.
    pub tr_code: String,
    /// Per-field constraints.
    pub fields: Vec<FieldConstraint>,
    /// Cross-field / combination rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_field: Vec<CrossFieldRule>,
}

/// A located preflight failure: the offending `field` and the human `reason`.
/// Converts into [`LsError::Invalid`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightError {
    /// The request field (or cross-field rule) that failed.
    pub field: String,
    /// Why it failed, in caller-fixable terms.
    pub reason: String,
}

impl From<PreflightError> for LsError {
    fn from(e: PreflightError) -> Self {
        LsError::Invalid {
            field: e.field,
            reason: e.reason,
        }
    }
}

/// Extract the scalar textual value of a request field, coercing a JSON number to
/// its textual form (request fields serialize as strings, or as numbers via
/// `string_as_number`). Returns `None` for absent / null / non-scalar values.
fn scalar(value: &serde_json::Value, field: &str) -> Option<String> {
    match value.get(field) {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

/// `true` if the field is present and non-empty (an empty string counts as absent
/// for required-ness — LS encodes an omitted field as `""`).
fn present(value: &serde_json::Value, field: &str) -> bool {
    match scalar(value, field) {
        Some(s) => !s.is_empty(),
        None => false,
    }
}

/// Validate a serialized request `value` against `schema`, returning the first
/// located [`PreflightError`]. Only positively-confirmed bounds block (R6);
/// type and required-ness always block (structurally grounded, KTD5).
pub fn validate_request(
    schema: &ConstraintSchema,
    value: &serde_json::Value,
) -> Result<(), PreflightError> {
    for field in &schema.fields {
        validate_field(field, value)?;
    }
    for rule in &schema.cross_field {
        validate_cross_field(rule, value)?;
    }
    Ok(())
}

fn validate_field(
    field: &FieldConstraint,
    value: &serde_json::Value,
) -> Result<(), PreflightError> {
    // Required-ness (always blocks — grounded).
    if field.required && !present(value, &field.name) {
        return Err(PreflightError {
            field: field.name.clone(),
            reason: "is required but was missing or empty".to_string(),
        });
    }

    let Some(scalar_value) = scalar(value, &field.name) else {
        // Absent optional field: nothing else to check.
        return Ok(());
    };
    if scalar_value.is_empty() {
        return Ok(());
    }

    // Type (always blocks — grounded).
    match field.field_type {
        FieldType::String => {}
        FieldType::Integer => {
            if scalar_value.parse::<i64>().is_err() {
                return Err(PreflightError {
                    field: field.name.clone(),
                    reason: format!("must be an integer, got `{scalar_value}`"),
                });
            }
        }
        FieldType::Number => {
            if scalar_value.parse::<f64>().is_err() {
                return Err(PreflightError {
                    field: field.name.clone(),
                    reason: format!("must be a number, got `{scalar_value}`"),
                });
            }
        }
    }

    // Enum (blocks only when confirmed).
    if field.enum_rule.applicable
        && field.enum_rule.confirmed
        && !field.enum_rule.values.iter().any(|v| v == &scalar_value)
    {
        return Err(PreflightError {
            field: field.name.clone(),
            reason: format!(
                "must be one of [{}], got `{scalar_value}`",
                field.enum_rule.values.join(", ")
            ),
        });
    }

    // Range (blocks only when confirmed; numeric fields only). Parse as f64 so a
    // fractional value on a `Number` field (e.g. an F/O price) is enforced too —
    // an i64-only parse would silently skip the check on any decimal value. Bounds
    // are declared as integers but compared in f64 space.
    if field.range.applicable && field.range.confirmed {
        if let Ok(n) = scalar_value.parse::<f64>() {
            if let Some(min) = field.range.min {
                if n < min as f64 {
                    return Err(PreflightError {
                        field: field.name.clone(),
                        reason: format!("must be >= {min}, got {scalar_value}"),
                    });
                }
            }
            if let Some(max) = field.range.max {
                if n > max as f64 {
                    return Err(PreflightError {
                        field: field.name.clone(),
                        reason: format!("must be <= {max}, got {scalar_value}"),
                    });
                }
            }
        }
    }

    // Format (blocks only when confirmed).
    if field.format.applicable && field.format.confirmed {
        if let Some(kind) = field.format.kind {
            let ok = match kind {
                FormatKind::Symbol => {
                    !scalar_value.is_empty() && scalar_value.chars().all(|c| c.is_alphanumeric())
                }
                FormatKind::Date => {
                    scalar_value.len() == 8 && scalar_value.chars().all(|c| c.is_ascii_digit())
                }
            };
            if !ok {
                let expect = match kind {
                    FormatKind::Symbol => "an alphanumeric symbol",
                    FormatKind::Date => "an 8-digit YYYYMMDD date",
                };
                return Err(PreflightError {
                    field: field.name.clone(),
                    reason: format!("must be {expect}, got `{scalar_value}`"),
                });
            }
        }
    }

    Ok(())
}

fn validate_cross_field(
    rule: &CrossFieldRule,
    value: &serde_json::Value,
) -> Result<(), PreflightError> {
    match rule {
        CrossFieldRule::DateOrder {
            start,
            end,
            confirmed,
            // Tolerance is a probe-side concern, not a preflight one: a confirmed
            // ordering still blocks locally regardless of gateway tolerance.
            gateway_tolerant: _,
        } => {
            if !confirmed {
                return Ok(());
            }
            // Both must be present to compare; a missing endpoint is the field's
            // own required-ness concern, not this rule's.
            if let (Some(s), Some(e)) = (scalar(value, start), scalar(value, end)) {
                if !s.is_empty() && !e.is_empty() && s > e {
                    return Err(PreflightError {
                        field: format!("{start}/{end}"),
                        reason: format!(
                            "start date `{s}` must not be after end date `{e}`"
                        ),
                    });
                }
            }
            Ok(())
        }
    }
}

/// One mechanically-generated invalid request variant (R10): the valid seed with
/// exactly one declared constraint violated. The differential negative probe runs
/// each of these against paper alongside the valid control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidVariant {
    /// The field (or `"<start>/<end>"` for a cross-field rule) that was violated.
    pub field: String,
    /// The violated input class: `type`, `required`, `enum`, `range`, `format`,
    /// or `cross_field`.
    pub class: String,
    /// The full request body with exactly this one violation injected.
    pub request: serde_json::Value,
}

/// Generate one invalid variant per declared constraint by mechanically violating
/// it against a valid `seed` request (R10). Unlike preflight, generation covers
/// EVERY declared class regardless of `confirmed` — the probe is what confirms a
/// bound, so it must exercise unconfirmed declarations too. Deterministic: stable
/// order, no clock, no randomness.
pub fn generate_invalid_variants(
    schema: &ConstraintSchema,
    seed: &serde_json::Value,
) -> Vec<InvalidVariant> {
    let mut variants = Vec::new();
    let set = |base: &serde_json::Value, field: &str, v: serde_json::Value| {
        let mut obj = base.clone();
        if let Some(map) = obj.as_object_mut() {
            map.insert(field.to_string(), v);
        }
        obj
    };
    for field in &schema.fields {
        // Type: only an integer/number field can be given a non-numeric value.
        if matches!(field.field_type, FieldType::Integer | FieldType::Number) {
            variants.push(InvalidVariant {
                field: field.name.clone(),
                class: "type".into(),
                request: set(seed, &field.name, serde_json::json!("not-a-number")),
            });
        }
        // Required: omit the field (empty string = absent in the LS model).
        if field.required {
            variants.push(InvalidVariant {
                field: field.name.clone(),
                class: "required".into(),
                request: set(seed, &field.name, serde_json::json!("")),
            });
        }
        // Enum: a value provably outside the declared set.
        if field.enum_rule.applicable {
            let bad = format!("{}__invalid", field.enum_rule.values.join("_"));
            variants.push(InvalidVariant {
                field: field.name.clone(),
                class: "enum".into(),
                request: set(seed, &field.name, serde_json::json!(bad)),
            });
        }
        // Range: one past the nearer declared bound.
        if field.range.applicable {
            let bad = match (field.range.min, field.range.max) {
                (Some(min), _) => min - 1,
                (None, Some(max)) => max + 1,
                (None, None) => -1,
            };
            variants.push(InvalidVariant {
                field: field.name.clone(),
                class: "range".into(),
                request: set(seed, &field.name, serde_json::json!(bad.to_string())),
            });
        }
        // Format: a value that cannot match the required shape.
        if field.format.applicable {
            let bad = match field.format.kind {
                Some(FormatKind::Symbol) => "!!bad!!",
                Some(FormatKind::Date) => "notadate",
                None => "??",
            };
            variants.push(InvalidVariant {
                field: field.name.clone(),
                class: "format".into(),
                request: set(seed, &field.name, serde_json::json!(bad)),
            });
        }
    }
    // Cross-field: invert the ordering so start > end.
    for rule in &schema.cross_field {
        match rule {
            CrossFieldRule::DateOrder { start, end, .. } => {
                let mut req = set(seed, start, serde_json::json!("99991231"));
                req = set(&req, end, serde_json::json!("00010101"));
                variants.push(InvalidVariant {
                    field: format!("{start}/{end}"),
                    class: "cross_field".into(),
                    request: req,
                });
            }
        }
    }
    variants
}

/// The differential negative-probe outcome for one variant (R10, AE2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Valid control succeeded and the injected violation was rejected — the
    /// declared bound is confirmed by observed behavior.
    Clean,
    /// Inconclusive: the valid control itself failed (session-closed / unfunded /
    /// stale seed / paper-incompatible), so the variant's outcome cannot be
    /// attributed to the injected violation. Distinct from a divergence.
    Held,
    /// The declared bound diverges from observed behavior: the control succeeded
    /// but paper ACCEPTED the injected invalid variant (or the classifier saw the
    /// valid value rejected). Promotion is blocked until reconciled.
    Divergent,
}

/// The gateway's verdict on one injected invalid variant (R10, KTD1). Three-way
/// because a rejection code is not self-evidently a *merits* rejection: the
/// gateway may have accepted the variant, rejected it on merits, or never
/// evaluated it at all (a throttle, a transport failure, or an unknown code). The
/// last case is inconclusive and must fail safe to `Held`, not be read as a
/// confirmation — see [`classify_probe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantVerdict {
    /// The gateway evaluated the variant and ACCEPTED it (a success response) —
    /// the declared bound diverges from observed behavior (`Divergent`).
    Accepted,
    /// The gateway evaluated the variant on merits and REJECTED it — the injected
    /// violation is what the response reflects, so the declared bound is confirmed
    /// (`Clean`).
    Rejected,
    /// The gateway did NOT evaluate the variant on merits — an `IGW00201` throttle
    /// (see [`is_noneval_code`]), a transport failure, or an unknown/unclassified
    /// code. The probe carries no signal about the injected constraint, so it is
    /// inconclusive (`Held`) and must be re-run, never read as a confirmation.
    Inconclusive,
}

/// Classify one differential probe result (R10, KTD1). `control_succeeded` is
/// whether the valid control request came back a success; `verdict` is the
/// gateway's three-way verdict on the injected invalid variant. A failed control
/// is HELD regardless of the variant — the injected violation is not what the
/// response reflects. On a passing control: `Accepted → Divergent`,
/// `Rejected → Clean`, `Inconclusive → Held` (a throttle / non-evaluation fails
/// safe to inconclusive rather than false-`Clean`).
pub fn classify_probe(control_succeeded: bool, verdict: VariantVerdict) -> ProbeOutcome {
    if !control_succeeded {
        return ProbeOutcome::Held;
    }
    match verdict {
        VariantVerdict::Accepted => ProbeOutcome::Divergent,
        VariantVerdict::Rejected => ProbeOutcome::Clean,
        VariantVerdict::Inconclusive => ProbeOutcome::Held,
    }
}

/// `true` if `rsp_cd` is a gateway code that means the request was **never
/// evaluated on merits** — the gateway refused to route/process it, so it carries
/// no signal about any injected constraint and a differential probe seeing it is
/// inconclusive ([`VariantVerdict::Inconclusive`] → `Held`), not a confirmation.
///
/// Deliberately narrow: **only `IGW00201`** — a warm-sensitive *cumulative*
/// throttle (see the `igw00201-budget-characterization` learning). It is NOT a
/// merits reject (`IGW40011` / `IGW40013`) and NOT a success code. Add a sibling
/// code here only with per-code evidence that it, too, is a non-evaluation,
/// mirroring [`crate::error_catalog::is_ingress_validation_reject`]'s
/// one-code-with-evidence discipline.
pub fn is_noneval_code(rsp_cd: &str) -> bool {
    rsp_cd == "IGW00201"
}

/// `true` if `rsp_cd` is a code the gateway returns after evaluating a **READ**
/// variant on merits and REJECTING it — the injected violation is what the
/// response reflects, so the differential probe reads `Clean`
/// ([`VariantVerdict::Rejected`]). The read leg's merits-reject vocabulary is
/// small and fully characterized, so it is realized as a positive allowlist
/// (strict inversion): a read reject code NOT in this set is treated as a
/// non-evaluation ([`VariantVerdict::Inconclusive`] → `Held`) and re-probed,
/// never silently read as `Clean`.
///
/// Seeded from observed live evidence, deliberately narrow:
/// - `IGW40011` — an ingress input-validation reject (delegated to
///   [`crate::error_catalog::is_ingress_validation_reject`]).
/// - `IGW40013` — the t0425 `sortgb/required → IGW40013 → Clean` gateway-enforced
///   negative anchor (ledger §30, lines 1699/1996). It **must** stay in this set
///   or that certified anchor regresses `Clean → Held`.
/// - `00009` — a **business** merits-reject ("조회할 자료 없음" / invalid query key).
///   The t1101 live differential probe (`metadata/error-coverage/t1101.yaml`,
///   attended 2026-07-06) recorded `shcode/required → rsp_cd=00009` as a distinct
///   rejection in its certified CLEAN chain; a *recommended* TR. It **must** be in
///   this set or t1101's `shcode/required` anchor regresses `Clean → Held`. Unlike
///   the two IGW codes, `00009` is an exchange **business** reject, not a gateway
///   ingress code — the gateway routed the request and the exchange refused a blank
///   `shcode` on merits, which is still a merits evaluation for probe purposes.
///
/// Note the deliberate split from [`crate::error_catalog`], whose catalog groups
/// `IGW40013` with the hard-gateway `IGW50008` as one category: on the *read*
/// path they are split by evidence — `IGW40013` is a merits reject (`Clean`),
/// while `IGW50008` stays inconclusive (`Held`). Do not drop `IGW40013` from this
/// seed by reading it as a hard-gateway code. Add a sibling only with per-code
/// evidence that it, too, is a genuine read merits reject.
pub fn is_read_merits_reject(rsp_cd: &str) -> bool {
    crate::error_catalog::is_ingress_validation_reject(rsp_cd)
        || rsp_cd == "IGW40013"
        || rsp_cd == "00009"
}

fn registry() -> &'static BTreeMap<String, ConstraintSchema> {
    static REGISTRY: OnceLock<BTreeMap<String, ConstraintSchema>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut map = BTreeMap::new();
        for (tr, body) in crate::embedded::CONSTRAINT_FILES {
            let schema: ConstraintSchema = serde_yaml::from_str(body).unwrap_or_else(|e| {
                panic!("embedded metadata/constraints/{tr}.yaml must parse: {e}")
            });
            assert_eq!(
                &schema.tr_code, tr,
                "constraints/{tr}.yaml declares tr_code `{}`",
                schema.tr_code
            );
            map.insert((*tr).to_string(), schema);
        }
        map
    })
}

/// The embedded constraint schema for `tr_code`, if the TR carries one.
pub fn schema_for(tr_code: &str) -> Option<&'static ConstraintSchema> {
    registry().get(tr_code)
}

/// Locate the object a schema's fields live in. LS request bodies wrap their
/// caller-facing fields in a `{"<TR>InBlock": { ... }}` block, so the schema's
/// fields are one level down, not at the top. Returns the top-level object if it
/// directly carries a declared field, else the first nested object that does,
/// else the value unchanged (a genuinely missing field then surfaces as an
/// `Invalid` required-ness error rather than being masked).
fn locate_fields_object<'a>(
    value: &'a serde_json::Value,
    schema: &ConstraintSchema,
) -> &'a serde_json::Value {
    let has_a_field = |obj: &serde_json::Map<String, serde_json::Value>| {
        schema.fields.iter().any(|f| obj.contains_key(&f.name))
    };
    if let Some(obj) = value.as_object() {
        if has_a_field(obj) {
            return value;
        }
        for nested in obj.values() {
            if let Some(inner) = nested.as_object() {
                if has_a_field(inner) {
                    return nested;
                }
            }
        }
    }
    value
}

/// Preflight a typed request against the TR's schema, if it has one. Serializes
/// the request to a `serde_json::Value`, descends into the `{TR}InBlock` wrapper,
/// and runs [`validate_request`]; a violation returns [`LsError::Invalid`] and the
/// caller must not dispatch. A TR with no schema returns `Ok(())` (unchanged
/// behavior — permissive by default).
pub fn preflight_request<Req: Serialize>(tr_code: &str, req: &Req) -> Result<(), LsError> {
    let Some(schema) = schema_for(tr_code) else {
        return Ok(());
    };
    let value = serde_json::to_value(req).map_err(LsError::Decode)?;
    let target = locate_fields_object(&value, schema);
    validate_request(schema, target).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, ty: FieldType, required: bool) -> FieldConstraint {
        FieldConstraint {
            name: name.to_string(),
            field_type: ty,
            required,
            enum_rule: EnumRule {
                applicable: false,
                values: vec![],
                confirmed: false,
            },
            range: RangeRule {
                applicable: false,
                min: None,
                max: None,
                confirmed: false,
            },
            format: FormatRule {
                applicable: false,
                kind: None,
                confirmed: false,
            },
            gateway_tolerant: vec![],
            placed_nothing_codes: BTreeMap::new(),
        }
    }

    #[test]
    fn positive_qty_constraint_rejects_negative_with_named_field() {
        // AE1: qty declared a positive integer; qty = -5 is Invalid, names qty.
        let mut qty = field("qty", FieldType::Integer, true);
        qty.range = RangeRule {
            applicable: true,
            min: Some(1),
            max: None,
            confirmed: true,
        };
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![qty],
            cross_field: vec![],
        };
        let err = validate_request(&schema, &serde_json::json!({"qty": "-5"}))
            .expect_err("negative qty must reject");
        assert_eq!(err.field, "qty");
        assert!(err.reason.contains(">= 1"), "reason: {}", err.reason);
    }

    #[test]
    fn missing_required_field_rejects() {
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![field("shcode", FieldType::String, true)],
            cross_field: vec![],
        };
        let err = validate_request(&schema, &serde_json::json!({"shcode": ""}))
            .expect_err("empty required field must reject");
        assert_eq!(err.field, "shcode");
    }

    #[test]
    fn invalid_enum_value_rejects_when_confirmed() {
        let mut f = field("gubun", FieldType::String, true);
        f.enum_rule = EnumRule {
            applicable: true,
            values: vec!["0".into(), "1".into(), "2".into()],
            confirmed: true,
        };
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![f],
            cross_field: vec![],
        };
        let err = validate_request(&schema, &serde_json::json!({"gubun": "3"}))
            .expect_err("out-of-set enum must reject");
        assert_eq!(err.field, "gubun");
    }

    #[test]
    fn unconfirmed_enum_is_permissive_no_false_reject() {
        // R6: an unconfirmed bound must NOT block — the request proceeds.
        let mut f = field("gubun", FieldType::String, true);
        f.enum_rule = EnumRule {
            applicable: true,
            values: vec!["0".into(), "1".into(), "2".into()],
            confirmed: false,
        };
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![f],
            cross_field: vec![],
        };
        // `3` is outside the declared set but the bound is unconfirmed → permissive.
        assert!(validate_request(&schema, &serde_json::json!({"gubun": "3"})).is_ok());
    }

    #[test]
    fn cross_field_start_after_end_rejects_when_confirmed() {
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![
                field("sdate", FieldType::String, true),
                field("edate", FieldType::String, true),
            ],
            cross_field: vec![CrossFieldRule::DateOrder {
                start: "sdate".into(),
                end: "edate".into(),
                confirmed: true,
                gateway_tolerant: false,
            }],
        };
        let err =
            validate_request(&schema, &serde_json::json!({"sdate": "20260701", "edate": "20260601"}))
                .expect_err("start after end must reject");
        assert_eq!(err.field, "sdate/edate");
    }

    #[test]
    fn wrong_type_integer_rejects() {
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![field("cnt", FieldType::Integer, true)],
            cross_field: vec![],
        };
        let err = validate_request(&schema, &serde_json::json!({"cnt": "abc"}))
            .expect_err("non-integer must reject");
        assert_eq!(err.field, "cnt");
    }

    #[test]
    fn confirmed_range_enforces_on_a_fractional_number_field() {
        // A decimal value below a confirmed min must reject — an i64-only parse
        // would silently skip the check on any fractional value.
        let mut price = field("price", FieldType::Number, true);
        price.range = RangeRule {
            applicable: true,
            min: Some(1),
            max: None,
            confirmed: true,
        };
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![price],
            cross_field: vec![],
        };
        let err = validate_request(&schema, &serde_json::json!({"price": "0.5"}))
            .expect_err("0.5 < min 1 must reject");
        assert_eq!(err.field, "price");
        // A fractional value within range passes.
        assert!(validate_request(&schema, &serde_json::json!({"price": "1.5"})).is_ok());
    }

    #[test]
    fn number_field_accepts_json_number_form() {
        // A field serialized as a JSON number (string_as_number) still validates.
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![field("cnt", FieldType::Integer, true)],
            cross_field: vec![],
        };
        assert!(validate_request(&schema, &serde_json::json!({"cnt": 20})).is_ok());
    }

    #[test]
    fn valid_request_passes() {
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![field("shcode", FieldType::String, true)],
            cross_field: vec![],
        };
        assert!(validate_request(&schema, &serde_json::json!({"shcode": "005930"})).is_ok());
    }

    #[test]
    fn preflight_error_converts_to_ls_error_invalid() {
        let e: LsError = PreflightError {
            field: "qty".into(),
            reason: "must be >= 1".into(),
        }
        .into();
        assert!(matches!(e, LsError::Invalid { field, .. } if field == "qty"));
    }

    #[test]
    fn gateway_tolerant_class_round_trips_through_yaml() {
        // A YAML field carrying `gateway_tolerant: [required]` parses with the
        // class present on the field (U1 round-trip).
        let yaml = r#"
tr_code: TEST
fields:
  - name: shcode
    type: string
    required: true
    gateway_tolerant: [required]
    enum: {applicable: false}
    range: {applicable: false}
    format: {applicable: false}
"#;
        let schema: ConstraintSchema = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(schema.fields[0].gateway_tolerant, vec!["required".to_string()]);
    }

    #[test]
    fn missing_gateway_tolerant_key_defaults_empty() {
        // Backward-compat: a schema with no `gateway_tolerant` key parses with an
        // empty vec — existing schemas round-trip unchanged.
        let yaml = r#"
tr_code: TEST
fields:
  - name: shcode
    type: string
    required: true
    enum: {applicable: false}
    range: {applicable: false}
    format: {applicable: false}
"#;
        let schema: ConstraintSchema = serde_yaml::from_str(yaml).expect("parses");
        assert!(schema.fields[0].gateway_tolerant.is_empty());
    }

    #[test]
    fn gateway_tolerant_does_not_weaken_preflight() {
        // Covers R8, AE1: a `required: true` field that also marks
        // `gateway_tolerant: [required]` still fails preflight when omitted — the
        // facet only informs the probe, never preflight.
        let mut shcode = field("shcode", FieldType::String, true);
        shcode.gateway_tolerant = vec!["required".into()];
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![shcode],
            cross_field: vec![],
        };
        let err = validate_request(&schema, &serde_json::json!({"shcode": ""}))
            .expect_err("omitted required field must still reject");
        assert_eq!(err.field, "shcode");
    }

    #[test]
    fn placed_nothing_codes_round_trip_through_yaml() {
        // Route B (plan 2026-07-14-001): a field carrying a `placed_nothing_codes`
        // per-class → code map parses with the map present on the field.
        let yaml = r#"
tr_code: TEST
fields:
  - name: OrdprcPtnCode
    type: string
    required: true
    placed_nothing_codes:
      required: [IGW00000]
    enum: {applicable: false}
    range: {applicable: false}
    format: {applicable: false}
"#;
        let schema: ConstraintSchema = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(
            schema.fields[0].placed_nothing_codes.get("required"),
            Some(&vec!["IGW00000".to_string()])
        );
    }

    #[test]
    fn missing_placed_nothing_codes_key_defaults_empty() {
        // Backward-compat: a schema with no `placed_nothing_codes` key parses with
        // an empty map — every existing constraint file round-trips unchanged.
        let yaml = r#"
tr_code: TEST
fields:
  - name: shcode
    type: string
    required: true
    enum: {applicable: false}
    range: {applicable: false}
    format: {applicable: false}
"#;
        let schema: ConstraintSchema = serde_yaml::from_str(yaml).expect("parses");
        assert!(schema.fields[0].placed_nothing_codes.is_empty());
    }

    #[test]
    fn placed_nothing_codes_does_not_weaken_preflight() {
        // KTD4 analogue: a `required: true` field that also declares
        // `placed_nothing_codes: {required: [IGW00000]}` still fails preflight when
        // omitted — the facet only informs the order probe, never preflight.
        let mut f = field("OrdprcPtnCode", FieldType::String, true);
        f.placed_nothing_codes
            .insert("required".into(), vec!["IGW00000".into()]);
        let schema = ConstraintSchema {
            tr_code: "TEST".into(),
            fields: vec![f],
            cross_field: vec![],
        };
        let err = validate_request(&schema, &serde_json::json!({"OrdprcPtnCode": ""}))
            .expect_err("omitted required field must still reject");
        assert_eq!(err.field, "OrdprcPtnCode");
    }

    #[test]
    fn embedded_constraint_schemas_all_parse() {
        // The registry panics on a malformed embedded schema; touching it proves
        // every committed metadata/constraints/*.yaml round-trips at build+load.
        let _ = registry();
    }

    #[test]
    fn cross_field_gateway_tolerant_flag_round_trips_from_embedded_t8412() {
        // U1 (§30): the t8412 `sdate/edate` date_order rule is marked
        // `gateway_tolerant: true` in the embedded metadata and round-trips
        // build+load with the flag set. Preflight is unaffected — the rule stays
        // `confirmed: false` (non-blocking); tolerance is a probe-side concern only.
        let schema = schema_for("t8412").expect("t8412 carries an embedded schema");
        let tolerant = schema
            .cross_field
            .iter()
            .find_map(|r| match r {
                CrossFieldRule::DateOrder {
                    start,
                    end,
                    gateway_tolerant,
                    ..
                } => (start == "sdate" && end == "edate").then_some(*gateway_tolerant),
            })
            .expect("t8412 declares an sdate/edate date_order rule");
        assert!(tolerant, "the sdate/edate rule is marked gateway_tolerant (§30)");
    }

    #[test]
    fn cross_field_gateway_tolerant_defaults_false_when_absent() {
        // Backward-compat: a date_order rule with no `gateway_tolerant` key parses
        // with the flag off — existing schemas round-trip unchanged.
        let yaml = r#"
tr_code: TEST
fields: []
cross_field:
  - kind: date_order
    start: sdate
    end: edate
    confirmed: false
"#;
        let schema: ConstraintSchema = serde_yaml::from_str(yaml).expect("parses");
        let CrossFieldRule::DateOrder {
            gateway_tolerant, ..
        } = &schema.cross_field[0];
        assert!(!gateway_tolerant, "absent key defaults to false");
    }

    #[test]
    fn gateway_tolerant_classes_are_real_generatable_classes() {
        // A `gateway_tolerant` entry that names a class the field never generates
        // (a typo like `Required`, or `[required]` on a `required:false` field) is a
        // silent dead no-op: the differential probe would still report the accepted
        // violation as Divergent and promotion would block with no diagnostic. Assert
        // every marked class is one `generate_invalid_variants` actually emits for the
        // field, so a mis-declaration fails the offline gate instead of a live probe.
        for (tr, schema) in registry() {
            for field in &schema.fields {
                let mut generatable: std::collections::BTreeSet<&str> = Default::default();
                if matches!(field.field_type, FieldType::Integer | FieldType::Number) {
                    generatable.insert("type");
                }
                if field.required {
                    generatable.insert("required");
                }
                if field.enum_rule.applicable {
                    generatable.insert("enum");
                }
                if field.range.applicable {
                    generatable.insert("range");
                }
                if field.format.applicable {
                    generatable.insert("format");
                }
                for class in &field.gateway_tolerant {
                    assert!(
                        generatable.contains(class.as_str()),
                        "constraints/{tr}.yaml field `{}`: gateway_tolerant class `{class}` is not \
                         a class this field generates (generatable: {generatable:?}) — a typo or a \
                         tolerance on an inapplicable class",
                        field.name
                    );
                }
            }
        }
    }

    #[test]
    fn preflight_descends_into_the_inblock_wrapper() {
        // LS requests wrap fields in {"<TR>InBlock": {...}}; preflight must validate
        // the inner block, not the wrapper, or every real SDK call false-rejects.
        let bad = serde_json::json!({
            "t8412InBlock": {"shcode": "", "ncnt": "1", "qrycnt": "20", "nday": "1",
                             "sdate": "20260101", "edate": "20260105"}
        });
        let err = preflight_request("t8412", &bad).expect_err("empty shcode rejects");
        assert!(matches!(err, LsError::Invalid { field, .. } if field == "shcode"));

        let good = serde_json::json!({
            "t8412InBlock": {"shcode": "005930", "ncnt": 1, "qrycnt": 20, "nday": "1",
                             "sdate": "20260101", "edate": "20260105",
                             "cts_date": "", "cts_time": ""}
        });
        assert!(preflight_request("t8412", &good).is_ok(), "valid wrapped request passes");
    }

    // --- differential negative-probe offline twin (U4/R10/AE2) --------------

    fn sample_schema() -> ConstraintSchema {
        let mut shcode = field("shcode", FieldType::String, true);
        shcode.format = FormatRule {
            applicable: true,
            kind: Some(FormatKind::Symbol),
            confirmed: false,
        };
        let mut cnt = field("cnt", FieldType::Integer, true);
        cnt.range = RangeRule {
            applicable: true,
            min: Some(1),
            max: None,
            confirmed: false,
        };
        let mut gubun = field("gubun", FieldType::String, true);
        gubun.enum_rule = EnumRule {
            applicable: true,
            values: vec!["0".into(), "1".into()],
            confirmed: false,
        };
        ConstraintSchema {
            tr_code: "SAMPLE".into(),
            fields: vec![shcode, cnt, gubun],
            cross_field: vec![CrossFieldRule::DateOrder {
                start: "sdate".into(),
                end: "edate".into(),
                confirmed: false,
                gateway_tolerant: false,
            }],
        }
    }

    #[test]
    fn variant_generation_covers_every_declared_class() {
        let schema = sample_schema();
        let seed = serde_json::json!({
            "shcode": "005930", "cnt": "20", "gubun": "0",
            "sdate": "20260101", "edate": "20260131"
        });
        let variants = generate_invalid_variants(&schema, &seed);
        let classes: std::collections::BTreeSet<(&str, &str)> = variants
            .iter()
            .map(|v| (v.field.as_str(), v.class.as_str()))
            .collect();
        // Every declared class produces a variant.
        assert!(classes.contains(&("shcode", "required")));
        assert!(classes.contains(&("shcode", "format")));
        assert!(classes.contains(&("cnt", "type")));
        assert!(classes.contains(&("cnt", "required")));
        assert!(classes.contains(&("cnt", "range")));
        assert!(classes.contains(&("gubun", "enum")));
        assert!(classes.contains(&("sdate/edate", "cross_field")));
        // A String field with no format has no type-violation variant.
        assert!(!classes.contains(&("gubun", "type")));
    }

    #[test]
    fn generated_variants_actually_violate_the_schema() {
        // Determinism + correctness: each generated variant, run back through the
        // validator with its class CONFIRMED, is rejected on that field.
        let mut schema = sample_schema();
        for f in &mut schema.fields {
            f.enum_rule.confirmed = true;
            f.range.confirmed = true;
            f.format.confirmed = true;
        }
        for r in &mut schema.cross_field {
            let CrossFieldRule::DateOrder { confirmed, .. } = r;
            *confirmed = true;
        }
        let seed = serde_json::json!({
            "shcode": "005930", "cnt": "20", "gubun": "0",
            "sdate": "20260101", "edate": "20260131"
        });
        for v in generate_invalid_variants(&schema, &seed) {
            assert!(
                validate_request(&schema, &v.request).is_err(),
                "variant violating {}/{} should fail validation",
                v.field,
                v.class
            );
        }
    }

    #[test]
    fn differential_classifier_distinguishes_held_clean_divergent() {
        // AE2 / KTD1: control fails → HELD regardless of the variant verdict;
        // control ok maps Accepted → Divergent, Rejected → Clean, and (the new
        // fail-safe arm) Inconclusive → Held.
        assert_eq!(
            classify_probe(false, VariantVerdict::Rejected),
            ProbeOutcome::Held
        );
        assert_eq!(
            classify_probe(false, VariantVerdict::Accepted),
            ProbeOutcome::Held
        );
        assert_eq!(
            classify_probe(false, VariantVerdict::Inconclusive),
            ProbeOutcome::Held
        );
        assert_eq!(
            classify_probe(true, VariantVerdict::Accepted),
            ProbeOutcome::Divergent
        );
        assert_eq!(
            classify_probe(true, VariantVerdict::Rejected),
            ProbeOutcome::Clean
        );
        assert_eq!(
            classify_probe(true, VariantVerdict::Inconclusive),
            ProbeOutcome::Held
        );
    }

    #[test]
    fn is_noneval_code_is_igw00201_only() {
        // KTD3: the non-evaluation seed is deliberately one code. A throttle is
        // inconclusive; success codes and merits rejects are NOT non-evaluations.
        assert!(is_noneval_code("IGW00201"));
        for code in ["", "00000", "00136", "IGW40011", "IGW40013", "IGW40014", "IGW50008"] {
            assert!(
                !is_noneval_code(code),
                "`{code}` is not a non-evaluation code"
            );
        }
    }

    #[test]
    fn is_read_merits_reject_is_igw40011_igw40013_and_00009() {
        // KTD3: the read merits-reject allowlist is exactly {IGW40011, IGW40013,
        // 00009}. IGW40013 MUST be present (the t0425 sortgb Clean anchor, §30) and
        // 00009 MUST be present (the t1101 shcode/required Clean anchor,
        // error-coverage/t1101.yaml). A throttle, a success code, the hard-gateway
        // IGW50008, and unknown codes are NOT merits rejects — they surface
        // Inconclusive (Held), the fail-safe direction.
        assert!(is_read_merits_reject("IGW40011"));
        assert!(is_read_merits_reject("IGW40013"));
        assert!(is_read_merits_reject("00009"));
        for code in ["", "00000", "00136", "IGW00201", "IGW40014", "IGW50008", "40510"] {
            assert!(
                !is_read_merits_reject(code),
                "`{code}` is not a read merits reject"
            );
        }
    }
}
