use hoimin_core::{
    CommandArg, ConfigError, FingerprintInputFile, MAX_JOBS, MutationProfile, OutputConfig,
    OutputFormat, PlanConfig, RawRunConfig, RawRunLimits, RunConfig, SessionConfig,
};

fn plan_value() -> serde_json::Value {
    serde_json::to_value(fixture_run_config().into_plan_config()).unwrap()
}

type InvalidPlanCase = (&'static str, fn(&mut serde_json::Value), ConfigError);

#[test]
fn plan_config_round_trips_run_semantics_without_session_or_resume() {
    let mut config = fixture_run_config();
    config.session = Some(SessionConfig {
        path: "state.sqlite".into(),
    });
    config.resume = true;

    let value = serde_json::to_value(config.clone().into_plan_config()).unwrap();
    assert_eq!(
        value["fingerprint_files"],
        serde_json::json!(["pyproject.toml"])
    );
    assert!(value.get("session").is_none());
    assert!(value.get("resume").is_none());

    let restored = serde_json::from_value::<PlanConfig>(value)
        .unwrap()
        .into_run_config(OutputConfig {
            format: OutputFormat::Json,
            metrics: None,
        });

    let expected = RunConfig {
        output: OutputConfig {
            format: OutputFormat::Json,
            metrics: None,
        },
        session: None,
        resume: false,
        ..config
    };
    assert_eq!(restored, expected);
    assert_eq!(restored.session, None);
    assert!(!restored.resume);
    assert_eq!(restored.fingerprint_files, ["pyproject.toml"]);
}

#[test]
fn plan_config_defaults_missing_fingerprint_files() {
    let mut value = serde_json::to_value(fixture_run_config().into_plan_config()).unwrap();
    value.as_object_mut().unwrap().remove("fingerprint_files");

    let plan: PlanConfig = serde_json::from_value(value).unwrap();

    assert!(plan.fingerprint_files.is_empty());
}

#[test]
fn valid_normalized_plan_and_run_configs_pass_validation() {
    let run = fixture_run_config();
    let plan = run.clone().into_plan_config();

    assert_eq!(plan.validate(), Ok(()));
    assert_eq!(run.validate(), Ok(()));
}

#[test]
fn plan_config_rejects_invalid_normalized_semantics() {
    let cases: &[InvalidPlanCase] = &[
        (
            "missing selector",
            |value| {
                value["selection"]["sources"] = serde_json::json!([]);
                value["selection"]["files"] = serde_json::json!([]);
                value["selection"]["lines"] = serde_json::json!([]);
                value["selection"]["symbols"] = serde_json::json!([]);
                value["selection"]["changed"] = serde_json::json!(false);
            },
            ConfigError::MissingSelector,
        ),
        (
            "diff base without changed",
            |value| {
                value["selection"]["diff_base"] = serde_json::json!("main");
                value["selection"]["changed"] = serde_json::json!(false);
            },
            ConfigError::DiffBaseRequiresChanged,
        ),
        (
            "changed without source",
            |value| {
                value["selection"]["sources"] = serde_json::json!([]);
                value["selection"]["changed"] = serde_json::json!(true);
            },
            ConfigError::ChangedRequiresSource,
        ),
        (
            "symbol without source",
            |value| {
                value["selection"]["sources"] = serde_json::json!([]);
                value["selection"]["files"] = serde_json::json!([]);
                value["selection"]["symbols"] = serde_json::json!([{
                    "module": "module",
                    "qualname": "symbol",
                }]);
            },
            ConfigError::SymbolRequiresSource,
        ),
        (
            "empty test argv",
            |value| value["test_argv"] = serde_json::json!([]),
            ConfigError::MissingTestArgv,
        ),
        (
            "jobs above maximum",
            |value| value["limits"]["jobs"] = serde_json::json!(MAX_JOBS + 1),
            ConfigError::JobsExceedsMaximum {
                jobs: MAX_JOBS + 1,
                maximum: MAX_JOBS,
            },
        ),
        (
            "zero analyzer duration",
            |value| {
                value["limits"]["analyzer_timeout"] = serde_json::json!({"secs": 0, "nanos": 0});
            },
            ConfigError::InvalidLimit("analyzer_timeout"),
        ),
        (
            "zero baseline duration",
            |value| {
                value["limits"]["baseline_timeout"] = serde_json::json!({"secs": 0, "nanos": 0});
            },
            ConfigError::InvalidLimit("baseline_timeout"),
        ),
        (
            "zero mutant duration",
            |value| {
                value["limits"]["mutant_timeout"] =
                    serde_json::json!({"Fixed": {"secs": 0, "nanos": 0}});
            },
            ConfigError::InvalidLimit("mutant_timeout"),
        ),
        (
            "zero total duration",
            |value| {
                value["limits"]["total_timeout"] = serde_json::json!({"secs": 0, "nanos": 0});
            },
            ConfigError::InvalidLimit("total_timeout"),
        ),
        (
            "baseline arithmetic overflow",
            |value| {
                value["limits"]["baseline_timeout"] =
                    serde_json::json!({"secs": u64::MAX, "nanos": 999_999_999});
            },
            ConfigError::InvalidLimit("baseline_timeout"),
        ),
    ];

    for (name, mutate, expected) in cases {
        let mut value = plan_value();
        mutate(&mut value);
        let config: PlanConfig = serde_json::from_value(value).unwrap();
        assert_eq!(config.validate(), Err(expected.clone()), "{name}");
    }
}

#[test]
fn normalized_limit_validation_rejects_cross_field_violations() {
    let mut value = plan_value();
    value["limits"]["jobs"] = serde_json::json!(2);
    value["limits"]["max_processes"] = serde_json::json!(1);
    let config: PlanConfig = serde_json::from_value(value).unwrap();
    assert_eq!(
        config.validate(),
        Err(ConfigError::JobsExceedsProcesses {
            jobs: 2,
            max_processes: 1,
        })
    );
}

#[test]
fn normalized_limit_validation_accepts_maximum_jobs() {
    let mut value = plan_value();
    value["limits"]["jobs"] = serde_json::json!(MAX_JOBS);
    value["limits"]["max_processes"] = serde_json::json!(MAX_JOBS);
    let config: PlanConfig = serde_json::from_value(value).unwrap();

    assert_eq!(config.validate(), Ok(()));
}

#[cfg(target_pointer_width = "64")]
#[test]
fn normalized_limit_validation_rejects_process_count_above_u32() {
    let mut value = plan_value();
    let max_processes = usize::try_from(u64::from(u32::MAX) + 1).unwrap();
    value["limits"]["max_processes"] = serde_json::json!(max_processes);
    let config: PlanConfig = serde_json::from_value(value).unwrap();

    assert_eq!(
        config.validate(),
        Err(ConfigError::InvalidLimit("max_processes"))
    );
}

#[test]
fn run_config_validation_keeps_runtime_only_resume_dependency() {
    let mut config = fixture_run_config();
    config.resume = true;
    config.session = None;

    assert_eq!(config.validate(), Err(ConfigError::ResumeRequiresSession));
}

fn fixture_run_config() -> RunConfig {
    let mut config = RunConfig::try_from(RawRunConfig {
        root: "project".into(),
        files: vec!["src/lib.py".into()],
        fingerprint_includes: vec!["pyproject.toml".to_owned()],
        fingerprint_files: vec!["pyproject.toml".to_owned()],
        operators: vec!["compare_eq_ne".to_owned()],
        allow_best_effort_memory: true,
        profile: MutationProfile::Focused,
        limits: RawRunLimits {
            jobs: 2,
            max_processes: 2,
            ..RawRunLimits::default()
        },
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        output: OutputConfig {
            format: OutputFormat::Human,
            metrics: None,
        },
        ..RawRunConfig::default()
    })
    .unwrap();
    config.fingerprint_inputs = vec![FingerprintInputFile {
        path: "pyproject.toml".into(),
        hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
    }];
    config
}
