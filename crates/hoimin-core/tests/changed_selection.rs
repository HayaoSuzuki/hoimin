use hoimin_core::{
    ChangedSelection, CommandArg, ConfigError, MAX_CHANGED_CONTEXT, OutputConfig, OutputFormat,
    PlanConfig, RawRunConfig, RunConfig,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn constructor_enforces_supported_context(context: u32) {
        // Arrange: arbitrary input spans the supported boundary and integer maximum.
        let maximum = 1_073_741_823;
        // Act.
        let result = ChangedSelection::new(context, None);
        // Assert.
        prop_assert_eq!(result.is_ok(), context <= maximum);
    }

    #[test]
    fn deserialization_enforces_supported_context(context: u32) {
        // Arrange.
        let value = serde_json::json!({"context": context, "diff_base": null});
        // Act.
        let result = serde_json::from_value::<ChangedSelection>(value);
        // Assert.
        prop_assert_eq!(result.is_ok(), context <= 1_073_741_823);
    }

    #[test]
    fn raw_disabled_selection_rejects_nonzero_context(context in 1..=1_073_741_823_u32) {
        // Arrange.
        let raw = RawRunConfig {
            files: vec!["src/lib.py".into()],
            changed_context: context,
            test_argv: vec![CommandArg::Unix(b"python".to_vec())],
            ..RawRunConfig::default()
        };
        // Act.
        let result = RunConfig::try_from(raw);
        // Assert.
        prop_assert_eq!(result.unwrap_err(), ConfigError::ChangedContextRequiresChanged);
    }

    #[test]
    fn raw_normalization_rejects_oversized_context(
        enabled: bool,
        context in 1_073_741_824_u32..=u32::MAX,
    ) {
        // Arrange.
        let raw = RawRunConfig {
            sources: vec!["src".into()],
            changed: enabled,
            changed_context: context,
            test_argv: vec![CommandArg::Unix(b"python".to_vec())],
            ..RawRunConfig::default()
        };
        // Act.
        let result = RunConfig::try_from(raw);
        // Assert.
        prop_assert_eq!(result.unwrap_err(), ConfigError::ChangedContextTooLarge {
            context,
            maximum: 1_073_741_823,
        });
    }

    #[test]
    fn changed_selection_survives_plan_run_roundtrip(context in 0..=MAX_CHANGED_CONTEXT, base in proptest::option::of(prop_oneof![Just(String::new()), "(?s).{1,30}"])) {
        // Arrange.
        let raw = RawRunConfig {
            sources: vec!["src".into()],
            changed: true,
            changed_context: context,
            diff_base: base.clone(),
            test_argv: vec![CommandArg::Unix(b"python".to_vec())],
            ..RawRunConfig::default()
        };
        let plan = RunConfig::try_from(raw).unwrap().with_environment(hoimin_core::EnvironmentSnapshot::Untracked).unwrap().into_plan_config(hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap());
        // Act.
        let restored: PlanConfig = serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
        let run = restored.into_run_config(OutputConfig { format: OutputFormat::Json, metrics: None });
        // Assert.
        let changed = run.selection.changed.unwrap();
        prop_assert_eq!(changed.context(), context);
        prop_assert_eq!(changed.diff_base(), base.as_deref());
    }
}

#[test]
fn constructor_reports_context_bound() {
    // Arrange.
    let context = 1_073_741_824;
    // Act.
    let result = ChangedSelection::new(context, None);
    // Assert.
    assert_eq!(
        result,
        Err(ConfigError::ChangedContextTooLarge {
            context,
            maximum: 1_073_741_823
        })
    );
}

#[test]
fn saved_changed_value_rejects_malformed_representation() {
    // Arrange: each value would lose or invent part of the requested scope.
    for value in [
        serde_json::json!(true),
        serde_json::json!(false),
        serde_json::json!({"diff_base": "main"}),
        serde_json::json!({"context": 0, "changed_context": 1}),
        serde_json::json!({"context": -1}),
        serde_json::json!({"context": 4_294_967_296_u64}),
    ] {
        // Act.
        let result = serde_json::from_value::<ChangedSelection>(value.clone());
        // Assert.
        assert!(result.is_err(), "accepted {value}");
    }
}
