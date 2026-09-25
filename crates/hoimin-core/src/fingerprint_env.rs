use crate::ConfigError;

/// Normalizes portable environment identifiers using the host's name matching.
///
/// # Errors
/// Returns an error for names outside `[A-Za-z_][A-Za-z0-9_]*`.
pub fn normalize_fingerprint_env_names(names: &[String]) -> Result<Vec<String>, ConfigError> {
    normalize_names(names, cfg!(windows))
}

fn normalize_names(names: &[String], windows: bool) -> Result<Vec<String>, ConfigError> {
    let mut normalized = Vec::with_capacity(names.len());
    for name in names {
        let mut bytes = name.bytes();
        if !bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(ConfigError::InvalidFingerprintEnvName);
        }
        normalized.push(if windows {
            name.to_ascii_uppercase()
        } else {
            name.clone()
        });
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

pub(crate) fn validate_fingerprint_env(
    names: &[String],
    hash: Option<&str>,
    prepared: bool,
) -> Result<(), ConfigError> {
    if normalize_fingerprint_env_names(names)? != names {
        return Err(ConfigError::NonNormalizedFingerprintEnv);
    }
    if let Some(hash) = hash {
        if names.is_empty()
            || hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ConfigError::InvalidFingerprintEnvHash);
        }
    } else if prepared && !names.is_empty() {
        return Err(ConfigError::InvalidFingerprintEnvHash);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_platform_case_rules_and_set_semantics() {
        let names = vec!["Path".into(), "PATH".into(), "Path".into(), "Z_1".into()];
        assert_eq!(normalize_names(&names, true).unwrap(), ["PATH", "Z_1"]);
        assert_eq!(
            normalize_names(&names, false).unwrap(),
            ["PATH", "Path", "Z_1"]
        );
        for name in ["", "1BAD", "A=B", "A\0B", "A-B", "日本語", "A B"] {
            assert!(normalize_names(&[name.into()], false).is_err(), "{name:?}");
        }
    }

    #[test]
    fn persisted_snapshot_requires_consistent_canonical_metadata() {
        let names = vec!["FLAG".into()];
        let hash = "a".repeat(64);
        assert!(validate_fingerprint_env(&names, Some(&hash), true).is_ok());
        assert!(validate_fingerprint_env(&names, None, false).is_ok());
        assert!(validate_fingerprint_env(&[], None, true).is_ok());
        assert!(validate_fingerprint_env(&names, None, true).is_err());
        assert!(validate_fingerprint_env(&[], Some(&hash), true).is_err());
        for hash in [
            String::new(),
            "a".repeat(63),
            "a".repeat(65),
            "A".repeat(64),
            "g".repeat(64),
        ] {
            assert!(validate_fingerprint_env(&names, Some(&hash), true).is_err());
        }
        assert!(validate_fingerprint_env(&["Z".into(), "A".into()], Some(&hash), true).is_err());
        assert!(validate_fingerprint_env(&["A".into(), "A".into()], Some(&hash), true).is_err());
    }
    #[test]
    fn legacy_normalized_config_defaults_to_untracked() {
        let config = crate::RunConfig::try_from(crate::RawRunConfig {
            files: vec!["calc.py".into()],
            test_argv: vec![crate::CommandArg::Unix(b"python".to_vec())],
            ..crate::RawRunConfig::default()
        })
        .unwrap();
        let encoded = serde_json::to_value(&config).unwrap();
        assert!(encoded.get("fingerprint_env").is_none());
        assert!(encoded.get("fingerprint_env_hash").is_none());
        let decoded: crate::RunConfig = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, config);
        decoded.validate().unwrap();
        let plan = config.into_plan_config();
        let decoded: crate::PlanConfig =
            serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
        assert_eq!(decoded, plan);
        decoded.validate().unwrap();
    }

    #[test]
    fn tracked_names_and_digest_both_participate_in_run_compatibility() {
        let mut config = crate::RunConfig::try_from(crate::RawRunConfig {
            files: vec!["calc.py".into()],
            test_argv: vec![crate::CommandArg::Unix(b"python".to_vec())],
            fingerprint_env: vec!["B".into(), "A".into(), "B".into()],
            ..crate::RawRunConfig::default()
        })
        .unwrap();
        assert_eq!(config.fingerprint_env, ["A", "B"]);
        let make = |config: &crate::RunConfig| {
            crate::fingerprint(&crate::FingerprintInput::from_config(
                config,
                Vec::new(),
                Vec::new(),
                crate::ResourceMode::BestEffort,
            ))
        };
        config.fingerprint_env_hash = Some("a".repeat(64));
        let first = make(&config);
        config.fingerprint_env_hash = Some("b".repeat(64));
        assert_ne!(first, make(&config));
        config.fingerprint_env_hash = Some("a".repeat(64));
        config.fingerprint_env = vec!["C".into()];
        assert_ne!(first, make(&config));
    }
}
