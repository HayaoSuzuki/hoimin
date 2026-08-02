use hoimin_core::{CommandArg, ConfigError, MutationOperator, PlanConfig, RawRunConfig, RunConfig};

#[test]
fn raw_operator_includes_and_excludes_cannot_normalize_to_empty() {
    let mut raw = raw_config();
    raw.exclude_operators = vec!["compare_eq_ne".to_owned()];

    let error = RunConfig::try_from(raw).unwrap_err();
    assert_eq!(error, ConfigError::EmptyMutationOperatorSelection);
    assert!(error.to_string().contains("--operators"));
    assert!(error.to_string().contains("--exclude-operators"));
}

#[test]
fn normalized_run_config_rejects_an_empty_operator_selection() {
    let mut config = RunConfig::try_from(raw_config()).unwrap();
    config.operators.exclude(MutationOperator::CompareEqNe);

    assert_eq!(
        config.validate(),
        Err(ConfigError::EmptyMutationOperatorSelection)
    );
}

#[test]
fn persisted_plan_config_rejects_an_empty_operator_selection() {
    let plan = RunConfig::try_from(raw_config())
        .unwrap()
        .into_plan_config();
    let mut value = serde_json::to_value(plan).unwrap();
    value["operators"] = serde_json::json!([]);
    let plan: PlanConfig = serde_json::from_value(value).unwrap();

    assert_eq!(
        plan.validate(),
        Err(ConfigError::EmptyMutationOperatorSelection)
    );
}

fn raw_config() -> RawRunConfig {
    RawRunConfig {
        root: ".".into(),
        files: vec!["src/lib.py".into()],
        operators: vec!["compare_eq_ne".to_owned()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    }
}
