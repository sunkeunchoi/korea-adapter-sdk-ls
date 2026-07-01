//! Parse and ground per-TR request field-constraint schemas (error-resilience
//! gate U2, R3/R4/R5, KTD5).
//!
//! The constraint schema types live in [`crate::schema`]; this module holds the
//! offline *grounding* obligation: `type` and `required` must agree with the
//! normalized baseline's request field objects (the wire-shape source of truth).
//! Enum / range / format bounds are graded from the source spec (the baseline
//! carries no such data) and are not grounded here — their confirmation is the
//! differential live probe's job (R10).
//!
//! Grounding is a pure function over a lightweight [`BaselineField`] slice so it
//! is unit-testable from inline data and callable wherever the baseline JSON is
//! reachable (e.g. an `ls-core` gate test that reads the committed baseline).

use crate::schema::{ConstraintSchema, FieldType};

/// One request field extracted from a normalized baseline
/// (`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineField {
    /// The wire field name.
    pub name: String,
    /// The baseline `type` string, e.g. `"String"` or `"Number"`.
    pub type_str: String,
    /// The baseline `required` flag.
    pub required: bool,
}

/// A located grounding failure: a constraint-schema field that disagrees with the
/// baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroundingError {
    /// The schema declares a field the baseline's request blocks do not contain.
    UnknownField { tr_code: String, field: String },
    /// The schema's declared `type` is incompatible with the baseline type.
    TypeMismatch {
        tr_code: String,
        field: String,
        declared: FieldType,
        baseline: String,
    },
    /// The schema declares a field `required: true` that the wire marks optional
    /// (you may declare a baseline-required field optional, never the reverse).
    RequiredExceedsBaseline { tr_code: String, field: String },
}

impl std::fmt::Display for GroundingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroundingError::UnknownField { tr_code, field } => write!(
                f,
                "TR `{tr_code}`: constraint field `{field}` is not a request field in the baseline"
            ),
            GroundingError::TypeMismatch {
                tr_code,
                field,
                declared,
                baseline,
            } => write!(
                f,
                "TR `{tr_code}`: constraint field `{field}` declares type `{declared:?}` but the baseline says `{baseline}`"
            ),
            GroundingError::RequiredExceedsBaseline { tr_code, field } => write!(
                f,
                "TR `{tr_code}`: constraint field `{field}` is declared required but the baseline marks it optional"
            ),
        }
    }
}

/// `true` if a declared [`FieldType`] is compatible with a baseline type string.
/// The baseline vocabulary is `String` / `Number` (with occasional `Long`, `Int`,
/// `Float`, `Double` variants); a `String` field must be declared `String`, and a
/// numeric baseline type may be declared `Integer` or `Number`.
fn type_compatible(declared: FieldType, baseline: &str) -> bool {
    let numeric = matches!(
        baseline,
        "Number" | "Long" | "Int" | "Integer" | "Float" | "Double" | "Decimal"
    );
    match declared {
        FieldType::String => baseline.eq_ignore_ascii_case("String"),
        FieldType::Integer | FieldType::Number => numeric,
    }
}

/// Ground a constraint schema's `type` + `required` against the baseline request
/// fields (KTD5, R4). Returns every located [`GroundingError`]; empty means the
/// structural obligation is met. Enum/range/format are NOT grounded here.
pub fn ground_constraints(
    schema: &ConstraintSchema,
    baseline_fields: &[BaselineField],
) -> Vec<GroundingError> {
    let mut errors = Vec::new();
    for field in &schema.fields {
        let Some(baseline) = baseline_fields.iter().find(|b| b.name == field.name) else {
            errors.push(GroundingError::UnknownField {
                tr_code: schema.tr_code.clone(),
                field: field.name.clone(),
            });
            continue;
        };
        if !type_compatible(field.field_type, &baseline.type_str) {
            errors.push(GroundingError::TypeMismatch {
                tr_code: schema.tr_code.clone(),
                field: field.name.clone(),
                declared: field.field_type,
                baseline: baseline.type_str.clone(),
            });
        }
        // Permissive direction: a field may be declared LESS required than the
        // wire (e.g. continuation fields), never MORE.
        if field.required && !baseline.required {
            errors.push(GroundingError::RequiredExceedsBaseline {
                tr_code: schema.tr_code.clone(),
                field: field.name.clone(),
            });
        }
    }
    errors
}

