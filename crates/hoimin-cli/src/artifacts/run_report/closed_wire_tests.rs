use super::*;

fn document() -> serde_json::Value {
    test_support::current_report_view()
}

fn wire(document: &serde_json::Value) -> String {
    String::from_utf8(test_support::jsonl_bytes(document)).unwrap()
}

macro_rules! closed_case {
    ($name:ident, $pointer:literal, $field:literal) => {
        mod $name {
            use super::*;

            #[test]
            fn jsonl_unknown_is_rejected() {
                // Arrange.
                let mut value = document();
                value.pointer_mut($pointer).unwrap()["unexpected"] = 123.into();
                let wire = wire(&value);
                // Act.
                let result = read_buffered_report(Path::new("report.jsonl"), wire.as_bytes());
                // Assert.
                assert!(
                    matches!(result, Err(ReportReadError::Parse { .. })),
                    "{result:?}"
                );
            }

            #[test]
            fn review_unknown_is_rejected() {
                // Arrange.
                let mut value = document();
                value.pointer_mut($pointer).unwrap()["unexpected"] = 123.into();
                let wire = wire(&value);
                // Act.
                let result = read_typed_jsonl(Path::new("report.json"), wire.as_bytes());
                // Assert.
                assert!(matches!(result, Err(ReportReadError::Parse { .. })));
            }

            #[test]
            fn review_duplicate_is_rejected() {
                // Arrange.
                let value = document();
                let payload = value.pointer($pointer).unwrap();
                let original = serde_json::to_string(payload).unwrap();
                let duplicate =
                    original.replacen('{', &format!("{{\"{}\":{},", $field, payload[$field]), 1);
                let wire = wire(&value).replacen(&original, &duplicate, 1);
                // Act.
                let result = read_typed_jsonl(Path::new("report.json"), wire.as_bytes());
                // Assert.
                assert!(matches!(result, Err(ReportReadError::Parse { .. })));
            }

            macro_rules! duplicate_case {
                ($test:ident) => {
                    #[test]
                    fn $test() {
                        // Arrange.
                        let value = document();
                        let payload = value.pointer($pointer).unwrap();
                        let original = serde_json::to_string(payload).unwrap();
                        let duplicate = original.replacen(
                            '{',
                            &format!("{{\"{}\":{},", $field, payload[$field]),
                            1,
                        );
                        let wire = wire(&value).replacen(&original, &duplicate, 1);
                        // Act.
                        let result = read_buffered_report(Path::new("report"), wire.as_bytes());
                        // Assert.
                        assert!(
                            matches!(result, Err(ReportReadError::Parse { .. })),
                            "{result:?}"
                        );
                    }
                };
            }
            duplicate_case!(jsonl_duplicate_is_rejected);
        }
    };
}

closed_case!(run, "/run", "sequence");
closed_case!(baseline, "/baseline", "sequence");
closed_case!(mutant, "/mutants/0", "sequence");
closed_case!(summary, "/summary", "sequence");
closed_case!(candidate, "/mutants/0/candidate", "id");
closed_case!(span, "/mutants/0/candidate/span", "start");
closed_case!(output, "/baseline/output", "token");
closed_case!(counts, "/summary/counts", "killed");
closed_case!(versions, "/run/versions", "os");
closed_case!(resources, "/run/resource_control", "mode");
closed_case!(resume, "/run/resume", "status");
closed_case!(environment, "/run/normalized_config/environment", "state");
closed_case!(session, "/run/normalized_config/session", "mode");
closed_case!(output_config, "/run/normalized_config/output", "metrics");
closed_case!(
    session_config,
    "/run/normalized_config/session/config",
    "path"
);
closed_case!(
    changed,
    "/run/normalized_config/selection/changed",
    "context"
);

macro_rules! stream_event_case {
    ($name:ident, $index:expr) => {
        mod $name {
            use super::*;

            fn stream() -> Vec<String> {
                include_str!("../../../tests/golden/events/schema-v10-current.jsonl")
                    .lines()
                    .map(str::to_owned)
                    .collect()
            }

            #[test]
            fn producer_stream_is_accepted() {
                // Arrange.
                let wire = stream().join("\n");
                // Act.
                let result = read_buffered_report(Path::new("report.jsonl"), wire.as_bytes());
                // Assert.
                assert!(result.is_ok(), "{result:?}");
            }

            #[test]
            fn duplicate_payload_is_rejected() {
                // Arrange.
                let mut events = stream();
                events[$index] = events[$index].replacen('{', "{\"sequence\":999,", 1);
                let wire = events.join("\n");
                // Act.
                let result = read_buffered_report(Path::new("report.jsonl"), wire.as_bytes());
                // Assert.
                let error = result.unwrap_err();
                assert!(
                    error.to_string().contains("duplicate field `sequence`"),
                    "{error}"
                );
            }

            #[test]
            fn duplicate_kind_is_rejected() {
                // Arrange.
                let mut events = stream();
                let value: serde_json::Value = serde_json::from_str(&events[$index]).unwrap();
                events[$index] =
                    events[$index].replacen('{', &format!("{{\"kind\":{},", value["kind"]), 1);
                let wire = events.join("\n");
                // Act.
                let result = read_buffered_report(Path::new("report.jsonl"), wire.as_bytes());
                // Assert.
                let error = result.unwrap_err();
                assert!(
                    error.to_string().contains("duplicate field `kind`"),
                    "{error}"
                );
            }
        }
    };
}

stream_event_case!(stream_run, 0);
stream_event_case!(stream_baseline, 1);
stream_event_case!(stream_mutant_start, 2);
stream_event_case!(stream_mutant_finish, 3);
stream_event_case!(stream_diagnostic, 4);
stream_event_case!(stream_summary, 5);

#[test]
fn run_kind_requires_a_string_for_progress_and_review() {
    for kind in [
        serde_json::json!({"run_started": null}),
        serde_json::json!(["run_started"]),
        serde_json::Value::Null,
        serde_json::json!(42),
    ] {
        let mut value = document();
        value["run"]["kind"] = kind;
        let jsonl = wire(&value);
        let path = Path::new("report");
        assert!(matches!(
            read_buffered_report(path, jsonl.as_bytes()),
            Err(ReportReadError::Parse { .. })
        ));
        assert!(matches!(
            read_typed_jsonl(path, jsonl.as_bytes()),
            Err(ReportReadError::Parse { .. })
        ));
    }
}
