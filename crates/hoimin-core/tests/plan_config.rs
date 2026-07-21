use hoimin_core::{
    CommandArg, FingerprintInputFile, MutationProfile, OutputConfig, OutputFormat, PlanConfig,
    RawRunConfig, RawRunLimits, RunConfig, SessionConfig,
};

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
        });

    let expected = RunConfig {
        output: OutputConfig {
            format: OutputFormat::Json,
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
