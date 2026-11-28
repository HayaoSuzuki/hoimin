use super::*;
use hoimin_cli::cli::CliError;
use hoimin_core::{
    ConfigError, DiscoveryConfig, EnvironmentSnapshot, OutputConfig, PlanConfig, RunConfig,
};
use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Accepted { operators: Vec<String> },
    Unknown { selector: String },
    Empty,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    args: Vec<String>,
    expected: Observation,
}

fn restore(config: RunConfig, command: &str) -> RunConfig {
    if command == "plan" {
        let plan = config.into_plan_config(DiscoveryConfig::new(Duration::from_secs(30)).unwrap());
        let decoded: PlanConfig =
            serde_json::from_slice(&serde_json::to_vec(&plan).unwrap()).unwrap();
        decoded.into_run_config(OutputConfig::default()).unwrap()
    } else {
        serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap()
    }
}

fn replay(fixture: &Fixture, command: &str, parse: ConfigParser) {
    let context = format!("{command}: {}", fixture.id);
    let mut args = vec!["hoimin", command, "--file", "src/calc.py"];
    args.extend(fixture.args.iter().map(String::as_str));
    args.extend(["--", "python"]);
    let actual = match parse(&args) {
        Ok(config) => {
            let mut operators = config.operators.names();
            let saved = config
                .with_environment(EnvironmentSnapshot::Untracked)
                .unwrap();
            let restored = restore(saved, command);
            assert_eq!(
                restored.operators.names(),
                operators,
                "{context}: persisted selector order"
            );
            operators.sort_unstable();
            Observation::Accepted { operators }
        }
        Err(CliError::Config(ConfigError::UnknownMutationOperator { value })) => {
            Observation::Unknown { selector: value }
        }
        Err(CliError::Config(ConfigError::EmptyMutationOperatorSelection)) => Observation::Empty,
        Err(error) => panic!("{context}: unexpected selection error: {error}"),
    };
    assert_eq!(actual, fixture.expected, "{context}");
}

#[test]
fn operator_selection_matches_lean() {
    let mut ids = BTreeSet::new();
    for line in
        include_str!("../../../../formal/HoiminOracle/corpus/operator-selectors.jsonl").lines()
    {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "strict", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        for (command, parse) in CONFIG_PARSERS {
            replay(&fixture, command, parse);
        }
    }
    assert!(ids.contains("defaults"));
    assert!(ids.contains("unknown-include-before-exclude"));
}
