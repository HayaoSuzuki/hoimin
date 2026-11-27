use super::*;
use hoimin_cli::cli::CliError;
use hoimin_core::{
    ConfigError, DiscoveryConfig, EnvironmentSnapshot, OutputConfig, PlanConfig, RunConfig,
    Selection, SymbolSelection,
};
use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Line {
    path: String,
    start: u32,
    end: u32,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Accepted {
        lines: Vec<Line>,
        symbols: Vec<SymbolSelection>,
    },
    InvalidLine {
        value: String,
    },
    InvalidSymbol {
        value: String,
    },
    SymbolNeedsSource,
    Missing,
}

impl Observation {
    fn accepted(selection: &Selection) -> Self {
        Self::Accepted {
            lines: selection
                .lines
                .iter()
                .map(|line| Line {
                    path: line.path.as_str().to_owned(),
                    start: line.range.start,
                    end: line.range.end,
                })
                .collect(),
            symbols: selection.symbols.clone(),
        }
    }
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: bool,
    args: Vec<String>,
    posix: Observation,
    windows: Observation,
}

impl Fixture {
    const fn expected(&self) -> &Observation {
        if cfg!(windows) {
            &self.windows
        } else {
            &self.posix
        }
    }
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
    let id = format!("{command}: {}", fixture.id);
    let mut args = vec!["hoimin", command];
    args.extend(fixture.source.then_some("--source=src"));
    args.extend(fixture.args.iter().map(String::as_str));
    args.extend(["--", "python"]);
    let observation = match parse(&args) {
        Ok(config) => {
            let accepted = Observation::accepted(&config.selection);
            let saved = config
                .with_environment(EnvironmentSnapshot::Untracked)
                .unwrap();
            let restored = restore(saved, command);
            assert_eq!(
                Observation::accepted(&restored.selection),
                accepted,
                "{id}: restored"
            );
            accepted
        }
        Err(CliError::InvalidValue {
            name: "--line",
            value,
        }) => Observation::InvalidLine { value },
        Err(CliError::InvalidValue {
            name: "--symbol",
            value,
        }) => Observation::InvalidSymbol { value },
        Err(CliError::Config(ConfigError::SymbolRequiresSource)) => Observation::SymbolNeedsSource,
        Err(CliError::MissingTargetSelector) => Observation::Missing,
        Err(error) => panic!("{id}: unexpected selector error: {error}"),
    };
    assert_eq!(&observation, fixture.expected(), "{id}");
}

#[test]
fn line_and_symbol_selector_syntax_matches_lean() {
    let mut ids = BTreeSet::new();
    for line in include_str!("../../../../formal/HoiminOracle/corpus/selector-syntax.jsonl").lines()
    {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "strict", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        for (command, parse) in CONFIG_PARSERS {
            replay(&fixture, command, parse);
        }
    }
    assert_eq!(ids.len(), 258);
}
