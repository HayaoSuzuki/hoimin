use super::*;
use hoimin_cli::cli::CliError;
use hoimin_core::{
    ChangedSelection, CommandArg, ConfigError, DiscoveryConfig, EnvironmentSnapshot, LineRange,
    LineSelection, OutputConfig, PlanConfig, RawRunConfig, RunConfig, Selection, SymbolSelection,
};
use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Selector {
    Source,
    File,
    Line,
    Symbol,
}

impl Selector {
    const fn arg(self) -> &'static str {
        match self {
            Self::Source => "--source=src",
            Self::File => "--file=src/calc.py",
            Self::Line => "--line=src/calc.py:1-2",
            Self::Symbol => "--symbol=calc:total",
        }
    }
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Scope {
    context: u32,
    diff_base: Option<String>,
}

impl From<&ChangedSelection> for Scope {
    fn from(scope: &ChangedSelection) -> Self {
        Self {
            context: scope.context(),
            diff_base: scope.diff_base().map(str::to_owned),
        }
    }
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Accepted { changed: Option<Scope> },
    InvalidCliContext,
    CliContextRequiresChanged,
    MissingSelector,
    ChangedRequiresSource,
    SymbolRequiresSource,
    ContextRequiresChanged,
    DiffBaseRequiresChanged,
    ContextTooLarge { context: u32, maximum: u32 },
}

impl Observation {
    fn from_config<E>(result: Result<RunConfig<E>, ConfigError>, id: &str) -> Self {
        match result {
            Ok(config) => Self::accepted(&config.selection),
            Err(ConfigError::MissingSelector) => Self::MissingSelector,
            Err(ConfigError::ChangedRequiresSource) => Self::ChangedRequiresSource,
            Err(ConfigError::SymbolRequiresSource) => Self::SymbolRequiresSource,
            Err(ConfigError::ChangedContextRequiresChanged) => Self::ContextRequiresChanged,
            Err(ConfigError::DiffBaseRequiresChanged) => Self::DiffBaseRequiresChanged,
            Err(ConfigError::ChangedContextTooLarge { context, maximum }) => {
                Self::ContextTooLarge { context, maximum }
            }
            Err(error) => panic!("{id}: unexpected config error: {error}"),
        }
    }

    fn accepted(selection: &Selection) -> Self {
        Self::Accepted {
            changed: selection.changed.as_ref().map(Scope::from),
        }
    }

    fn saved_error(&self, id: &str) -> ConfigError {
        match *self {
            Self::ContextTooLarge { context, maximum } => {
                ConfigError::ChangedContextTooLarge { context, maximum }
            }
            Self::MissingSelector => ConfigError::MissingSelector,
            Self::ChangedRequiresSource => ConfigError::ChangedRequiresSource,
            Self::SymbolRequiresSource => ConfigError::SymbolRequiresSource,
            _ => panic!("{id}: unexpected rejection of saved config"),
        }
    }
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    selectors: Vec<Selector>,
    enabled: bool,
    context: Option<u32>,
    diff_base: Option<String>,
    saved_scope: Option<Scope>,
    cli: Observation,
    raw: Observation,
    saved: Observation,
}

fn replay_cli(fixture: &Fixture, command: &str, parse: ConfigParser) {
    let id = format!("{command}: {}", fixture.id);
    let context = fixture
        .context
        .map(|value| format!("--changed-context={value}"));
    let base = fixture
        .diff_base
        .as_ref()
        .map(|value| format!("--diff-base={value}"));
    let mut args = vec!["hoimin", command];
    args.extend(fixture.selectors.iter().map(|selector| selector.arg()));
    args.extend(fixture.enabled.then_some("--changed"));
    args.extend(context.as_deref());
    args.extend(base.as_deref());
    args.extend(["--", "python"]);
    let observation = match parse(&args) {
        Ok(config) => Observation::accepted(&config.selection),
        Err(CliError::MissingTargetSelector) => Observation::MissingSelector,
        Err(CliError::Config(error)) => {
            Observation::from_config::<EnvironmentSnapshot>(Err(error), &id)
        }
        Err(CliError::Clap(error)) => match error.kind() {
            clap::error::ErrorKind::ValueValidation => Observation::InvalidCliContext,
            clap::error::ErrorKind::MissingRequiredArgument => {
                Observation::CliContextRequiresChanged
            }
            _ => panic!("{id}: unexpected CLI error: {error}"),
        },
        Err(error) => panic!("{id}: unexpected CLI error: {error}"),
    };
    assert_eq!(observation, fixture.cli, "{id}");
}

