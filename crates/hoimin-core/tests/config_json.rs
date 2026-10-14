use std::time::Duration;

use hoimin_core::{
    CommandArg, MutantTimeout, OutputEvent, PlanConfig, RawRunConfig, RawRunLimits, RunConfig,
    RunStarted,
};

#[test]
fn mutant_timeout_round_trips_each_snake_case_variant() {
    let auto = config(CommandArg::Unix(b"python".to_vec()), None)
        .limits
        .mutant_timeout;
    let fixed = config(
        CommandArg::Unix(b"python".to_vec()),
        Some(Duration::from_secs(7)),
    )
    .limits
    .mutant_timeout;

    assert_eq!(
        serde_json::to_value(auto).unwrap(),
        serde_json::json!("auto")
    );
    assert_eq!(
        serde_json::to_value(fixed).unwrap(),
        serde_json::json!({"fixed": {"secs": 7, "nanos": 0}})
    );
    assert_eq!(
        serde_json::from_value::<MutantTimeout>(serde_json::json!("auto")).unwrap(),
        auto
    );
    assert_eq!(
        serde_json::from_value::<MutantTimeout>(
            serde_json::json!({"fixed": {"secs": 7, "nanos": 0}}),
        )
        .unwrap(),
        fixed
    );
}

#[test]
fn command_arg_round_trips_each_snake_case_variant() {
    let unix = CommandArg::Unix(vec![112, 121]);
    let windows = CommandArg::Windows(vec![112, 121]);

    assert_eq!(
        serde_json::to_value(&unix).unwrap(),
        serde_json::json!({"unix": [112, 121]})
    );
    assert_eq!(
        serde_json::to_value(&windows).unwrap(),
        serde_json::json!({"windows": [112, 121]})
    );
    assert_eq!(
        serde_json::from_value::<CommandArg>(serde_json::json!({"unix": [112, 121]})).unwrap(),
        unix
    );
    assert_eq!(
        serde_json::from_value::<CommandArg>(serde_json::json!({"windows": [112, 121]})).unwrap(),
        windows
    );
}

#[test]
fn plan_config_round_trips_canonical_names_and_rejects_legacy_names() {
    let expected = config(
        CommandArg::Windows(vec![112, 121]),
        Some(Duration::from_secs(7)),
    )
    .into_plan_config();
    let canonical = serde_json::to_value(&expected).unwrap();

    assert_eq!(
        canonical["limits"]["mutant_timeout"],
        serde_json::json!({"fixed": {"secs": 7, "nanos": 0}})
    );
    assert_eq!(
        canonical["test_argv"],
        serde_json::json!([{"windows": [112, 121]}])
    );
    assert_eq!(
        serde_json::from_value::<PlanConfig>(canonical.clone()).unwrap(),
        expected
    );

    for field in ["timeout", "argv", "operator"] {
        let mut legacy = canonical.clone();
        match field {
            "timeout" => {
                legacy["limits"]["mutant_timeout"] =
                    serde_json::json!({"Fixed": {"secs": 7, "nanos": 0}});
            }
            "argv" => legacy["test_argv"] = serde_json::json!([{"Windows": [112, 121]}]),
            _ => legacy["operators"] = serde_json::json!(["type_mapping"]),
        }
        assert!(
            serde_json::from_value::<PlanConfig>(legacy).is_err(),
            "{field}"
        );
    }
}

#[test]
fn run_started_round_trips_canonical_normalized_config() {
    let mut started = RunStarted::minimal("run-1", 1, test_resource_control());
    started.normalized_config = Some(config(
        CommandArg::Windows(vec![112, 121]),
        Some(Duration::from_secs(7)),
    ));
    let event = OutputEvent::RunStarted(started);

    let value = serde_json::to_value(&event).unwrap();

    assert_eq!(
        value["normalized_config"]["limits"]["mutant_timeout"],
        serde_json::json!({"fixed": {"secs": 7, "nanos": 0}})
    );
    assert_eq!(
        value["normalized_config"]["test_argv"],
        serde_json::json!([{"windows": [112, 121]}])
    );
    assert_eq!(serde_json::from_value::<OutputEvent>(value).unwrap(), event);
}

#[test]
fn run_started_rejects_legacy_normalized_config_names() {
    let mut started = RunStarted::minimal("run-1", 1, test_resource_control());
    started.normalized_config = Some(config(
        CommandArg::Windows(vec![112, 121]),
        Some(Duration::from_secs(7)),
    ));
    let canonical = serde_json::to_value(OutputEvent::RunStarted(started)).unwrap();
    for field in ["timeout", "argv", "operator"] {
        let mut legacy = canonical.clone();
        match field {
            "timeout" => {
                legacy["normalized_config"]["limits"]["mutant_timeout"] =
                    serde_json::json!({"Fixed": {"secs": 7, "nanos": 0}});
            }
            "argv" => {
                legacy["normalized_config"]["test_argv"] =
                    serde_json::json!([{"Windows": [112, 121]}]);
            }
            _ => legacy["normalized_config"]["operators"] = serde_json::json!(["type_mapping"]),
        }
        assert!(
            serde_json::from_value::<OutputEvent>(legacy).is_err(),
            "{field}"
        );
    }
}

fn config(test_arg: CommandArg, mutant_timeout: Option<Duration>) -> RunConfig {
    RunConfig::try_from(RawRunConfig {
        files: vec!["src/lib.py".into()],
        limits: RawRunLimits {
            mutant_timeout,
            ..RawRunLimits::default()
        },
        test_argv: vec![test_arg],
        ..RawRunConfig::default()
    })
    .unwrap()
}

fn test_resource_control() -> hoimin_core::ResourceControl {
    hoimin_core::ResourceControl {
        mode: hoimin_core::ResourceMode::Hard,
        mechanism: "test_supplied_hard".into(),
    }
}

#[test]
fn saved_enum_names_reject_legacy_aliases() {
    for value in [
        serde_json::json!("Auto"),
        serde_json::json!({"Fixed": {"secs": 7, "nanos": 0}}),
    ] {
        assert!(
            serde_json::from_value::<MutantTimeout>(value.clone()).is_err(),
            "{value}"
        );
    }
    for value in [
        serde_json::json!({"Unix": [112, 121]}),
        serde_json::json!({"Windows": [112, 121]}),
    ] {
        assert!(
            serde_json::from_value::<CommandArg>(value.clone()).is_err(),
            "{value}"
        );
    }
    assert!(
        serde_json::from_value::<hoimin_core::MutationOperator>(serde_json::json!("type_mapping"))
            .is_err()
    );
}
