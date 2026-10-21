use hoimin_core::OutputEvent;

fn event(index: usize) -> String {
    include_str!("../../hoimin-cli/tests/golden/events/schema-v10-current.jsonl")
        .lines()
        .nth(index)
        .unwrap()
        .to_owned()
}

macro_rules! event_case {
    ($name:ident, $index:expr) => {
        mod $name {
            use super::*;

            #[test]
            fn valid_producer_roundtrip() {
                // Arrange.
                let expected: OutputEvent = serde_json::from_str(&event($index)).unwrap();
                let wire = serde_json::to_string(&expected).unwrap();
                // Act.
                let actual: OutputEvent = serde_json::from_str(&wire).unwrap();
                // Assert.
                assert_eq!(actual, expected);
            }

            #[test]
            fn unknown_field_is_rejected() {
                // Arrange.
                let wire = event($index).replacen('{', "{\"unexpected\":123,", 1);
                // Act.
                let result = serde_json::from_str::<OutputEvent>(&wire);
                // Assert.
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("unknown field `unexpected`")
                );
            }

            #[test]
            fn duplicate_known_field_is_rejected() {
                // Arrange.
                let wire = event($index).replacen('{', "{\"sequence\":999,", 1);
                // Act.
                let result = serde_json::from_str::<OutputEvent>(&wire);
                // Assert.
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("duplicate field `sequence`")
                );
            }

            #[test]
            fn duplicate_discriminator_is_rejected() {
                // Arrange.
                let value: serde_json::Value = serde_json::from_str(&event($index)).unwrap();
                let prefix = format!("{{\"kind\":{},", value["kind"]);
                let wire = event($index).replacen('{', &prefix, 1);
                // Act.
                let result = serde_json::from_str::<OutputEvent>(&wire);
                // Assert.
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("duplicate field `kind`")
                );
            }
        }
    };
}

event_case!(run_started, 0);
event_case!(baseline_finished, 1);
event_case!(mutant_started, 2);
event_case!(mutant_finished, 3);
event_case!(diagnostic, 4);
event_case!(run_finished, 5);

fn nested_event(index: usize) -> serde_json::Value {
    let source_index = match index {
        6 => 0,
        7 => 3,
        _ => index,
    };
    let mut value: serde_json::Value = serde_json::from_str(&event(source_index)).unwrap();
    if index == 6 {
        value["verification_selection"] = serde_json::json!({
            "mode":"sample", "policy":"splitmix64_fisher_yates_v1", "requested":1,
            "selected":1,"scope":"sampled_candidates","plan_truncated":false,
            "sampling":{"population":1,"seed":0,"selected_ids":["m1"]}
        });
    }
    if index == 7 {
        value["outcome"] = serde_json::json!({"reused":{"status":"killed"}});
    }
    if index == 5 {
        value["disk"]["filesystems"] = serde_json::json!([{
            "key":"fs", "start_available_bytes": 1, "minimum_available_bytes": 1,
            "end_available_bytes": 2, "available_bytes_change": 1
        }]);
        value["disk"]["enforcement"] = serde_json::json!([
            {"kind":"portable_guard"},
            {"kind":"capacity_only","root_kind":"execution","filesystem_key":"fs"},
            {"kind":"verified_aggregate","backend":"linux_project_quota","probe":{
                "capability":"project_quota","verified":true,"observation":"verified"
            }}
        ]);
        value["disk"]["cleanup"] = serde_json::json!([{
            "root_id":"execution","owner":"run","status":"clean","examined_entries":0,
            "removed_entries":0,"details":[],"omitted_detail_count":0,"remaining_root":null
        }]);
        value["disk"]["stop"] = serde_json::json!({
            "code":"disk.stop","owned_bytes":null,"available_bytes":null,"message":null,
            "secondary":[
                {"Error":{"code":"disk.error","message":"failed"}},
                {"Observation":{"reason":"measurement_failed","value":{
                    "owned_bytes":0,"available_bytes":0,"measured_in":{"secs":0,"nanos":0}
                }}}
            ]
        });
        value["disk"]["janitor"] =
            serde_json::to_value(hoimin_core::JanitorSummary::default()).unwrap();
        value["disk"]["janitor"]["details"] =
            serde_json::json!([{"reason":"young","message":"young"}]);
    }
    if index == 3 {
        value["diagnostics"] = serde_json::json!([{
            "mutant_id":"m1","level":"warning","code":"test","message":"test"
        }]);
    }
    value
}

