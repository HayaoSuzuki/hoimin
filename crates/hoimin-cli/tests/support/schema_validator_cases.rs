macro_rules! schema_validator_tests {
    ($validate:path) => {
        #[rstest::rstest]
#[case::invalid_0(
            serde_json::json!({"$ref":"https://example.invalid/schema"}),
            serde_json::json!(null),
            "unsupported schema reference",
        )]
#[case::invalid_1(
            serde_json::json!({"$ref":"#/missing"}),
            serde_json::json!(null),
            "unresolved schema reference",
        )]
#[case::invalid_2(serde_json::json!({"const":1}), serde_json::json!(2), "expected const")]
#[case::invalid_3(serde_json::json!({"type":"unknown"}), serde_json::json!(null), "type")]
#[case::invalid_4(serde_json::json!({"type":"integer"}), serde_json::json!("bad"), "type")]
#[case::invalid_5(serde_json::json!({"minimum":1}), serde_json::json!(0), "below")]
#[case::invalid_6(
            serde_json::json!({"required":["field"]}),
            serde_json::json!({}),
            "missing required property",
        )]
#[case::invalid_7(
            serde_json::json!({"properties":{},"additionalProperties":false}),
            serde_json::json!({"extra":1}),
            "unexpected property",
        )]
#[case::invalid_8(serde_json::json!({"items":{"type":"integer"}}), serde_json::json!(["bad"]), "type")]
        fn schema_validator_rejects_invalid_instance(
            #[case] schema: serde_json::Value,
            #[case] value: serde_json::Value,
            #[case] diagnostic: &str,
        ) {
            let result = $validate(&schema, &value, &schema, "$test");

            let error = result.unwrap_err();
            assert!(error.contains(diagnostic), "{error}");
            assert!(error.contains("$test"), "{error}");
        }

        #[rstest::rstest]
        #[case::negative(serde_json::json!(-1))]
        #[case::unsigned_max(serde_json::json!(u64::MAX))]
        fn schema_validator_accepts_integer_boundaries(#[case] value: serde_json::Value) {
            let schema = serde_json::json!({"type":"integer"});

            let result = $validate(&schema, &value, &schema, "$test");

            assert!(result.is_ok());
        }
    };
}

pub(crate) use schema_validator_tests;
