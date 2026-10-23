use hoimin_core::{
    CommandArg, ConfigError, FingerprintInputFile, MAX_JOBS, MAX_TIMEOUT, MutationOperator,
    MutationProfile, OutputConfig, PlanConfig, RawRunConfig, RawRunLimits, RunConfig,
    SessionConfig,
};

fn plan_value() -> serde_json::Value {
    serde_json::to_value(fixture_run_config().into_plan_config(
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    ))
    .unwrap()
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
    .unwrap()
    .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
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
    for (raw, expected) in [
        (
            RawRunLimits {
                max_workspace_size: 0,
                ..RawRunLimits::default()
            },
            "--max-workspace-size",
        ),
        (
            RawRunLimits {
                min_free_space: 0,
                ..RawRunLimits::default()
            },
            "--min-free-space",
        ),
    ] {
        let error = hoimin_core::RunLimits::try_from(&raw).unwrap_err();

        assert!(error.to_string().contains(expected), "{expected}: {error}");
    }
}

type InvalidPlanCase = (&'static str, fn(&mut serde_json::Value), ConfigError);

#[test]
fn plan_config_round_trips_run_semantics_with_session_disabled() {
    // Arrange
    let mut data = fixture_run_config().into_data();
    data.session = hoimin_core::SessionMode::Resume(SessionConfig {
        path: "state.sqlite".into(),
    });
    let config = RunConfig::try_from(data).unwrap();
    let value = serde_json::to_value(config.clone().into_plan_config(
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    ))
    .unwrap();
    let mut expected = config.into_data();
    expected.output = OutputConfig::default();
    expected.session = hoimin_core::SessionMode::Disabled;
    let expected = RunConfig::try_from(expected).unwrap();

    // Act
    let restored = serde_json::from_value::<PlanConfig>(value)
        .unwrap()
        .into_run_config(OutputConfig::default())
        .unwrap();

    // Assert
    assert_eq!(restored, expected);
}

#[test]
fn plan_config_rejects_absent_required_arrays() {
    for field in ["import_roots", "fingerprint_files"] {
        for replacement in [None, Some(serde_json::Value::Null)] {
            // Arrange
            let mut value = plan_value();
            value.as_object_mut().unwrap().remove(field);
            if let Some(replacement) = replacement {
                value[field] = replacement;
            }

            // Act
            let result = serde_json::from_value::<PlanConfig>(value);

            // Assert
            assert!(result.is_err(), "accepted absent {field}");
        }
    }
}

#[test]
fn plan_config_preserves_explicit_required_arrays() {
    for paths in [vec![], vec!["vendor", "src"]] {
        // Arrange
        let mut value = plan_value();
        value["import_roots"] = serde_json::json!(paths);
        value["fingerprint_files"] = serde_json::json!(paths);

        // Act
        let plan: PlanConfig = serde_json::from_value(value).unwrap();

        // Assert
        assert_eq!(plan.import_roots, paths);
        assert_eq!(plan.fingerprint_files, paths);
    }
}

#[test]
fn plan_config_serializes_the_mapping_operator_with_its_canonical_name() {
    let mut config = fixture_run_config().into_data();
    config.operators.exclude(MutationOperator::CompareEqNe);
    config.operators.include(MutationOperator::TypeMapping);

    let config = RunConfig::try_from(config).unwrap();
    let value = serde_json::to_value(config.into_plan_config(
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    ))
    .unwrap();

    assert_eq!(value["operators"], serde_json::json!(["type_dict_mapping"]));
}

#[test]
fn plan_config_rejects_the_historical_mapping_operator_name() {
    let mut value = plan_value();
    value["operators"] = serde_json::json!(["type_mapping"]);

    assert!(serde_json::from_value::<PlanConfig>(value).is_err());
}

#[test]
fn valid_normalized_plan_and_run_configs_pass_validation() {
    let run = fixture_run_config();
    let plan = run.clone().into_plan_config(
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    );

    assert_eq!(plan.validate(), Ok(()));
    assert_eq!(run.validate(), Ok(()));
}

#[test]
fn normalized_configs_reject_the_removed_outer_root() {
    for root in ["project", "other-project", "project/./"] {
        let mut run_value = serde_json::to_value(fixture_run_config()).unwrap();
        run_value["root"] = serde_json::json!(root);
        let mut plan = plan_value();
        plan["root"] = serde_json::json!(root);

        let run_result = serde_json::from_value::<RunConfig>(run_value);
        let plan_result = serde_json::from_value::<PlanConfig>(plan);

        assert!(
            run_result
                .unwrap_err()
                .to_string()
                .contains("unknown field `root`")
        );
        assert!(
            plan_result
                .unwrap_err()
                .to_string()
                .contains("unknown field `root`")
        );
    }
}

#[test]
fn normalized_configs_accept_selection_root_without_filesystem_access() {
    let mut run = fixture_run_config().into_data();
    run.selection.root = "nonexistent-project/./src/".into();
    let run = RunConfig::try_from(run).unwrap();
    let plan = run.clone().into_plan_config(
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    );

    let results = [run.validate(), plan.validate()];

    assert_eq!(results, [Ok(()), Ok(())]);
}

proptest::proptest! {
    #[test]
    fn saved_configs_preserve_the_single_project_root(root in "[a-z]{1,12}(/\\./[a-z]{1,12})?/?") {
        let mut config = fixture_run_config().into_data();
        config.selection.root = root.clone().into();
        let mut value = serde_json::to_value(config).unwrap();
        value.as_object_mut().unwrap().remove("root");

        let run: RunConfig = serde_json::from_value(value).unwrap();
        let plan_value = serde_json::to_value(run.into_plan_config(hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap())).unwrap();
        let plan: PlanConfig = serde_json::from_value(plan_value.clone()).unwrap();
        let restored = plan.into_run_config(OutputConfig::default()).unwrap();
        let run_value = serde_json::to_value(&restored).unwrap();

        proptest::prop_assert!(plan_value.get("root").is_none());
        proptest::prop_assert!(run_value.get("root").is_none());
        proptest::prop_assert_eq!(restored.selection.root.as_str(), root);
    }
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
                value["selection"]["changed"] = serde_json::json!(null);
            },
            ConfigError::MissingSelector,
        ),
        (
            "changed without source",
            |value| {
                value["selection"]["sources"] = serde_json::json!([]);
                value["selection"]["changed"] = serde_json::json!({"context":0,"diff_base":null});
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
fn normalized_limit_validation_accepts_more_workers_than_processes_per_tree() {
    // Arrange
    let mut value = plan_value();
    value["limits"]["jobs"] = serde_json::json!(4);
    value["limits"]["max_processes"] = serde_json::json!(1);
    let config: PlanConfig = serde_json::from_value(value).unwrap();

    // Act
    let result = config.validate();

    // Assert
    assert_eq!(result, Ok(()));
}

#[test]
fn normalized_limit_validation_accepts_maximum_jobs() {
    let mut value = plan_value();
    value["limits"]["jobs"] = serde_json::json!(MAX_JOBS);
    value["limits"]["max_processes"] = serde_json::json!(1);
    let config: PlanConfig = serde_json::from_value(value).unwrap();

    assert_eq!(config.validate(), Ok(()));
}

#[test]
fn normalized_timeout_limits_enforce_the_inclusive_ceiling() {
    let maximum = serde_json::json!({
        "secs": MAX_TIMEOUT.as_secs(),
        "nanos": MAX_TIMEOUT.subsec_nanos()
    });
    let oversized = MAX_TIMEOUT + std::time::Duration::from_nanos(1);
    let oversized = serde_json::json!({
        "secs": oversized.as_secs(),
        "nanos": oversized.subsec_nanos()
    });
    for (name, maximum, oversized) in [
        ("baseline_timeout", maximum.clone(), oversized.clone()),
        (
            "mutant_timeout",
            serde_json::json!({"fixed": maximum}),
            serde_json::json!({"fixed": oversized}),
        ),
        ("total_timeout", maximum, oversized),
    ] {
        let mut accepted = plan_value();
        accepted["limits"]["mutant_timeout"] = serde_json::json!({
            "fixed": {"secs": 5, "nanos": 0}
        });
        accepted["limits"][name] = maximum;

        let config: PlanConfig = serde_json::from_value(accepted).unwrap();

        assert_eq!(config.validate(), Ok(()), "accepted {name}");

        let mut rejected = plan_value();
        rejected["limits"]["mutant_timeout"] = serde_json::json!({
            "fixed": {"secs": 5, "nanos": 0}
        });
        rejected["limits"][name] = oversized;

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
    value["limits"]["mutant_timeout"] = serde_json::json!("auto");
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
fn raw_config_rejects_resume_without_session() {
    // Arrange
    let raw = RawRunConfig {
        files: vec!["example.py".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        resume: true,
        ..RawRunConfig::default()
    };

    // Act
    let result = RunConfig::try_from(raw);

    // Assert
    assert_eq!(result, Err(ConfigError::ResumeRequiresSession));
}

fn fixture_run_config() -> RunConfig {
    let config = RunConfig::try_from(RawRunConfig {
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
        output: OutputConfig { metrics: None },
        ..RawRunConfig::default()
    })
    .unwrap()
    .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
    .unwrap();
    config.with_fingerprint_inputs(vec![FingerprintInputFile {
        path: "pyproject.toml".into(),
        hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
    }])
}

#[test]
fn run_config_rejects_absent_required_arrays() {
    for field in ["import_roots", "fingerprint_files"] {
        for replacement in [None, Some(serde_json::Value::Null)] {
            // Arrange
            let mut value = serde_json::to_value(fixture_run_config()).unwrap();
            value.as_object_mut().unwrap().remove(field);
            if let Some(replacement) = replacement {
                value[field] = replacement;
            }

            // Act
            let result = serde_json::from_value::<RunConfig>(value);

            // Assert
            assert!(result.is_err(), "accepted absent {field}");
        }
    }
}

#[test]
fn run_config_preserves_explicit_required_arrays() {
    for paths in [vec![], vec!["vendor", "src"]] {
        // Arrange
        let mut value = serde_json::to_value(fixture_run_config()).unwrap();
        value["import_roots"] = serde_json::json!(paths);
        value["fingerprint_files"] = serde_json::json!(paths);

        // Act
        let run: RunConfig = serde_json::from_value(value).unwrap();

        // Assert
        assert_eq!(run.import_roots, paths);
        assert_eq!(run.fingerprint_files, paths);
    }
}

proptest::proptest! {
    #[test]
    fn saved_configs_preserve_ordered_import_roots(
        roots in proptest::collection::vec("[a-z]{1,8}", 0..8),
    ) {
        // Arrange: valid normalized roots, including meaningful order variations.
        proptest::prop_assume!(roots.iter().collect::<std::collections::BTreeSet<_>>().len() == roots.len());
        let mut config = fixture_run_config().into_data();
        config.import_roots = roots.iter().map(Into::into).collect();

        // Act
        let run: RunConfig = serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        let plan: PlanConfig = serde_json::from_slice(
            &serde_json::to_vec(&RunConfig::try_from(config).unwrap().into_plan_config(hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap())).unwrap(),
        ).unwrap();
        let restored = plan.into_run_config(OutputConfig::default()).unwrap();

        // Assert: neither representation may erase or reorder import precedence.
        proptest::prop_assert_eq!(&run.import_roots, &roots);
        proptest::prop_assert_eq!(&restored.import_roots, &roots);
    }

    #[test]
    fn saved_plan_preserves_explicit_fingerprint_files(
        files in proptest::collection::vec("[a-z]{1,8}\\.toml", 0..8),
    ) {
        // Arrange
        let mut config = fixture_run_config().into_data();
        config.fingerprint_files = files.clone();

        // Act
        let plan: PlanConfig = serde_json::from_slice(
            &serde_json::to_vec(&RunConfig::try_from(config).unwrap().into_plan_config(hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap())).unwrap(),
        ).unwrap();
        let restored = plan.into_run_config(OutputConfig::default()).unwrap();

        // Assert: saved input retains the exact configured fingerprint file list.
        proptest::prop_assert_eq!(&restored.fingerprint_files, &files);
    }
}

macro_rules! invalid_import_root_cases {
    ($($name:ident: $roots:expr;)*) => {
        $(#[test]
        fn $name() {
            // Arrange
            let mut data = fixture_run_config().into_data();
            data.import_roots = $roots.into_iter().map(Into::into).collect();
            let mut plan: PlanConfig = serde_json::from_value(plan_value()).unwrap();
            plan.import_roots.clone_from(&data.import_roots);

            // Act
            let run_result = RunConfig::try_from(data);
            let plan_result = plan.into_run_config(OutputConfig::default());

            // Assert
            assert!(run_result.is_err());
            assert!(plan_result.is_err());
        })*
    };
}

invalid_import_root_cases! {
    checked_import_roots_reject_escape: ["../outside"];
    checked_import_roots_reject_absolute: ["/absolute"];
    checked_import_roots_reject_duplicates: ["src", "src"];
    checked_import_roots_reject_dot_component: ["src/./pkg"];
    checked_import_roots_reject_empty: [""];
    checked_import_roots_reject_parent_component: ["src/../src"];
}

#[test]
fn saved_selection_rejects_obsolete_sibling_fields() {
    // Arrange: an otherwise valid saved selection with a discarded legacy field.
    for field in ["changed_context", "diff_base"] {
        let mut value = plan_value();
        value["selection"][field] = serde_json::json!(null);

        // Act.
        let result = serde_json::from_value::<PlanConfig>(value);

        // Assert: silently dropping the field could change the selected targets.
        assert!(result.is_err(), "accepted obsolete {field}");
    }
}

#[test]
fn raw_config_keeps_the_default_for_omitted_import_roots() {
    // Arrange
    let mut value = serde_json::to_value(RawRunConfig::default()).unwrap();
    value.as_object_mut().unwrap().remove("import_roots");

    // Act
    let raw: RawRunConfig = serde_json::from_value(value).unwrap();

    // Assert
    assert_eq!(raw.import_roots, Vec::<camino::Utf8PathBuf>::new());
}

#[test]
fn discovery_timeout_requires_a_positive_bounded_duration() {
    // Arrange: finite boundary cases, including the inclusive upper bound.
    for (name, duration, accepted) in [
        ("zero", std::time::Duration::ZERO, false),
        ("minimum", std::time::Duration::from_nanos(1), true),
        ("maximum", MAX_TIMEOUT, true),
        (
            "overflow",
            MAX_TIMEOUT + std::time::Duration::from_nanos(1),
            false,
        ),
    ] {
        // Act.
        let result = hoimin_core::DiscoveryConfig::new(duration);
        // Assert.
        assert_eq!(result.is_ok(), accepted, "{name}");
        if let Err(error) = result {
            assert_eq!(error, ConfigError::InvalidLimit("analyzer_timeout"));
        }
    }
}

#[test]
fn saved_plan_requires_complete_discovery_settings() {
    // Arrange.
    for (name, discovery) in [
        ("null", serde_json::Value::Null),
        ("missing_timeout", serde_json::json!({})),
        (
            "zero",
            serde_json::json!({"analyzer_timeout": {"secs": 0, "nanos": 0}}),
        ),
        (
            "unknown_field",
            serde_json::json!({"analyzer_timeout": {"secs": 30, "nanos": 0}, "unknown": true}),
        ),
    ] {
        let mut value = plan_value();
        value["discovery"] = discovery;
        // Act.
        let result = serde_json::from_value::<PlanConfig>(value);
        // Assert.
        assert!(result.is_err(), "{name}");
    }
}

#[test]
fn saved_plan_rejects_missing_discovery() {
    // Arrange.
    let mut value = plan_value();
    value.as_object_mut().unwrap().remove("discovery");
    // Act.
    let error = serde_json::from_value::<PlanConfig>(value).unwrap_err();
    // Assert.
    assert!(error.to_string().contains("discovery"));
}

#[test]
fn saved_plan_rejects_oversized_discovery_timeout_during_validation() {
    // Arrange.
    let mut value = plan_value();
    value["discovery"]["analyzer_timeout"] =
        serde_json::json!({"secs": MAX_TIMEOUT.as_secs(), "nanos": 1});
    let plan: PlanConfig = serde_json::from_value(value).unwrap();
    // Act.
    let result = plan.validate();
    // Assert.
    assert_eq!(result, Err(ConfigError::InvalidLimit("analyzer_timeout")));
}

#[test]
fn saved_plan_rejects_obsolete_runtime_analyzer_timeout() {
    // Arrange.
    let mut value = plan_value();
    value["limits"]["analyzer_timeout"] = serde_json::json!({"secs": 30, "nanos": 0});
    // Act.
    let error = serde_json::from_value::<PlanConfig>(value).unwrap_err();
    // Assert.
    assert!(
        error
            .to_string()
            .contains("unknown field `analyzer_timeout`")
    );
}