macro_rules! nested_case {
    ($name:ident, $index:expr, $pointer:literal) => {
        mod $name {
            use super::*;

            #[test]
            fn valid_nested_payload_is_accepted() {
                // Arrange.
                let wire = serde_json::to_string(&nested_event($index)).unwrap();
                // Act.
                let result = serde_json::from_str::<OutputEvent>(&wire);
                // Assert.
                assert!(result.is_ok(), "{result:?}");
            }

            #[test]
            fn unknown_nested_field_is_rejected() {
                // Arrange.
                let mut value = nested_event($index);
                value.pointer_mut($pointer).unwrap()["unexpected"] = 123.into();
                let wire = serde_json::to_string(&value).unwrap();
                // Act.
                let result = serde_json::from_str::<OutputEvent>(&wire);
                // Assert.
                assert!(result.is_err(), "accepted unknown field at {}", $pointer);
            }

            #[test]
            fn duplicate_nested_field_is_rejected() {
                // Arrange: insert a second spelling into raw JSON, never into a Value map.
                let value = nested_event($index);
                let payload = value.pointer($pointer).unwrap();
                let (key, field) = payload.as_object().unwrap().iter().next().unwrap();
                let original = serde_json::to_string(payload).unwrap();
                let duplicate = original.replacen(
                    '{',
                    &format!("{{{}:{field},", serde_json::to_string(key).unwrap()),
                    1,
                );
                let wire = serde_json::to_string(&value)
                    .unwrap()
                    .replacen(&original, &duplicate, 1);
                // Act.
                let result = serde_json::from_str::<OutputEvent>(&wire);
                // Assert.
                assert!(result.is_err(), "accepted duplicate field at {}", $pointer);
            }
        }
    };
}

nested_case!(versions, 0, "/versions");
nested_case!(resource_control, 0, "/resource_control");
nested_case!(resume, 0, "/resume");
nested_case!(baseline_output, 1, "/output");
nested_case!(baseline_termination, 1, "/termination");
nested_case!(candidate, 3, "/candidate");
nested_case!(candidate_span, 3, "/candidate/span");
nested_case!(outcome, 3, "/outcome");
nested_case!(executed, 3, "/outcome/executed");
nested_case!(mutant_output, 3, "/outcome/executed/output");
nested_case!(mutant_elapsed, 3, "/outcome/executed/elapsed");
nested_case!(mutant_termination, 3, "/outcome/executed/termination");
nested_case!(session_diagnostic, 3, "/diagnostics/0");
nested_case!(counts, 5, "/counts");
nested_case!(disk, 5, "/disk");
nested_case!(filesystem, 5, "/disk/filesystems/0");
nested_case!(portable_enforcement, 5, "/disk/enforcement/0");
nested_case!(capacity_enforcement, 5, "/disk/enforcement/1");
nested_case!(aggregate_enforcement, 5, "/disk/enforcement/2");
nested_case!(capability_probe, 5, "/disk/enforcement/2/probe");
nested_case!(cleanup, 5, "/disk/cleanup/0");
nested_case!(stop, 5, "/disk/stop");
nested_case!(secondary_error, 5, "/disk/stop/secondary/0/Error");
nested_case!(
    secondary_observation,
    5,
    "/disk/stop/secondary/1/Observation"
);
nested_case!(
    secondary_value,
    5,
    "/disk/stop/secondary/1/Observation/value"
);
nested_case!(
    secondary_duration,
    5,
    "/disk/stop/secondary/1/Observation/value/measured_in"
);
nested_case!(janitor, 5, "/disk/janitor");
nested_case!(janitor_scan, 5, "/disk/janitor/scan");
nested_case!(janitor_detail, 5, "/disk/janitor/details/0");
nested_case!(janitor_omissions, 5, "/disk/janitor/detail_omissions");

#[test]
fn resumed_outcome_rejects_unknown_field() {
    // Arrange.
    let mut value = nested_event(0);
    value["resume"] = serde_json::json!({"status":"resumed","unexpected":123});
    let wire = serde_json::to_string(&value).unwrap();
    // Act.
    let result = serde_json::from_str::<OutputEvent>(&wire);
    // Assert.
    assert!(result.is_err());
}

