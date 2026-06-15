//! `ls-core` — the transport-agnostic runtime for the maintained LS Securities SDK.
//!
//! Ported from the Migration Source `korea-broker-sdk-ls` `crates/core`, stripped of
//! generator coupling. Houses auth, config, transport dispatch, rate limiting,
//! pagination, and the load-bearing serde wire-compat helpers.

pub mod auth;
pub mod client;
pub mod config;
pub mod config_resolve;
pub mod endpoint_policy;
pub mod error;
pub mod inner;
pub mod pagination;
pub mod parse;
pub mod rate_limiter;

pub use auth::{revoke_token_http, TokenData, TokenManager};
pub use client::LsClient;
pub use config::{Environment, LsConfig, RateLimitConfig, WsOverflowPolicy};
pub use config_resolve::{ResolvedConfig, ResolvedRateLimits};
pub use endpoint_policy::{EndpointPolicy, Protocol};
pub use error::{LsError, LsResult};
pub use inner::{is_paper_incompatible, Inner, PAPER_INCOMPATIBLE_CODE};
pub use pagination::HasPagination;
pub use rate_limiter::{RateLimitCategory, RateLimiterManager};

// `impl_has_pagination!` is `#[macro_export]`, so it is available at the crate
// root as `ls_core::impl_has_pagination!` for paginated request structs in
// `ls-sdk`. No additional re-export statement is required.

/// Deserialise a JSON string **or** number into a `String`.
///
/// TR response blocks use this because the LS simulation gateway occasionally
/// returns numerics as JSON numbers instead of strings. Referenced from
/// `ls-sdk` TR structs as `#[serde(deserialize_with = "ls_core::string_or_number")]`.
pub fn string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or number")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }
    }

    deserializer.deserialize_any(Visitor)
}

/// Deserialise a JSON `null`, string, or number into an `Option<String>`.
/// Companion to [`string_or_number`] for optional response fields.
pub fn option_string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct OptionVisitor;

    impl<'de> serde::de::Visitor<'de> for OptionVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("null, string, or number")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            string_or_number(deserializer).map(Some)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }
    }

    deserializer.deserialize_option(OptionVisitor)
}

/// Serialise a `String` value as a JSON number when it parses as `i64`,
/// otherwise as a JSON string.
///
/// Used for request fields that the LS gateway expects as numeric JSON
/// values (e.g. `dwmcode`, `idx`, `cnt`) even though the spec lists them
/// as string-typed on the wire.
pub fn string_as_number<S>(value: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if let Ok(n) = value.parse::<i64>() {
        serializer.serialize_i64(n)
    } else {
        serializer.serialize_str(value)
    }
}

/// Deserialise a JSON value that LS may encode as a single object `{...}`, an
/// array `[{...}]`, `null`, or an empty string `""` into a `Vec<T>`.
///
/// LS multi-block responses are inconsistent: a block holding a single record
/// arrives as a bare object, list-style blocks arrive as arrays, an absent block
/// may arrive as `null`, and some TRs (e.g. t1702) return an empty string `""`
/// when no data is available. Response structs model every secondary block as
/// `Vec<T>`; this adapter accepts all four wire shapes so a single-record or
/// empty block does not fail decoding. An array stays a `Vec` unchanged; a bare
/// object becomes a one-element `Vec`; `null` and empty string become an empty `Vec`.
pub fn de_vec_or_single<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    struct VecOrSingle<T>(std::marker::PhantomData<T>);

    impl<'de, T> serde::de::Visitor<'de> for VecOrSingle<T>
    where
        T: serde::Deserialize<'de>,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("null, array, object, or empty string")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            serde::Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))
        }

        fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            let val = T::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
            Ok(vec![val])
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if v.is_empty() {
                Ok(Vec::new())
            } else {
                Err(E::custom(format!(
                    "expected empty string for empty block, got: {}",
                    v
                )))
            }
        }
    }

    deserializer.deserialize_any(VecOrSingle(std::marker::PhantomData))
}

