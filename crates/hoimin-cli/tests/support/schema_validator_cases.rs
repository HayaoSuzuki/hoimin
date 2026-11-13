use serde_json::{Value, json};

pub fn check_invalid_schema_instances(
    validate: impl Fn(&Value, &Value, &Value, &str) -> Result<(), String>,
) {
    for (schema, value, diagnostic) in [
        (
            json!({"$ref":"https://example.invalid/schema"}),
            json!(null),
            "unsupported schema reference",
        ),
        (
            json!({"$ref":"#/missing"}),
            json!(null),
            "unresolved schema reference",
        ),
        (json!({"const":1}), json!(2), "expected const"),
        (json!({"type":"unknown"}), json!(null), "type"),
        (json!({"type":"integer"}), json!("bad"), "type"),
        (json!({"minimum":1}), json!(0), "below"),
        (
            json!({"required":["field"]}),
            json!({}),
            "missing required property",
        ),
        (
            json!({"properties":{},"additionalProperties":false}),
            json!({"extra":1}),
            "unexpected property",
        ),
        (json!({"items":{"type":"integer"}}), json!(["bad"]), "type"),
    ] {
        let error = validate(&schema, &value, &schema, "$test").unwrap_err();
        assert!(error.contains(diagnostic), "{error}");
        assert!(error.contains("$test"), "{error}");
    }
    for value in [json!(-1), json!(u64::MAX)] {
        let schema = json!({"type":"integer"});
        assert!(validate(&schema, &value, &schema, "$test").is_ok());
    }
}
