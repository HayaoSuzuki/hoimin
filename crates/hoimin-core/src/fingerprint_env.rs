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

/// Canonical names requested for capture; no environment values have been read.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct EnvironmentRequest(Vec<String>);

impl EnvironmentRequest {
    /// Creates a request using host name matching, sorting, and duplicate removal.
    ///
    /// # Errors
    /// Returns an error for an invalid portable environment identifier.
    pub fn new(names: &[String]) -> Result<Self, ConfigError> {
        normalize_fingerprint_env_names(names).map(Self)
    }

    /// Returns the canonical selected names.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for EnvironmentRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let names = Vec::<String>::deserialize(deserializer)?;
        Self::new(&names).map_err(serde::de::Error::custom)
    }
}

/// Validated inherited-environment metadata; captured values are never retained.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    from = "EnvironmentSnapshotWire"
)]
pub enum EnvironmentSnapshot {
    /// No inherited variables participate in compatibility.
    #[default]
    Untracked,
    /// A nonempty canonical name set and its aggregate native-value digest.
    Captured(CapturedEnvironment),
}

// Serde's internally tagged unit variants ignore extra fields. An empty struct
// variant makes untracked metadata closed without complicating the public API.
#[derive(serde::Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum EnvironmentSnapshotWire {
    Untracked {},
    Captured(CapturedEnvironment),
}

impl From<EnvironmentSnapshotWire> for EnvironmentSnapshot {
    fn from(value: EnvironmentSnapshotWire) -> Self {
        match value {
            EnvironmentSnapshotWire::Untracked {} => Self::Untracked,
            EnvironmentSnapshotWire::Captured(value) => Self::Captured(value),
        }
    }
}

/// Captured metadata whose names and digest cannot be independently changed.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "CapturedEnvironmentWire")]
pub struct CapturedEnvironment {
    names: Vec<String>,
    digest: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CapturedEnvironmentWire {
    names: Vec<String>,
    digest: String,
}

impl TryFrom<CapturedEnvironmentWire> for CapturedEnvironment {
    type Error = ConfigError;

    fn try_from(value: CapturedEnvironmentWire) -> Result<Self, Self::Error> {
        if normalize_fingerprint_env_names(&value.names)? != value.names {
            return Err(ConfigError::NonNormalizedFingerprintEnv);
        }
        if value.names.is_empty()
            || value.digest.len() != 64
            || !value
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ConfigError::InvalidFingerprintEnvHash);
        }
        Ok(Self {
            names: value.names,
            digest: value.digest,
        })
    }
}

impl EnvironmentSnapshot {
    /// Creates captured metadata from canonical names and a lowercase BLAKE3 digest.
    ///
    /// # Errors
    /// Returns an error for empty/noncanonical names or a digest outside 64 lowercase hex digits.
    pub fn captured(names: Vec<String>, digest: String) -> Result<Self, ConfigError> {
        CapturedEnvironment::try_from(CapturedEnvironmentWire { names, digest }).map(Self::Captured)
    }

    /// Returns canonical selected names, or an empty slice when untracked.
    #[must_use]
    pub fn names(&self) -> &[String] {
        match self {
            Self::Untracked => &[],
            Self::Captured(value) => &value.names,
        }
    }

