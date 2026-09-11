use hoimin_core::{
    CommandArg, ConfigError, FingerprintInputFile, MAX_JOBS, MAX_TIMEOUT, MutationOperator,
    MutationProfile, OutputConfig, OutputFormat, PlanConfig, RawRunConfig, RawRunLimits, RunConfig,
    SessionConfig,
};

fn plan_value() -> serde_json::Value {
    serde_json::to_value(fixture_run_config().into_plan_config()).unwrap()
}

#[test]
fn disk_limits_have_mandatory_eight_and_ten_gibibyte_defaults() {
    let raw = RawRunLimits::default();
    assert_eq!(raw.max_workspace_size, 8 * 1024 * 1024 * 1024);
    assert_eq!(raw.min_free_space, 10 * 1024 * 1024 * 1024);

    let normalized = RunConfig::try_from(RawRunConfig {
        files: vec!["src/lib.py".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    })
    .unwrap();
    assert_eq!(
        normalized.limits.max_workspace_size.get(),
        8 * 1024 * 1024 * 1024
    );
    assert_eq!(
        normalized.limits.min_free_space.get(),
        10 * 1024 * 1024 * 1024
    );
}

#[test]
fn zero_disk_limits_name_the_public_flags() {
    for (field, expected) in [
        ("max_workspace_size", "--max-workspace-size"),
        ("min_free_space", "--min-free-space"),
    ] {
        let mut raw = RawRunLimits::default();
        match field {
            "max_workspace_size" => raw.max_workspace_size = 0,
            "min_free_space" => raw.min_free_space = 0,
            _ => unreachable!(),
        }
        let error = hoimin_core::RunLimits::try_from(&raw).unwrap_err();
        assert!(error.to_string().contains(expected), "{field}: {error}");
    }
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
fn plan_config_serializes_the_mapping_operator_with_its_canonical_name() {
    let mut config = fixture_run_config();
    config.operators.exclude(MutationOperator::CompareEqNe);
    config.operators.include(MutationOperator::TypeMapping);

    let value = serde_json::to_value(config.into_plan_config()).unwrap();

    assert_eq!(value["operators"], serde_json::json!(["type_dict_mapping"]));
}

#[test]
fn plan_config_accepts_the_historical_mapping_operator_name() {
    let mut value = plan_value();
    value["operators"] = serde_json::json!(["type_mapping"]);

    let plan: PlanConfig = serde_json::from_value(value).unwrap();

    assert!(plan.operators.contains(MutationOperator::TypeMapping));
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

#[test]
fn normalized_timeout_limits_enforce_the_inclusive_ceiling() {
    for (name, encoded_name) in [
        ("analyzer_timeout", "analyzer_timeout"),
        ("baseline_timeout", "baseline_timeout"),
        ("mutant_timeout", "mutant_timeout"),
        ("total_timeout", "total_timeout"),
    ] {
        let mut accepted = plan_value();
        accepted["limits"]["mutant_timeout"] = serde_json::json!({
            "Fixed": {"secs": 5, "nanos": 0}
        });
        let maximum = serde_json::json!({
            "secs": MAX_TIMEOUT.as_secs(),
            "nanos": MAX_TIMEOUT.subsec_nanos()
        });
        if name == "mutant_timeout" {
            accepted["limits"][encoded_name] = serde_json::json!({"Fixed": maximum});
        } else {
            accepted["limits"][encoded_name] = maximum;
        }
        let config: PlanConfig = serde_json::from_value(accepted).unwrap();
        assert_eq!(config.validate(), Ok(()), "accepted {name}");

        let mut rejected = plan_value();
        rejected["limits"]["mutant_timeout"] = serde_json::json!({
            "Fixed": {"secs": 5, "nanos": 0}
        });
        let oversized = MAX_TIMEOUT + std::time::Duration::from_nanos(1);
        let oversized = serde_json::json!({
            "secs": oversized.as_secs(),
            "nanos": oversized.subsec_nanos()
        });
        if name == "mutant_timeout" {
            rejected["limits"][encoded_name] = serde_json::json!({"Fixed": oversized});
        } else {
            rejected["limits"][encoded_name] = oversized;
        }
        let config: PlanConfig = serde_json::from_value(rejected).unwrap();
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidLimit(name)),
            "rejected {name}"
        );
    }
}

#[test]
fn normalized_auto_timeout_rejects_a_baseline_with_oversized_derived_timeout() {
    let maximum_baseline = MAX_TIMEOUT
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap()
        / 2;
    let mut value = plan_value();
    value["limits"]["baseline_timeout"] = serde_json::json!({
        "secs": maximum_baseline.as_secs(),
        "nanos": maximum_baseline.subsec_nanos() + 1
    });
    value["limits"]["mutant_timeout"] = serde_json::json!("Auto");
    let config: PlanConfig = serde_json::from_value(value).unwrap();

    assert_eq!(
        config.validate(),
        Err(ConfigError::InvalidLimit("baseline_timeout"))
    );
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

#[test]
fn import_roots_round_trip_and_missing_historical_run_field_defaults_empty() {
    let mut config = fixture_run_config();
    config.import_roots = vec!["vendor".into(), "src".into(), ".".into()];
    let plan: PlanConfig =
        serde_json::from_value(serde_json::to_value(config.clone().into_plan_config()).unwrap())
            .unwrap();
    plan.validate().unwrap();
    assert_eq!(
        plan.into_run_config(config.output.clone()).import_roots,
        config.import_roots
    );
    let mut old = serde_json::to_value(config).unwrap();
    old.as_object_mut().unwrap().remove("import_roots");
    let old: RunConfig = serde_json::from_value(old).unwrap();
    assert!(old.import_roots.is_empty());
}

#[test]
fn normalized_run_and_plan_import_roots_cannot_bypass_validation() {
    for roots in [
        vec!["../outside"],
        vec!["/absolute"],
        vec!["src", "src"],
        vec!["src/./pkg"],
        vec![""],
        vec!["src/../src"],
    ] {
        let mut config = fixture_run_config();
        config.import_roots = roots.iter().map(|root| (*root).into()).collect();
        assert!(config.validate().is_err(), "{roots:?}");
        assert!(config.into_plan_config().validate().is_err(), "{roots:?}");
    }
}