#[test]
fn tagged_summary_preserves_raw_filesystem_integer_above_i64() {
    // Arrange.
    let mut value = nested_event(5);
    value["disk"]["filesystems"][0]["end_available_bytes"] = u64::MAX.into();
    value["disk"]["filesystems"][0]["available_bytes_change"] = (u64::MAX - 1).into();
    let wire = serde_json::to_string(&value).unwrap();
    // Act.
    let result = serde_json::from_str::<OutputEvent>(&wire);
    // Assert.
    let OutputEvent::RunFinished(summary) = result.unwrap() else {
        panic!("expected summary")
    };
    assert_eq!(
        summary.disk.filesystems[0].available_bytes_change,
        Some(i128::from(u64::MAX - 1))
    );
}

macro_rules! discriminator_position_case {
    ($name:ident, $index:expr) => {
        mod $name {
            use super::*;

            fn trailing_kind() -> String {
                let mut value = nested_event($index);
                let kind = value.as_object_mut().unwrap().remove("kind").unwrap();
                let mut wire = serde_json::to_string(&value).unwrap();
                wire.pop();
                format!("{wire},\"kind\":{kind}}}")
            }

            #[test]
            fn discriminator_after_payload_is_accepted() {
                // Arrange.
                let wire = trailing_kind();
                // Act.
                let result = serde_json::from_str::<OutputEvent>(&wire);
                // Assert.
                assert!(result.is_ok(), "{result:?}");
            }

            #[test]
            fn duplicate_payload_before_discriminator_is_rejected() {
                // Arrange.
                let wire = trailing_kind().replacen('{', "{\"sequ\\u0065nce\":999,", 1);
                // Act.
                let result = serde_json::from_str::<OutputEvent>(&wire);
                // Assert.
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("duplicate field `sequence`")
                );
            }

            #[test]
            fn escaped_duplicate_discriminator_at_end_is_rejected() {
                // Arrange.
                let mut wire = trailing_kind();
                wire.pop();
                wire.push_str(",\"ki\\u006ed\":\"run_finished\"}");
                // Act.
                let result = serde_json::from_str::<OutputEvent>(&wire);
                // Assert.
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("duplicate field `kind`")
                );
            }
        }
    };
}

discriminator_position_case!(run_started_position, 0);
discriminator_position_case!(baseline_finished_position, 1);
discriminator_position_case!(mutant_started_position, 2);
discriminator_position_case!(mutant_finished_position, 3);
discriminator_position_case!(diagnostic_position, 4);
discriminator_position_case!(run_finished_position, 5);

nested_case!(sampling, 6, "/verification_selection/sampling");
nested_case!(reused, 7, "/outcome/reused");

#[test]
fn run_finished_requires_explicit_resolved_timeout_or_null() {
    let mut wire: serde_json::Value = serde_json::from_str(&event(5)).unwrap();
    wire.as_object_mut()
        .unwrap()
        .remove("resolved_mutant_timeout");
    let error = serde_json::from_value::<OutputEvent>(wire).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("missing field `resolved_mutant_timeout`")
    );
}

fn assert_invalid_resolved_timeout(value: serde_json::Value) {
    let mut wire: serde_json::Value = serde_json::from_str(&event(5)).unwrap();
    wire["resolved_mutant_timeout"] = value;
    assert!(serde_json::from_value::<OutputEvent>(wire).is_err());
}

#[test]
fn resolved_timeout_wire_rejects_array_duration() {
    assert_invalid_resolved_timeout(serde_json::json!([1, 2]));
}

#[test]
fn resolved_timeout_wire_rejects_unknown_duration_fields() {
    assert_invalid_resolved_timeout(serde_json::json!({"secs":1,"nanos":2,"unexpected":3}));
}

#[test]
fn resolved_timeout_wire_rejects_noncanonical_nanoseconds() {
    assert_invalid_resolved_timeout(serde_json::json!({"secs":1,"nanos":1_000_000_000}));
}

#[test]
fn every_event_kind_requires_a_string() {
    for index in 0..6 {
        let original: serde_json::Value = serde_json::from_str(&event(index)).unwrap();
        let kind = original["kind"].as_str().unwrap();
        for invalid in [
            serde_json::json!({kind: null}),
            serde_json::json!([kind]),
            serde_json::Value::Null,
            serde_json::json!(42),
        ] {
            let mut value = original.clone();
            value["kind"] = invalid;
            let wire = serde_json::to_string(&value).unwrap();
            assert!(
                serde_json::from_str::<OutputEvent>(&wire).is_err(),
                "event {kind} accepted a non-string discriminator"
            );
        }
    }
}
