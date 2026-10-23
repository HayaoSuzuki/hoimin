use super::*;

fn document() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../tests/golden/reports/schema-v10-current.json"
    ))
    .unwrap()
}

fn wires(document: &serde_json::Value) -> (String, String) {
    let mut events: Vec<serde_json::Value> =
        include_str!("../../../tests/golden/events/schema-v10-current.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    events[0] = document["run"].clone();
    events[1] = document["baseline"].clone();
    events[3] = document["mutants"][0].clone();
    events[5] = document["summary"].clone();
    (
        serde_json::to_string(document).unwrap(),
        events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

macro_rules! closed_case {
    ($name:ident, $pointer:literal, $field:literal) => {
        mod $name {
            use super::*;

            #[test]
            fn json_unknown_is_rejected() {
                // Arrange.
                let mut value = document();
                value.pointer_mut($pointer).unwrap()["unexpected"] = 123.into();
                let (wire, _) = wires(&value);
                // Act.
                let result = read_buffered_report(Path::new("report.json"), wire.as_bytes());
                // Assert.
                assert!(
                    matches!(result, Err(ReportReadError::Parse { .. })),
                    "{result:?}"
                );
            }

            #[test]
            fn jsonl_unknown_is_rejected() {
                // Arrange.
                let mut value = document();
                value.pointer_mut($pointer).unwrap()["unexpected"] = 123.into();
                let (_, wire) = wires(&value);
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
                let (wire, _) = wires(&value);
                // Act.
                let result = read_typed_json(Path::new("report.json"), wire.as_bytes());
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
                let (json, _) = wires(&value);
                let wire = json.replacen(&original, &duplicate, 1);
                // Act.
                let result = read_typed_json(Path::new("report.json"), wire.as_bytes());
                // Assert.
                assert!(matches!(result, Err(ReportReadError::Parse { .. })));
            }

            macro_rules! duplicate_case {
                ($test:ident, $jsonl:expr) => {
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
                        let (json, jsonl) = wires(&value);
                        let wire =
                            if $jsonl { jsonl } else { json }.replacen(&original, &duplicate, 1);
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
            duplicate_case!(json_duplicate_is_rejected, false);
            duplicate_case!(jsonl_duplicate_is_rejected, true);
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
        let (json, jsonl) = wires(&value);
        let path = Path::new("report");
        assert!(matches!(
            read_buffered_report(path, json.as_bytes()),
            Err(ReportReadError::Parse { .. })
        ));
        assert!(read_buffered_report(path, jsonl.as_bytes()).is_err());
        assert!(matches!(
            read_typed_json(path, json.as_bytes()),
            Err(ReportReadError::Parse { .. })
        ));
    }
}
