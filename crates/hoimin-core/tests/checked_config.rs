use std::num::NonZeroUsize;
use std::time::Duration;

use hoimin_core::{
    CommandArg, ConfigError, DiscoveryConfig, EnvironmentSnapshot, OutputConfig, RawRunConfig,
    RunConfig, RunConfigData,
};

fn accepted_config() -> RunConfig {
    RunConfig::try_from(RawRunConfig {
        root: "project-that-need-not-exist".into(),
        files: vec!["calc.py".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    })
    .unwrap()
    .with_environment(EnvironmentSnapshot::Untracked)
    .unwrap()
}

macro_rules! rejected_construction_cases {
    ($($name:ident: $change:expr => $expected:expr;)*) => {
        $(#[test]
        fn $name() {
            // Arrange
            let mut data = accepted_config().into_data();
            let change: fn(&mut RunConfigData) = $change;
            change(&mut data);

            // Act
            let result = RunConfig::try_from(data);

            // Assert
            assert_eq!(result.unwrap_err(), $expected);
        })*
    };
}

rejected_construction_cases! {
    construction_rejects_absolute_jobs_cap: |data| {
        data.limits.jobs = NonZeroUsize::new(257).unwrap();
        data.limits.max_processes = NonZeroUsize::new(300).unwrap();
    } => ConfigError::JobsExceedsMaximum { jobs: 257, maximum: 256 };
    construction_rejects_empty_selector: |data| data.selection.files.clear()
        => ConfigError::MissingSelector;
    construction_rejects_empty_argv: |data| data.test_argv.clear()
        => ConfigError::MissingTestArgv;
    construction_rejects_empty_operators: |data| {
        data.operators = serde_json::from_str("[]").unwrap();
    } => ConfigError::EmptyMutationOperatorSelection;
    construction_rejects_escaping_root: |data| data.import_roots = vec!["../escape".into()]
        => ConfigError::InvalidImportRoot { path: "../escape".into() };
}

macro_rules! rejected_plan_conversion_cases {
    ($($name:ident: $change:expr => $expected:expr;)*) => {
        $(#[test]
        fn $name() {
            // Arrange: persisted plan data can be edited without header acceptance.
            let mut plan = accepted_config().into_plan_config(
                DiscoveryConfig::new(Duration::from_secs(30)).unwrap(),
            );
            let change: fn(&mut hoimin_core::PlanConfig) = $change;
            change(&mut plan);

            // Act
            let result = plan.into_run_config(OutputConfig::default());

            // Assert
            assert_eq!(result.unwrap_err(), $expected);
        })*
    };
}

rejected_plan_conversion_cases! {
    plan_conversion_rejects_absolute_jobs_cap: |plan| {
        plan.limits.jobs = NonZeroUsize::new(257).unwrap();
        plan.limits.max_processes = NonZeroUsize::new(300).unwrap();
    } => ConfigError::JobsExceedsMaximum { jobs: 257, maximum: 256 };
    plan_conversion_rejects_empty_selector: |plan| plan.selection.files.clear()
        => ConfigError::MissingSelector;
    plan_conversion_rejects_empty_argv: |plan| plan.test_argv.clear()
        => ConfigError::MissingTestArgv;
    plan_conversion_rejects_empty_operators: |plan| {
        plan.operators = serde_json::from_str("[]").unwrap();
    } => ConfigError::EmptyMutationOperatorSelection;
    plan_conversion_rejects_escaping_root: |plan| plan.import_roots = vec!["../escape".into()]
        => ConfigError::InvalidImportRoot { path: "../escape".into() };
    plan_conversion_rejects_discovery_timeout: |plan| {
        plan.discovery.analyzer_timeout = hoimin_budget_math::NonZeroDuration::new(
            hoimin_core::MAX_TIMEOUT + Duration::from_nanos(1),
        ).unwrap();
    } => ConfigError::InvalidLimit("analyzer_timeout");
}

proptest::proptest! {
    #[test]
    fn checked_construction_and_serde_preserve_valid_settings(
        jobs in 1_usize..=256,
        max_processes in 1_usize..=512,
        root in "[a-z]{1,12}",
        tracked in proptest::bool::ANY,
        argv in proptest::collection::vec(proptest::collection::vec(proptest::num::u8::ANY, 0..20), 1..5),
    ) {
        // Arrange: constraints are generated independently of production validation.
        let environment = if tracked {
            EnvironmentSnapshot::captured(vec!["FLAG".into()], "a".repeat(64)).unwrap()
        } else {
            EnvironmentSnapshot::Untracked
        };
        let mut data = accepted_config().into_data();
        data.limits.jobs = NonZeroUsize::new(jobs).unwrap();
        data.limits.max_processes = NonZeroUsize::new(max_processes).unwrap();
        data.import_roots = vec![root.clone().into()];
        data.test_argv = argv.iter().cloned().map(CommandArg::Unix).collect();
        data.environment = environment.clone();

        // Act
        let accepted = RunConfig::try_from(data).unwrap();
        let encoded = serde_json::to_vec(&accepted).unwrap();
        let decoded: RunConfig = serde_json::from_slice(&encoded).unwrap();

        // Assert
        proptest::prop_assert_eq!(decoded.limits.jobs.get(), jobs);
        proptest::prop_assert_eq!(decoded.limits.max_processes.get(), max_processes);
        proptest::prop_assert_eq!(decoded.import_roots[0].as_str(), root);
        proptest::prop_assert_eq!(&decoded.environment, &environment);
        proptest::prop_assert_eq!(&decoded.test_argv, &argv.into_iter().map(CommandArg::Unix).collect::<Vec<_>>());
        proptest::prop_assert_eq!(decoded, accepted);
    }
}

#[test]
fn checked_construction_accepts_more_workers_than_processes_per_tree() {
    // Arrange
    let mut data = accepted_config().into_data();
    data.limits.jobs = NonZeroUsize::new(4).unwrap();
    data.limits.max_processes = NonZeroUsize::new(1).unwrap();

    // Act
    let config = RunConfig::try_from(data).unwrap();

    // Assert
    assert_eq!(config.limits.jobs.get(), 4);
    assert_eq!(config.limits.max_processes.get(), 1);
}

#[test]
fn saved_configuration_accepts_more_workers_than_processes_per_tree() {
    // Arrange
    let mut value = serde_json::to_value(accepted_config()).unwrap();
    value["limits"]["jobs"] = serde_json::json!(4);
    value["limits"]["max_processes"] = serde_json::json!(1);

    // Act
    let config: RunConfig = serde_json::from_value(value).unwrap();

    // Assert
    assert_eq!(config.limits.jobs.get(), 4);
    assert_eq!(config.limits.max_processes.get(), 1);
}

#[test]
fn plan_conversion_preserves_independent_worker_and_process_limits() {
    // Arrange
    let mut plan =
        accepted_config().into_plan_config(DiscoveryConfig::new(Duration::from_secs(30)).unwrap());
    plan.limits.jobs = NonZeroUsize::new(4).unwrap();
    plan.limits.max_processes = NonZeroUsize::new(1).unwrap();

    // Act
    let config = plan.into_run_config(OutputConfig::default()).unwrap();

    // Assert
    assert_eq!(config.limits.jobs.get(), 4);
    assert_eq!(config.limits.max_processes.get(), 1);
}