/// Extract the request fields from a normalized-baseline JSON document (the
/// `request_blocks` array, minus the shared `request_header` block). Returns the
/// caller-facing InBlock fields — the ones a constraint schema constrains.
pub fn baseline_request_fields(baseline_json: &serde_json::Value) -> Vec<BaselineField> {
    let mut out = Vec::new();
    let Some(blocks) = baseline_json.get("request_blocks").and_then(|v| v.as_array()) else {
        return out;
    };
    for b in blocks {
        let block_name = b.get("block_name").and_then(|v| v.as_str()).unwrap_or("");
        if block_name == "request_header" {
            continue;
        }
        let Some(name) = b.get("field_name").and_then(|v| v.as_str()) else {
            continue;
        };
        out.push(BaselineField {
            name: name.to_string(),
            type_str: b
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            required: b.get("required").and_then(|v| v.as_bool()).unwrap_or(false),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{EnumRule, FieldConstraint, FormatRule, RangeRule};

    fn f(name: &str, ty: FieldType, required: bool) -> FieldConstraint {
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

    fn baseline() -> Vec<BaselineField> {
        vec![
            BaselineField {
                name: "shcode".into(),
                type_str: "String".into(),
                required: true,
            },
            BaselineField {
                name: "ncnt".into(),
                type_str: "Number".into(),
                required: true,
            },
            BaselineField {
                name: "cts_date".into(),
                type_str: "String".into(),
                required: true,
            },
        ]
    }

    #[test]
    fn clean_schema_grounds() {
        let schema = ConstraintSchema {
            tr_code: "t8412".into(),
            fields: vec![
                f("shcode", FieldType::String, true),
                f("ncnt", FieldType::Integer, true),
                // A wire-required field declared caller-optional — the permissive
                // direction is allowed.
                f("cts_date", FieldType::String, false),
            ],
            cross_field: vec![],
        };
        assert!(ground_constraints(&schema, &baseline()).is_empty());
    }

    #[test]
    fn unknown_field_is_located_error() {
        let schema = ConstraintSchema {
            tr_code: "t8412".into(),
            fields: vec![f("nope", FieldType::String, true)],
            cross_field: vec![],
        };
        let errors = ground_constraints(&schema, &baseline());
        assert!(matches!(errors[0], GroundingError::UnknownField { .. }));
        assert!(errors[0].to_string().contains("nope"));
    }

    #[test]
    fn type_mismatch_is_located_error() {
        // shcode is a String on the wire; declaring it Integer is a mismatch.
        let schema = ConstraintSchema {
            tr_code: "t8412".into(),
            fields: vec![f("shcode", FieldType::Integer, true)],
            cross_field: vec![],
        };
        let errors = ground_constraints(&schema, &baseline());
        assert!(matches!(errors[0], GroundingError::TypeMismatch { .. }));
    }

    #[test]
    fn required_exceeding_baseline_is_located_error() {
        let mut base = baseline();
        base[2].required = false; // cts_date optional on the wire
        let schema = ConstraintSchema {
            tr_code: "t8412".into(),
            fields: vec![f("cts_date", FieldType::String, true)],
            cross_field: vec![],
        };
        let errors = ground_constraints(&schema, &base);
        assert!(matches!(
            errors[0],
            GroundingError::RequiredExceedsBaseline { .. }
        ));
    }

    #[test]
    fn numeric_baseline_accepts_integer_or_number() {
        for ty in [FieldType::Integer, FieldType::Number] {
            let schema = ConstraintSchema {
                tr_code: "t8412".into(),
                fields: vec![f("ncnt", ty, true)],
                cross_field: vec![],
            };
            assert!(ground_constraints(&schema, &baseline()).is_empty());
        }
    }

    #[test]
    fn baseline_request_fields_skips_header_block() {
        let json = serde_json::json!({
            "request_blocks": [
                {"block_name": "request_header", "field_name": "authorization", "type": "String", "required": true},
                {"block_name": "t8412InBlock", "field_name": "shcode", "type": "String", "required": true},
                {"block_name": "t8412InBlock", "field_name": "ncnt", "type": "Number", "required": true}
            ]
        });
        let fields = baseline_request_fields(&json);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "shcode");
        assert!(!fields.iter().any(|f| f.name == "authorization"));
    }
}
