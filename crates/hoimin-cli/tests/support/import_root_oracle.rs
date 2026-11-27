use super::*;
use hoimin_core::{
    ConfigError, DiscoveryConfig, EnvironmentSnapshot, OutputConfig, PlanConfig, RunConfig,
};
use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Accepted { paths: Vec<String> },
    MissingCliValue,
    Invalid { path: String },
    NonNormalized,
}

fn observe(result: Result<&[camino::Utf8PathBuf], &ConfigError>, context: &str) -> Observation {
    match result {
        Ok(paths) => Observation::Accepted {
            paths: paths.iter().map(|path| path.as_str().to_owned()).collect(),
        },
        Err(ConfigError::InvalidImportRoot { path }) => Observation::Invalid {
            path: path.as_str().to_owned(),
        },
        Err(ConfigError::NonNormalizedImportRoots) => Observation::NonNormalized,
        Err(error) => panic!("{context}: unexpected configuration error: {error}"),
    }
}

#[derive(serde::Deserialize)]
struct Expectations {
    cli: Observation,
    persisted: Observation,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    input: Vec<String>,
    posix: Expectations,
    windows: Expectations,
}

impl Fixture {
    const fn expected(&self) -> &Expectations {
        if cfg!(windows) {
            &self.windows
        } else {
            &self.posix
        }
    }
}

fn replay_cli(fixture: &Fixture, command: &str, parse: ConfigParser) {
    let context = format!("{command}: {}", fixture.id);
    let options: Vec<_> = fixture
        .input
        .iter()
        .map(|path| format!("--import-root={path}"))
        .collect();
    let mut args = vec!["hoimin", command, "--file", "src/calc.py"];
    args.extend(options.iter().map(String::as_str));
    args.extend(["--", "python"]);
    let result = parse(&args);
    let observation = match &result {
        Ok(config) => {
            assert_eq!(config.selection.files, ["src/calc.py"], "{context}");
            assert!(config.selection.sources.is_empty(), "{context}");
            observe(Ok(&config.import_roots), &context)
        }
        Err(hoimin_cli::cli::CliError::Clap(error))
            if error.kind() == clap::error::ErrorKind::InvalidValue
                && error
                    .to_string()
                    .contains("a value is required for '--import-root <DIR>'") =>
        {
            Observation::MissingCliValue
        }
        Err(error) => observe(
            Err(error
                .config_error()
                .unwrap_or_else(|| panic!("{context}: unexpected CLI error: {error}"))),
            &context,
        ),
    };
    assert_eq!(observation, fixture.expected().cli, "{context}");
}

fn replay_persisted(fixture: &Fixture, base: &RunConfig) {
    let id = &fixture.id;
    let expected = &fixture.expected().persisted;
    let mut data = base.clone().into_data();
    data.import_roots = fixture
        .input
        .iter()
        .map(|path| path.as_str().into())
        .collect();
    let original = serde_json::to_vec(&data).unwrap();
    let checked = RunConfig::try_from(data);
    let observation = observe(
        checked
            .as_ref()
            .map(|config| config.import_roots.as_slice()),
        id,
    );
    assert_eq!(&observation, expected, "{id}: checked run configuration");

    let decoded: Result<RunConfig, _> = serde_json::from_slice(&original);
    match checked {
        Ok(config) => assert_eq!(decoded.unwrap(), config, "{id}: saved run configuration"),
        Err(error) => assert!(
            decoded
                .unwrap_err()
                .to_string()
                .contains(&error.to_string()),
            "{id}"
        ),
    }

    let mut plan = base
        .clone()
        .into_plan_config(DiscoveryConfig::new(Duration::from_secs(30)).unwrap());
    plan.import_roots = fixture
        .input
        .iter()
        .map(|path| path.as_str().into())
        .collect();
    let plan: PlanConfig = serde_json::from_slice(&serde_json::to_vec(&plan).unwrap()).unwrap();
    let validated = plan.validate();
    let observation = observe(
        validated.as_ref().map(|()| plan.import_roots.as_slice()),
        id,
    );
    assert_eq!(&observation, expected, "{id}: saved plan validation");
    let restored = plan.into_run_config(OutputConfig::default());
    let observation = observe(
        restored
            .as_ref()
            .map(|config| config.import_roots.as_slice()),
        id,
    );
    assert_eq!(&observation, expected, "{id}: plan execution settings");
}

#[test]
fn import_root_normalization_matches_lean() {
    let base = parse_config_from(["hoimin", "run", "--file", "src/calc.py", "--", "python"])
        .unwrap()
        .with_environment(EnvironmentSnapshot::Untracked)
        .unwrap();
    let mut ids = BTreeSet::new();
    for line in include_str!("../../../../formal/HoiminOracle/corpus/import-roots.jsonl").lines() {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "strict", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        for (command, parse) in CONFIG_PARSERS {
            replay_cli(&fixture, command, parse);
        }
        replay_persisted(&fixture, &base);
    }
    assert_eq!(ids.len(), 85);
}
