use std::io::{self, Read};

use proptest::prelude::*;

use super::{
    PLAN_SCHEMA_VERSION, PlanError, PlanManifest, RankedPlanCandidate, RankingReason,
    RankingReasonCode, decode_manifest,
};

#[path = "../../tests/support/plan_schema_fixture.rs"]
mod fixture;

fn message(bytes: &[u8]) -> String {
    match decode_manifest(bytes) {
        Err(PlanError::ManifestInvalid(message)) => message,
        other => panic!("expected invalid manifest, got {other:?}"),
    }
}

macro_rules! invalid_documents {
    ($($name:ident: $input:expr => $fragment:expr,)*) => {
        $(#[test]
        fn $name() {
            let bytes = $input;

            let error = message(bytes);

            assert!(error.contains($fragment), "{error}");
        })*
    };
}

invalid_documents! {
    obsolete_shape_is_rejected: br#"{"obsolete_root":{},"schema_version":9}"# => "unknown field `obsolete_root`",
    future_shape_is_rejected: br#"{"schema_version":11,"future_field":{}}"# => "unknown field `future_field`",
    unsupported_first_duplicate_is_typed_error: br#"{"schema_version":9,"schema_version":10}"# => "duplicate field `schema_version`",
    current_first_duplicate_is_typed_error: br#"{"schema_version":10,"schema_version":9}"# => "duplicate field `schema_version`",
    escaped_duplicate_is_typed_error: br#"{"schema_version":10,"sch\u0065ma_version":10}"# => "duplicate field `schema_version`",
    duplicate_kind_is_typed_error: br#"{"schema_version":10,"kind":"plan","kind":"plan"}"# => "duplicate field `kind`",
    unknown_field_is_typed_error: br#"{"schema_version":10,"obsolete_root":{}}"# => "unknown field `obsolete_root`",
    wrong_type_schema_is_rejected: br#"{"schema_version":"10"}"# => "invalid type",
    negative_schema_is_rejected: br#"{"schema_version":-1}"# => "invalid value",
    fractional_schema_is_rejected: br#"{"schema_version":10.5}"# => "invalid type",
    malformed_prefix_is_rejected: br#"{"candidates":[},"schema_version":10}"# => "expected value",
    malformed_current_body_is_rejected: br#"{"schema_version":10,"candidates":[}"# => "expected value",
    malformed_obsolete_body_precedes_guidance: br#"{"schema_version":9,"candidates":[}"# => "expected value",

}

#[test]
fn obsolete_shapes_are_rejected_without_diagnostic_precedence() {
    for bytes in [
        br#"{"schema_version":9} trailing"#.as_slice(),
        br#"{"schema_version":9,"old":"\q"}"#,
        br#"{"schema_version":9,"old":"\uD800"}"#,
        br#"{"schema_version":9,"old":1e400}"#,
        b"{\"schema_version\":9,\"old\":\"\xff\"}",
    ] {
        assert!(decode_manifest(bytes).is_err());
    }
}

#[test]
fn missing_schema_is_rejected() {
    let bytes = b"{}";

    let error = message(bytes);

    assert!(error.contains("schema"), "{error}");
}

#[test]
fn nested_schema_does_not_supply_top_level_version() {
    let bytes = br#"{"candidates":[{"schema_version":9}]}"#;

    let error = message(bytes);

    assert!(!error.contains("regenerate"), "{error}");
}

// Reorder real typed fields without deserializing the resulting bytes before Act.
fn bytes_with_schema(position: usize, escaped: bool, payload_bytes: usize) -> Vec<u8> {
    let manifest = fixture::manifest(2, payload_bytes);
    super::validate_header(&manifest).unwrap();
    let serde_json::Value::Object(mut object) = serde_json::to_value(manifest).unwrap() else {
        panic!("manifest must be an object");
    };
    let version = object.remove("schema_version").unwrap();
    let mut fields: Vec<_> = object
        .into_iter()
        .map(|(key, value)| format!("{}:{value}", serde_json::to_string(&key).unwrap()))
        .collect();
    let key = if escaped {
        r#""sch\u0065ma_version""#
    } else {
        r#""schema_version""#
    };
    fields.insert(position, format!("{key}:{version}"));
    format!("{{{}}}", fields.join(",")).into_bytes()
}

macro_rules! valid_documents {
    ($($name:ident: $position:expr, $escaped:expr,)*) => {
        $(#[test]
        fn $name() {
            let bytes = bytes_with_schema($position, $escaped, 32);

            let result = decode_manifest(&bytes).unwrap();

            assert_eq!(result.schema_version, 10);
            assert_eq!(result.candidates.len(), 2);
            assert_eq!(result.candidates[0].original, format!("['{}']", "x".repeat(32)));
        })*
    };
}

valid_documents! {
    schema_first_is_accepted: 0, false,
    schema_last_is_accepted: 7, false,
    escaped_schema_first_is_accepted: 0, true,
    escaped_schema_last_is_accepted: 7, true,
}

#[test]
fn current_trailing_content_is_rejected_after_typed_body() {
    let mut bytes = bytes_with_schema(0, false, 32);
    bytes.extend_from_slice(b" null");

    let error = message(&bytes);

    assert!(error.contains("trailing characters"), "{error}");
}

#[test]
fn duplicate_nested_known_field_is_rejected_from_original_bytes() {
    let bytes = bytes_with_schema(0, false, 32);
    let input = String::from_utf8(bytes).unwrap().replacen(
        "\"files\":[\"sample.py\"]",
        "\"files\":[\"sample.py\"],\"files\":[\"sample.py\"]",
        1,
    );
    assert!(
        input.matches("\"files\"").count() == 2,
        "fixture did not duplicate files"
    );

    let error = message(input.as_bytes());

    assert!(error.contains("duplicate field `files`"), "{error}");
}

struct Counted<'a> {
    input: &'a [u8],
    read: usize,
}

impl Read for Counted<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self.input.read(output)?;
        self.read += count;
        Ok(count)
    }
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(24))]

    #[test]
    fn strict_decode_reads_all_bytes_for_reordered_escaped_schema(
        position in 0_usize..8,
        escaped in any::<bool>(),
        payload_bytes in 0_usize..4_097,
    ) {
        let bytes = bytes_with_schema(position, escaped, payload_bytes);
        let mut reader = Counted { input: &bytes, read: 0 };

        let routed = decode_manifest(&bytes).unwrap();
        let strict = super::schema::decode_reader(&mut reader).unwrap();

        prop_assert_eq!(routed.candidates, strict.candidates);
        prop_assert_eq!(routed.schema_version, 10);
        prop_assert_eq!(reader.read, bytes.len());
    }
}

#[test]
fn current_shaped_old_and_future_versions_are_rejected() {
    for version in [9, 11] {
        let bytes = bytes_with_schema(7, false, 32);
        let text = String::from_utf8(bytes).unwrap().replace(
            "\"schema_version\":10",
            &format!("\"schema_version\":{version}"),
        );
        assert!(message(text.as_bytes()).contains("unsupported schema version"));
    }
}
