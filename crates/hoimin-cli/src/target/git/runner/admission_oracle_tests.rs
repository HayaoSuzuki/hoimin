use super::{GitError, GitRunner};
use crate::process::{ProcessCancellation, ProcessStartGate};
use hoimin_core::TargetError;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::Instant;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Event {
    StopRequest,
    StopLifecycle,
    Advance { ticks: u64 },
    TrySpawn,
}

#[derive(Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    SpawnFailure,
    Cancelled,
}

#[derive(Deserialize)]
struct Step {
    event: Event,
    outcome: Option<Outcome>,
}

#[derive(Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    deadline: Option<u64>,
    steps: Vec<Step>,
}

#[tokio::test(start_paused = true)]
async fn git_spawn_admission_matches_lean() {
    let corpus = include_str!("../../../../../../formal/HoiminOracle/corpus/git-admission.jsonl");
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing-git.exe");
    let missing_error = std::process::Command::new(&missing)
        .output()
        .unwrap_err()
        .to_string();
    let mut ids = BTreeSet::new();
    let mut attempts = 0;

    for row in corpus.lines() {
        let fixture: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);
        let gate = ProcessStartGate::new();
        let cancellation = ProcessCancellation::new();
        let runner = fixture.deadline.map_or_else(GitRunner::default, |ticks| {
            GitRunner::for_run(gate.clone(), Instant::now() + Duration::from_millis(ticks))
        });

        for (index, step) in fixture.steps.into_iter().enumerate() {
            let outcome = match step.event {
                Event::StopRequest => {
                    gate.cancel();
                    None
                }
                Event::StopLifecycle => {
                    cancellation.cancel();
                    None
                }
                Event::Advance { ticks } => {
                    tokio::time::advance(Duration::from_millis(ticks)).await;
                    None
                }
                Event::TrySpawn => {
                    attempts += 1;
                    let was_cancelled = cancellation.is_cancelled();
                    let result = runner
                        .run_command(Command::new(&missing), &cancellation)
                        .await;
                    assert_eq!(
                        cancellation.is_cancelled(),
                        was_cancelled,
                        "{} step {index}",
                        fixture.id
                    );
                    let observed = match result {
                        Err(GitError::Cancelled) => Outcome::Cancelled,
                        Err(GitError::Failed(TargetError::GitFailed(message))) => {
                            assert_eq!(message, missing_error, "{} step {index}", fixture.id);
                            Outcome::SpawnFailure
                        }
                        result => panic!("{} step {index}: {result:?}", fixture.id),
                    };
                    Some(observed)
                }
            };
            assert_eq!(outcome, step.outcome, "{} step {index}", fixture.id);
            assert_eq!(
                runner.state.active.load(Ordering::Acquire),
                0,
                "{} step {index}",
                fixture.id
            );
            runner.drain_for_shutdown(Duration::ZERO).await.unwrap();
        }
        assert!(ids.insert(fixture.id));
    }
    assert_eq!(ids.len(), 170);
    assert_eq!(attempts, 284);
}
