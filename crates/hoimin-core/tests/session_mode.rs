use hoimin_core::{CommandArg, RawRunConfig, RunConfig};

#[test]
fn normalized_config_rejects_legacy_resume_field() {
    // Arrange
    let raw = RawRunConfig {
        files: vec!["example.py".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    };
    let config = RunConfig::try_from(raw).unwrap();
    let mut wire = serde_json::to_value(config).unwrap();
    wire["resume"] = serde_json::json!(false);

    // Act
    let restored = serde_json::from_value::<RunConfig<hoimin_core::EnvironmentRequest>>(wire);

    // Assert
    assert!(restored.is_err());
}

use hoimin_core::{SessionConfig, SessionMode};
use proptest::prelude::*;

fn session_mode(path: &str, mode: u8) -> SessionMode {
    let config = SessionConfig { path: path.into() };
    match mode {
        0 => SessionMode::Disabled,
        1 => SessionMode::Fresh(config),
        _ => SessionMode::Resume(config),
    }
}

proptest! {
    #[test]
    fn session_mode_round_trips(path in "[a-zA-Z0-9_-]{1,48}\\.sqlite", mode in 0_u8..3) {
        // Arrange
        let expected = session_mode(&path, mode);
        let wire = serde_json::to_value(&expected).unwrap();

        // Act
        let restored = serde_json::from_value::<SessionMode>(wire).unwrap();

        // Assert
        prop_assert_eq!(restored, expected);
    }

    #[test]
    fn raw_session_normalizes_to_requested_mode(path in "[a-z]{1,48}", resume in any::<bool>()) {
        // Arrange
        let expected = session_mode(&path, if resume { 2 } else { 1 });
        let raw = RawRunConfig {
            files: vec!["example.py".into()],
            test_argv: vec![CommandArg::Unix(b"python".to_vec())],
            session: Some(SessionConfig { path: path.into() }),
            resume,
            ..RawRunConfig::default()
        };

        // Act
        let normalized = RunConfig::try_from(raw).unwrap();

        // Assert
        prop_assert_eq!(normalized.session, expected);
    }

    #[test]
    fn path_normalization_preserves_session_intent(path in "[a-z]{1,48}", resume in any::<bool>()) {
        // Arrange
        let mut mode = session_mode("relative.sqlite", if resume { 2 } else { 1 });
        let expected = session_mode(&path, if resume { 2 } else { 1 });

        // Act
        mode.config_mut().unwrap().path = path.into();

        // Assert
        prop_assert_eq!(mode, expected);
    }
}

#[test]
fn resumed_session_requires_path_in_saved_input() {
    // Arrange
    let wire = serde_json::json!({ "mode": "resume" });

    // Act
    let result = serde_json::from_value::<SessionMode>(wire);

    // Assert
    assert!(result.is_err());
}

#[test]
fn disabled_session_rejects_unexpected_path() {
    // Arrange
    let wire = serde_json::json!({ "mode": "disabled", "path": "state.sqlite" });

    // Act
    let result = serde_json::from_value::<SessionMode>(wire);

    // Assert
    assert!(result.is_err());
}
