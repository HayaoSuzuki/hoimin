use super::*;
use hoimin_core::{MutationOutcome, ResumeOutcome, RunMetrics};

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Work {
    KilledMutants,
    LimitedMutants,
    NoCandidates,
    FailedBaseline,
}

impl Work {
    const fn source(self) -> &'static str {
        match self {
            Self::KilledMutants | Self::FailedBaseline => "def total():\n    return 1 + 2 + 3\n",
            Self::LimitedMutants => "def total():\n    return 1 + 2 + 3 + 4\n",
            Self::NoCandidates => "def total():\n    return 6\n",
        }
    }

    const fn command(self) -> &'static str {
        match self {
            Self::KilledMutants | Self::NoCandidates => {
                "from src.calc import total; assert total() == 6"
            }
            Self::FailedBaseline => "raise SystemExit(1)",
            Self::LimitedMutants => "from src.calc import total; assert total() == 10",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Session {
    Ephemeral,
    Fresh,
    Resume,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Observation {
    exit_code: i32,
    complete: bool,
    stages: Vec<String>,
    discovered: u64,
    executed: u64,
    reported_mutants: usize,
    reused_mutants: usize,
    not_run_mutants: usize,
    resume: Option<ResumeOutcome>,
}

#[derive(serde::Deserialize)]
struct Pass {
    session: Session,
    expected: Observation,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    work: Work,
    passes: Vec<Pass>,
}

async fn run_pass(
    root: &Path,
    metrics: &Path,
    database: &Path,
    work: Work,
    session: Session,
) -> FixtureRun {
    let metrics = metrics.to_str().unwrap();
    let command = work.command();
    match session {
        Session::Ephemeral => {
            run_project_options(
                root,
                1,
                command,
                &[
                    "--operators",
                    "binary_add_sub",
                    "--max-mutants",
                    "2",
                    "--metrics",
                    metrics,
                ],
            )
            .await
        }
        Session::Fresh | Session::Resume => {
            run_project_with_session_options(
                root,
                database,
                session == Session::Resume,
                2,
                command,
                &[
                    "--operators",
                    "binary_add_sub",
                    "--jobs",
                    "1",
                    "--metrics",
                    metrics,
                ],
            )
            .await
        }
    }
}

fn observe(run: &FixtureRun, metrics: &Path, id: &str) -> (Observation, String) {
    let bytes = std::fs::read(metrics)
        .unwrap_or_else(|error| panic!("{id}: cannot read sidecar: {error}; {}", run.stderr));
    let metrics: RunMetrics = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("{id}: invalid sidecar: {error}; {}", run.stderr));
    metrics.validate().unwrap();
    assert_eq!(metrics.run_id, run.document["run"]["run_id"], "{id}");
    assert_eq!(run.document["summary"]["exit_code"], run.exit_code, "{id}");
    for warning in ["metrics.state", "metrics.write", "metrics.incomplete"] {
        assert!(!run.stderr.contains(warning), "{id}: {}", run.stderr);
    }
    let mutants = run.document["mutants"].as_array().unwrap();
    let (reused_mutants, not_run_mutants) = mutants.iter().fold(
        (0, 0),
        |(reused, not_run), event| match mutation_outcome::outcome(event) {
            MutationOutcome::Reused { .. } => (reused + 1, not_run),
            MutationOutcome::NotRun => (reused, not_run + 1),
            MutationOutcome::Executed { .. } => (reused, not_run),
        },
    );
    let observed = Observation {
        exit_code: run.exit_code,
        complete: run.document["summary"]["complete"].as_bool().unwrap(),
        stages: metrics.stages.into_iter().map(|stage| stage.name).collect(),
        discovered: metrics.discovered,
        executed: metrics.executed,
        reported_mutants: mutants.len(),
        reused_mutants,
        not_run_mutants,
        resume: serde_json::from_value(run.document["run"]["resume"].clone()).unwrap(),
    };
    (observed, metrics.run_id)
}

async fn replay(fixture: Fixture) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    let source = root.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(source.join("calc.py"), fixture.work.source()).unwrap();
    let metrics = directory.path().join("metrics.json");
    let database = directory.path().join("session.sqlite3");
    std::fs::write(&metrics, b"previous sidecar").unwrap();

    let mut previous_identity = None;
    for pass in fixture.passes {
        let context = format!("{} ({:?})", fixture.id, pass.session);
        let run = tokio::time::timeout(
            Duration::from_secs(20),
            run_pass(&root, &metrics, &database, fixture.work, pass.session),
        )
        .await
        .unwrap_or_else(|_| panic!("{context}: CLI timed out"));
        let (actual, identity) = observe(&run, &metrics, &context);
        assert_eq!(actual, pass.expected, "{context}: {}", run.stderr);
        if pass.session == Session::Resume {
            assert_eq!(
                previous_identity.as_deref() == Some(identity.as_str()),
                matches!(pass.expected.resume, Some(ResumeOutcome::Resumed)),
                "{context}: run ID reuse"
            );
        }
        previous_identity = Some(identity);
    }
}

#[tokio::test]
async fn stage_metrics_public_cli_matches_lean() {
    let mut ids = BTreeSet::new();
    for line in
        include_str!("../../../../formal/HoiminOracle/corpus/stage-metrics-cli.jsonl").lines()
    {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "strict", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        replay(fixture).await;
    }
    assert_eq!(ids.len(), 5);
}