#[cfg(test)]
mod tests {
    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct StringField {
        #[serde(deserialize_with = "crate::string_or_number")]
        val: String,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct OptStringField {
        #[serde(default)]
        #[serde(deserialize_with = "crate::option_string_or_number")]
        val: Option<String>,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct NumericField {
        #[serde(deserialize_with = "crate::string_or_number")]
        val: String,
    }

    #[test]
    fn string_or_number_string_and_number_yield_same_value() {
        // The load-bearing behaviour: "123" and 123 must both land on the same
        // numeric value so the simulation gateway's number-vs-string drift is
        // transparent to typed TR structs.
        let from_string: StringField = serde_json::from_str(r#"{"val":"123"}"#).unwrap();
        let from_number: StringField = serde_json::from_str(r#"{"val":123}"#).unwrap();
        assert_eq!(from_string.val, "123");
        assert_eq!(from_number.val, "123");
        assert_eq!(from_string, from_number);
    }

    #[test]
    fn string_or_number_accepts_string() {
        let parsed: StringField = serde_json::from_str(r#"{"val":"hello"}"#).unwrap();
        assert_eq!(parsed.val, "hello");
    }

    #[test]
    fn string_or_number_accepts_u64() {
        let parsed: StringField = serde_json::from_str(r#"{"val":57109}"#).unwrap();
        assert_eq!(parsed.val, "57109");
    }

    #[test]
    fn string_or_number_accepts_f64() {
        let parsed: StringField = serde_json::from_str(r#"{"val":3.14}"#).unwrap();
        assert_eq!(parsed.val, "3.14");
    }

    #[test]
    fn non_numeric_string_into_numeric_field_surfaces_error_not_panic() {
        // A non-numeric string routed into a downstream numeric parse must
        // surface a recoverable error (LsError::Parse), never panic.
        let parsed: NumericField = serde_json::from_str(r#"{"val":"not-a-number"}"#).unwrap();
        let result = crate::parse::price(&parsed.val);
        assert!(matches!(result, Err(crate::LsError::Parse(_))));
    }

    #[test]
    fn option_string_or_number_accepts_string() {
        let parsed: OptStringField = serde_json::from_str(r#"{"val":"hello"}"#).unwrap();
        assert_eq!(parsed.val, Some("hello".to_string()));
    }

    #[test]
    fn option_string_or_number_accepts_number() {
        let parsed: OptStringField = serde_json::from_str(r#"{"val":57109}"#).unwrap();
        assert_eq!(parsed.val, Some("57109".to_string()));
    }

    #[test]
    fn option_string_or_number_accepts_null() {
        let parsed: OptStringField = serde_json::from_str(r#"{"val":null}"#).unwrap();
        assert_eq!(parsed.val, None);
    }

    #[test]
    fn option_string_or_number_missing_field_is_none() {
        let parsed: OptStringField = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(parsed.val, None);
    }

    #[test]
    fn option_string_or_number_some_for_zero_string() {
        // "0" is a present value, not absence: must yield Some("0"), never None.
        let parsed: OptStringField = serde_json::from_str(r#"{"val":"0"}"#).unwrap();
        assert_eq!(parsed.val, Some("0".to_string()));
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Row {
        n: i64,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct VecOrSingleField {
        #[serde(deserialize_with = "crate::de_vec_or_single")]
        rows: Vec<Row>,
    }

    #[test]
    fn de_vec_or_single_accepts_array() {
        let parsed: VecOrSingleField =
            serde_json::from_str(r#"{"rows":[{"n":1},{"n":2}]}"#).unwrap();
        assert_eq!(parsed.rows, vec![Row { n: 1 }, Row { n: 2 }]);
    }

    #[test]
    fn de_vec_or_single_accepts_bare_object() {
        // LS encodes a single-record block (e.g. an order result) as a bare object.
        let parsed: VecOrSingleField = serde_json::from_str(r#"{"rows":{"n":7}}"#).unwrap();
        assert_eq!(parsed.rows, vec![Row { n: 7 }]);
    }

    #[test]
    fn de_vec_or_single_accepts_empty_array() {
        let parsed: VecOrSingleField = serde_json::from_str(r#"{"rows":[]}"#).unwrap();
        assert_eq!(parsed.rows, Vec::<Row>::new());
    }

    #[test]
    fn de_vec_or_single_accepts_null() {
        // LS may encode an absent secondary block as JSON null.
        let parsed: VecOrSingleField = serde_json::from_str(r#"{"rows":null}"#).unwrap();
        assert_eq!(parsed.rows, Vec::<Row>::new());
    }

    #[test]
    fn de_vec_or_single_accepts_empty_string() {
        // Some LS TRs (e.g. t1702) encode an empty block as "" rather than [] or null.
        let parsed: VecOrSingleField = serde_json::from_str(r#"{"rows":""}"#).unwrap();
        assert_eq!(parsed.rows, Vec::<Row>::new());
    }

    #[derive(Debug, serde::Serialize)]
    struct NumberSerField {
        #[serde(serialize_with = "crate::string_as_number")]
        val: String,
    }

    #[test]
    fn string_as_number_serialises_numeric_as_number() {
        let out = serde_json::to_string(&NumberSerField {
            val: "42".to_string(),
        })
        .unwrap();
        assert_eq!(out, r#"{"val":42}"#);
    }

    #[test]
    fn string_as_number_serialises_non_numeric_as_string() {
        let out = serde_json::to_string(&NumberSerField {
            val: "N".to_string(),
        })
        .unwrap();
        assert_eq!(out, r#"{"val":"N"}"#);
    }
}
