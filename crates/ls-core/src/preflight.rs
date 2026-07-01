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

/// Classify one differential probe result (R10). `control_succeeded` is whether
/// the valid control request came back a success; `variant_rejected` is whether
/// the injected invalid variant was rejected by the gateway. A failed control is
/// HELD regardless of the variant — the injected violation is not what the
/// response reflects.
pub fn classify_probe(control_succeeded: bool, variant_rejected: bool) -> ProbeOutcome {
    if !control_succeeded {
        ProbeOutcome::Held
    } else if variant_rejected {
        ProbeOutcome::Clean
    } else {
        ProbeOutcome::Divergent
    }
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
    fn embedded_constraint_schemas_all_parse() {
        // The registry panics on a malformed embedded schema; touching it proves
        // every committed metadata/constraints/*.yaml round-trips at build+load.
        let _ = registry();
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
        // AE2: control fails → HELD regardless of variant; control ok + variant
        // rejected → Clean; control ok + variant accepted → Divergent.
        assert_eq!(classify_probe(false, true), ProbeOutcome::Held);
        assert_eq!(classify_probe(false, false), ProbeOutcome::Held);
        assert_eq!(classify_probe(true, true), ProbeOutcome::Clean);
        assert_eq!(classify_probe(true, false), ProbeOutcome::Divergent);
    }
}