    /// Returns the aggregate digest, or none when untracked.
    #[must_use]
    pub fn digest(&self) -> Option<&str> {
        match self {
            Self::Untracked => None,
            Self::Captured(value) => Some(&value.digest),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_follow_platform_case_rules_and_set_semantics() {
        let names = vec!["Path".into(), "PATH".into(), "Path".into(), "Z_1".into()];
        assert_eq!(normalize_names(&names, true).unwrap(), ["PATH", "Z_1"]);
        assert_eq!(
            normalize_names(&names, false).unwrap(),
            ["PATH", "Path", "Z_1"]
        );
        for name in ["", "1BAD", "A=B", "A\0B", "A-B", "日本語", "A B"] {
            assert!(EnvironmentRequest::new(&[name.into()]).is_err());
        }
        assert_eq!(
            EnvironmentRequest::new(&["B".into(), "A".into(), "B".into()])
                .unwrap()
                .names(),
            ["A", "B"]
        );
    }

    #[test]
    fn snapshot_deserialization_enforces_complete_canonical_metadata() {
        let valid = json!({"state":"captured", "names":["FLAG"], "digest":"a".repeat(64)});
        let snapshot: EnvironmentSnapshot = serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(serde_json::to_value(&snapshot).unwrap(), valid);
        assert_eq!(snapshot.names(), ["FLAG"]);
        assert_eq!(snapshot.digest(), Some("a".repeat(64).as_str()));
        for names in [
            json!([]),
            json!(["Z", "A"]),
            json!(["A", "A"]),
            json!(["A=B"]),
        ] {
            let mut invalid = valid.clone();
            invalid["names"] = names;
            assert!(serde_json::from_value::<EnvironmentSnapshot>(invalid).is_err());
        }
        for digest in [
            json!(null),
            json!(""),
            json!("a".repeat(63)),
            json!("a".repeat(65)),
            json!("A".repeat(64)),
            json!("g".repeat(64)),
            json!(42),
        ] {
            let mut invalid = valid.clone();
            invalid["digest"] = digest;
            assert!(serde_json::from_value::<EnvironmentSnapshot>(invalid).is_err());
        }
        for key in ["names", "digest", "state"] {
            let mut invalid = valid.clone();
            invalid.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<EnvironmentSnapshot>(invalid).is_err());
        }
        for invalid in [
            json!({"state":"untracked","digest":"a".repeat(64)}),
            json!({"state":"untracked","names":[]}),
            json!({"state":"pending"}),
            json!({"state":"captured","names":["FLAG"],"digest":"a".repeat(64),"value":"private"}),
        ] {
            assert!(serde_json::from_value::<EnvironmentSnapshot>(invalid).is_err());
        }
        assert!(EnvironmentSnapshot::captured(vec![], "a".repeat(64)).is_err());
        assert!(EnvironmentSnapshot::captured(vec!["FLAG".into()], "bad".into()).is_err());
    }

    #[test]
    fn request_transition_requires_the_selected_snapshot_names() {
        let request = crate::RunConfig::try_from(crate::RawRunConfig {
            files: vec!["calc.py".into()],
            test_argv: vec![crate::CommandArg::Unix(b"python".to_vec())],
            fingerprint_env: vec!["FLAG".into()],
            ..crate::RawRunConfig::default()
        })
        .unwrap();
        assert!(
            request
                .clone()
                .with_environment(EnvironmentSnapshot::Untracked)
                .is_err()
        );
        assert!(
            request
                .clone()
                .with_environment(
                    EnvironmentSnapshot::captured(vec!["OTHER".into()], "a".repeat(64)).unwrap()
                )
                .is_err()
        );
        let config = request
            .with_environment(
                EnvironmentSnapshot::captured(vec!["FLAG".into()], "a".repeat(64)).unwrap(),
            )
            .unwrap();
        let plan = config.clone().into_plan_config();
        assert_eq!(plan.environment, config.environment);
        assert_eq!(
            plan.into_run_config(config.output.clone()).environment,
            config.environment
        );
    }

    #[test]
    fn untracked_snapshot_is_explicit_in_current_config() {
        let config = crate::RunConfig::try_from(crate::RawRunConfig {
            files: vec!["calc.py".into()],
            test_argv: vec![crate::CommandArg::Unix(b"python".to_vec())],
            ..crate::RawRunConfig::default()
        })
        .unwrap()
        .with_environment(EnvironmentSnapshot::Untracked)
        .unwrap();
        let encoded = serde_json::to_value(&config).unwrap();
        assert_eq!(encoded["environment"], json!({"state":"untracked"}));
        assert!(encoded.get("fingerprint_env").is_none());
        assert!(encoded.get("fingerprint_env_hash").is_none());
        let decoded: crate::RunConfig = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(decoded, config);
        let mut omitted = encoded;
        omitted.as_object_mut().unwrap().remove("environment");
        assert!(serde_json::from_value::<crate::RunConfig>(omitted).is_err());
    }
}