fn raw_config(fixture: &Fixture) -> RawRunConfig {
    let mut raw = RawRunConfig {
        changed: fixture.enabled,
        changed_context: fixture.context.unwrap_or(0),
        diff_base: fixture.diff_base.clone(),
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    };
    for selector in &fixture.selectors {
        match selector {
            Selector::Source => raw.sources.push("src".into()),
            Selector::File => raw.files.push("src/calc.py".into()),
            Selector::Line => raw.lines.push(LineSelection {
                path: "src/calc.py".into(),
                range: LineRange { start: 1, end: 2 },
            }),
            Selector::Symbol => raw.symbols.push(SymbolSelection {
                module: "calc".into(),
                qualname: "total".into(),
            }),
        }
    }
    raw
}

fn replay_saved(fixture: &Fixture, base: &RunConfig, saved_plan: &serde_json::Value) {
    let id = &fixture.id;
    let raw = raw_config(fixture);
    let mut selection = serde_json::to_value(Selection {
        sources: raw.sources,
        files: raw.files,
        lines: raw.lines,
        symbols: raw.symbols,
        ..base.selection.clone()
    })
    .unwrap();
    selection["changed"] = serde_json::to_value(&fixture.saved_scope).unwrap();

    let mut run = serde_json::to_value(base).unwrap();
    run["selection"] = selection.clone();
    match serde_json::from_value::<RunConfig>(run) {
        Ok(config) => assert_eq!(
            Observation::accepted(&config.selection),
            fixture.saved,
            "{id}"
        ),
        Err(error) => assert!(
            error
                .to_string()
                .contains(&fixture.saved.saved_error(id).to_string()),
            "{id}: {error}"
        ),
    }

    let mut plan = saved_plan.clone();
    plan["selection"] = selection;
    match serde_json::from_value::<PlanConfig>(plan) {
        Ok(plan) => {
            let restored = plan.into_run_config(OutputConfig::default());
            assert_eq!(
                Observation::from_config(restored, id),
                fixture.saved,
                "{id}: plan"
            );
        }
        Err(error) => {
            assert!(
                matches!(fixture.saved, Observation::ContextTooLarge { .. }),
                "{id}: {error}"
            );
            assert!(
                error
                    .to_string()
                    .contains(&fixture.saved.saved_error(id).to_string()),
                "{id}: {error}"
            );
        }
    }
}

#[test]
fn changed_selection_configuration_matches_lean() {
    let base = parse_config_from(["hoimin", "run", "--file", "src/calc.py", "--", "python"])
        .unwrap()
        .with_environment(EnvironmentSnapshot::Untracked)
        .unwrap();
    let saved_plan = serde_json::to_value(
        base.clone()
            .into_plan_config(DiscoveryConfig::new(Duration::from_secs(30)).unwrap()),
    )
    .unwrap();
    let mut ids = BTreeSet::new();
    for line in include_str!("../../../../formal/HoiminOracle/corpus/changed-config.jsonl").lines()
    {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "strict", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        for (command, parse) in CONFIG_PARSERS {
            replay_cli(&fixture, command, parse);
        }
        let raw = RunConfig::try_from(raw_config(&fixture));
        assert_eq!(
            Observation::from_config(raw, &fixture.id),
            fixture.raw,
            "{}",
            fixture.id
        );
        replay_saved(&fixture, &base, &saved_plan);
    }
    assert_eq!(ids.len(), 386);
}
