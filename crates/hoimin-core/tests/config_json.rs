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
fn mutant_timeout_accepts_each_legacy_variant_name() {
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
        serde_json::from_value::<MutantTimeout>(serde_json::json!("Auto")).unwrap(),
        auto
    );
    assert_eq!(
        serde_json::from_value::<MutantTimeout>(
            serde_json::json!({"Fixed": {"secs": 7, "nanos": 0}}),
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
fn command_arg_accepts_each_legacy_variant_name() {
    let unix = CommandArg::Unix(vec![112, 121]);
    let windows = CommandArg::Windows(vec![112, 121]);

    assert_eq!(
        serde_json::from_value::<CommandArg>(serde_json::json!({"Unix": [112, 121]})).unwrap(),
        unix
    );
    assert_eq!(
        serde_json::from_value::<CommandArg>(serde_json::json!({"Windows": [112, 121]})).unwrap(),
        windows
    );
}

#[test]
fn plan_config_round_trips_canonical_names_and_accepts_legacy_names() {
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

    let mut legacy = canonical;
    legacy["limits"]["mutant_timeout"] = serde_json::json!({"Fixed": {"secs": 7, "nanos": 0}});
    legacy["test_argv"] = serde_json::json!([{"Windows": [112, 121]}]);
    assert_eq!(
        serde_json::from_value::<PlanConfig>(legacy).unwrap(),
        expected
    );
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
fn run_started_accepts_legacy_normalized_config_names() {
    let mut started = RunStarted::minimal("run-1", 1, test_resource_control());
    started.normalized_config = Some(config(
        CommandArg::Windows(vec![112, 121]),
        Some(Duration::from_secs(7)),
    ));
    let expected = OutputEvent::RunStarted(started);
    let mut legacy = serde_json::to_value(&expected).unwrap();
    legacy["normalized_config"]["limits"]["mutant_timeout"] =
        serde_json::json!({"Fixed": {"secs": 7, "nanos": 0}});
    legacy["normalized_config"]["test_argv"] = serde_json::json!([{"Windows": [112, 121]}]);

    assert_eq!(
        serde_json::from_value::<OutputEvent>(legacy).unwrap(),
        expected
    );
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
