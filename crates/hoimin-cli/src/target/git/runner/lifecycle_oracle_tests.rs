use super::{ActiveLifecycle, GitChild, GitRunner, terminate_and_reap};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::io;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Child {
    A,
    B,
}

impl Child {
    fn name(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

enum WaitResult {
    Failed(u32),
    Reaped,
}

struct FixtureChild {
    name: &'static str,
    kill_failed: bool,
    kill_called: bool,
    entered_wait: mpsc::UnboundedSender<()>,
    results: mpsc::UnboundedReceiver<WaitResult>,
}

impl GitChild for FixtureChild {
    fn start_kill(&mut self) -> io::Result<()> {
        assert!(
            !self.kill_called,
            "{}: repeated termination request",
            self.name
        );
        self.kill_called = true;
        if self.kill_failed {
            Err(io::Error::other(format!("{} kill", self.name)))
        } else {
            Ok(())
        }
    }

    async fn reap(&mut self) -> io::Result<()> {
        assert!(self.kill_called, "{}: reap preceded termination", self.name);
        self.entered_wait.send(()).unwrap();
        match self.results.recv().await.unwrap() {
            WaitResult::Failed(code) => Err(io::Error::other(format!("{} wait {code}", self.name))),
            WaitResult::Reaped => Ok(()),
        }
    }
}

struct Running {
    results: mpsc::UnboundedSender<WaitResult>,
    entered_wait: mpsc::UnboundedReceiver<()>,
    task: JoinHandle<Result<(), String>>,
}

impl Running {
    fn start(runner: &GitRunner, name: &'static str, primary: bool, kill_failed: bool) -> Self {
        let (results, receive_results) = mpsc::unbounded_channel();
        let (entered, entered_wait) = mpsc::unbounded_channel();
        runner.state.active.fetch_add(1, Ordering::AcqRel);
        let mut lifecycle = ActiveLifecycle {
            state: Arc::clone(&runner.state),
            settled: false,
        };
        let task = tokio::spawn(async move {
            let mut child = FixtureChild {
                name,
                kill_failed,
                kill_called: false,
                entered_wait: entered,
                results: receive_results,
            };
            let primary = primary.then(|| format!("{name} capture"));
            terminate_and_reap(&mut child, &mut lifecycle, primary.as_deref()).await
        });
        Self {
            results,
            entered_wait,
            task,
        }
    }

    async fn at_wait(&mut self) {
        tokio::time::timeout(Duration::from_secs(1), self.entered_wait.recv())
            .await
            .expect("Git cleanup did not attempt its next wait")
            .expect("Git cleanup finished before it reaped the child");
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Event {
    Start {
        child: Child,
        primary: bool,
        kill_failed: bool,
    },
    FailedWait {
        child: Child,
        code: u32,
    },
    Reaped {
        child: Child,
    },
    Abandoned {
        child: Child,
    },
}

#[derive(Deserialize)]
struct Step {
    event: Event,
    active: usize,
    drain: Option<String>,
    cleanup: Option<String>,
}

#[derive(Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    steps: Vec<Step>,
}

async fn run_event(
    runner: &GitRunner,
    owners: &mut [Option<Running>; 2],
    event: Event,
) -> Option<String> {
    match event {
        Event::Start {
            child,
            primary,
            kill_failed,
        } => {
            let slot = &mut owners[child.index()];
            assert!(slot.is_none());
            let mut owner = Running::start(runner, child.name(), primary, kill_failed);
            owner.at_wait().await;
            *slot = Some(owner);
        }
        Event::FailedWait { child, code } => {
            let owner = owners[child.index()].as_mut().unwrap();
            owner.results.send(WaitResult::Failed(code)).unwrap();
            owner.at_wait().await;
        }
        Event::Reaped { child } => {
            let owner = owners[child.index()].take().unwrap();
            owner.results.send(WaitResult::Reaped).unwrap();
            return tokio::time::timeout(Duration::from_secs(1), owner.task)
                .await
                .expect("Git cleanup did not finish after reap")
                .unwrap()
                .err();
        }
        Event::Abandoned { child } => {
            let owner = owners[child.index()].take().unwrap();
            owner.task.abort();
            assert!(owner.task.await.unwrap_err().is_cancelled());
        }
    }
    None
}

#[tokio::test(start_paused = true)]
async fn git_cleanup_lifecycles_match_lean() {
    let corpus = include_str!("../../../../../../formal/HoiminOracle/corpus/git-lifecycle.jsonl");
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let fixture: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);
        let runner = GitRunner::default();
        let mut owners = [None, None];
        for (index, step) in fixture.steps.into_iter().enumerate() {
            let cleanup = run_event(&runner, &mut owners, step.event).await;
            assert_eq!(cleanup, step.cleanup, "{} step {index}", fixture.id);
            let active = runner.state.active.load(Ordering::Acquire);
            assert_eq!(active, step.active, "{} step {index}", fixture.id);
            let observed = runner.drain_for_shutdown(Duration::ZERO).await;
            assert_eq!(observed.err(), step.drain, "{} step {index}", fixture.id);
            assert_eq!(
                runner.state.active.load(Ordering::Acquire),
                active,
                "{} drain changed ownership at step {index}",
                fixture.id
            );
        }
        assert!(owners.iter().all(Option::is_none), "{}", fixture.id);
        assert!(ids.insert(fixture.id));
    }
    assert_eq!(ids.len(), 313);
}
