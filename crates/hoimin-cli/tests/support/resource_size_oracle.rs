use super::*;
use hoimin_cli::cli::CliError;
use hoimin_core::{
    ConfigError, DiscoveryConfig, EnvironmentSnapshot, OutputConfig, PlanConfig, RunLimits,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Accepted {
        sizes: BTreeMap<String, u64>,
        jobs: usize,
        max_processes: usize,
    },
    InvalidBytes {
        flag: String,
        value: String,
    },
    InvalidLimit {
        field: String,
    },
    JobsAboveMaximum {
        jobs: usize,
        maximum: usize,
    },
}

impl Observation {
    fn accepted(limits: &RunLimits) -> Self {
        Self::Accepted {
            sizes: [
                ("max-memory", limits.max_memory.get()),
                ("max-output", limits.max_output.get()),
                ("max-copy-size", limits.max_copy_size.get()),
                ("max-workspace-size", limits.max_workspace_size.get()),
                ("min-free-space", limits.min_free_space.get()),
            ]
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
            jobs: limits.jobs.get(),
            max_processes: limits.max_processes.get(),
        }
    }

    fn rejected(error: CliError, context: &str) -> Self {
        match error {
            CliError::InvalidValue { name, value } => Self::InvalidBytes {
                flag: name.to_owned(),
                value,
            },
            CliError::Config(ConfigError::InvalidLimit(field)) => Self::InvalidLimit {
                field: field.to_owned(),
            },
            CliError::Config(ConfigError::JobsExceedsMaximum { jobs, maximum }) => {
                Self::JobsAboveMaximum { jobs, maximum }
            }
            error => panic!("{context}: unexpected CLI error: {error}"),
        }
    }
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    sizes: Vec<(String, String)>,
    jobs: usize,
    max_processes: usize,
    expected: Observation,
}

fn replay(fixture: &Fixture, command: &str, parse: ConfigParser) {
    let context = format!("{command}: {}", fixture.id);
    let mut options = vec![
        format!("--jobs={}", fixture.jobs),
        format!("--max-processes={}", fixture.max_processes),
    ];
    options.extend(
        fixture
            .sizes
            .iter()
            .map(|(flag, input)| format!("--{flag}={input}")),
    );
    let mut args = vec!["hoimin", command, "--file", "src/calc.py"];
    args.extend(options.iter().map(String::as_str));
    args.extend(["--", "python"]);
    let actual = match parse(&args) {
        Ok(config) => {
            let accepted = Observation::accepted(&config.limits);
            if command == "plan" {
                let saved = config
                    .with_environment(EnvironmentSnapshot::Untracked)
                    .unwrap()
                    .into_plan_config(DiscoveryConfig::new(Duration::from_secs(30)).unwrap());
                let encoded = serde_json::to_vec(&saved).unwrap();
                let decoded: PlanConfig = serde_json::from_slice(&encoded).unwrap();
                let restored = decoded.into_run_config(OutputConfig::default()).unwrap();
                assert_eq!(
                    Observation::accepted(&restored.limits),
                    accepted,
                    "{context}: plan round trip"
                );
            }
            accepted
        }
        Err(error) => Observation::rejected(error, &context),
    };
    assert_eq!(actual, fixture.expected, "{context}");
}

#[test]
fn resource_size_normalization_matches_lean() {
    let mut ids = BTreeSet::new();
    for line in include_str!("../../../../formal/HoiminOracle/corpus/resource-sizes.jsonl").lines()
    {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "strict", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        for (command, parse) in CONFIG_PARSERS {
            replay(&fixture, command, parse);
        }
    }
    assert_eq!(ids.len(), 272);
}
