use hoimin_core::{
    CommandArg, ConfigError, MutationOperator, RawRunConfig, RunConfig,
};

#[test]
fn raw_operator_includes_and_excludes_cannot_normalize_to_empty() {
    let mut raw = raw_config();
    raw.exclude_operators = vec!["compare_eq_ne".to_owned()];

    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::EmptyMutationOperatorSelection)
    );
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
    let mut plan = RunConfig::try_from(raw_config())
        .unwrap()
        .into_plan_config();
    plan.operators.exclude(MutationOperator::CompareEqNe);

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
