use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, TryLockError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use uuid::Uuid;

use super::lifecycle::{LinuxRunCgroup, RootCgroup, RootEntry, RootSignal, RunState};
use super::{CgroupEventCounters, ResourceError};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/cgroup-lifecycle.jsonl");
const NORMAL_COUNTERS: &[u8] = b"oom_kill 0\n";
const BAD_COUNTERS: &[u8] = b"oom_kill invalid\n";
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    initial: State,
    action: Action,
    first: Outcome,
    retry: Outcome,
    overlap: Option<Overlap>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Terminate {
        root: Root,
        fail: bool,
        account_fault: Option<Root>,
    },
    Close {
        faults: Faults,
        account_fault: Option<Root>,
    },
    Overlap {
        root_fails: bool,
        faults: Faults,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Overlap {
    before_release: State,
    root_attempt: Outcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Root {
    First,
    Second,
}

impl Root {
    fn index(self) -> usize {
        match self {
            Self::First => 0,
            Self::Second => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Target {
    First,
    Second,
    Run,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Status {
    Missing,
    Inactive,
    Active,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct State {
    first: Status,
    second: Status,
    closed: bool,
    cleaned: bool,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Faults {
    first: bool,
    second: bool,
    run: bool,
}

impl Faults {
    fn fails(self, target: Target) -> bool {
        match target {
            Target::First => self.first,
            Target::Second => self.second,
            Target::Run => self.run,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Call {
    target: Target,
    state: State,
    held_roots: Vec<Root>,
    close_held: bool,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Outcome {
    after: State,
    calls: Vec<Call>,
    cleanup_error: Option<Target>,
    accounting_error: bool,
}

struct Attempt {
    calls: Vec<Call>,
    result: Result<(), ResourceError>,
}

struct Fixture {
    run: LinuxRunCgroup,
    roots: [Arc<RootCgroup>; 2],
    _temp: tempfile::TempDir,
}

impl Fixture {
    fn new(initial: State) -> Self {
        assert!(!initial.closed && !initial.cleaned);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("run");
        fs::create_dir(&path).unwrap();
        let mut state = RunState::default();
        let roots = [(1, initial.first), (2, initial.second)].map(|(number, status)| {
            let id = Uuid::from_u128(number);
            let root = Arc::new(RootCgroup {
                id,
                path: path.join(number.to_string()),
                cleanup_gate: Mutex::new(()),
            });
            if status != Status::Missing {
                fs::create_dir(&root.path).unwrap();
                fs::write(root.path.join("memory.events"), NORMAL_COUNTERS).unwrap();
                fs::write(root.path.join("pids.events"), b"max 0\n").unwrap();
                state.roots.insert(
                    id,
                    RootEntry {
                        root: Arc::clone(&root),
                        signal: Arc::new(RootSignal::default()),
                        active: status == Status::Active,
                        counters: CgroupEventCounters::default(),
                    },
                );
            }
            root
        });
        Self {
            run: LinuxRunCgroup {
                path,
                #[cfg(target_os = "linux")]
                diagnostics: Vec::new(),
                state: Mutex::new(state),
                close_gate: Mutex::new(()),
            },
            roots,
            _temp: temp,
        }
    }

    fn target(&self, path: &Path) -> Target {
        if path == self.roots[0].path {
            Target::First
        } else if path == self.roots[1].path {
            Target::Second
        } else {
            assert_eq!(path, self.run.path);
            Target::Run
        }
    }

    fn counters(&self, root: Option<Root>, bytes: &[u8]) {
        if let Some(root) = root {
            let root = &self.roots[root.index()];
            match fs::write(root.path.join("memory.events"), bytes) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => panic!("could not write counter fixture: {error}"),
            }
        }
    }

    fn snapshot(&self) -> State {
        let state = self
            .run
            .state
            .try_lock()
            .expect("cleanup released the shared state mutex");
        let status = |root: Root| {
            state
                .roots
                .get(&self.roots[root.index()].id)
                .map_or(Status::Missing, |entry| {
                    if entry.active {
                        Status::Active
                    } else {
                        Status::Inactive
                    }
                })
        };
        State {
            first: status(Root::First),
            second: status(Root::Second),
            closed: state.closed,
            cleaned: state.cleaned,
        }
    }

    fn call(&self, path: &Path) -> Call {
        let held = |gate: &Mutex<()>| matches!(gate.try_lock(), Err(TryLockError::WouldBlock));
        Call {
            target: self.target(path),
            state: self.snapshot(),
            held_roots: [Root::First, Root::Second]
                .into_iter()
                .filter(|root| held(&self.roots[root.index()].cleanup_gate))
                .collect(),
            close_held: held(&self.run.close_gate),
        }
    }

    fn terminate(&self, root: Root, fail: bool, hold: impl FnOnce()) -> Attempt {
        let mut calls = Vec::new();
        let result =
            self.run
                .terminate_root_with(&self.roots[root.index()], Some(41), |path, pid| {
                    assert_eq!(pid, Some(41));
                    calls.push(self.call(path));
                    hold();
                    remove(path, fail)
                });
        Attempt { calls, result }
    }

    fn close(&self, faults: Faults) -> Attempt {
        let mut calls = Vec::new();
        let result = self.run.close_with(|path, pid| {
            assert_eq!(pid, None);
            calls.push(self.call(path));
            remove(path, faults.fails(self.target(path)))
        });
        Attempt { calls, result }
    }

    fn cleanup_error(&self, error: &ResourceError) -> Target {
        let ResourceError::CgroupCleanup { path, source } = error else {
            panic!("unexpected cleanup error: {error:?}");
        };
        assert_eq!(source.to_string(), "oracle cleanup fault");
        self.target(path)
    }

    fn errors(&self, result: &Result<(), ResourceError>) -> (bool, Option<Target>) {
        match result {
            Ok(()) => (false, None),
            Err(ResourceError::InvalidCgroupData(_)) => (true, None),
            Err(ResourceError::CgroupAccountingAndCleanup {
                accounting,
                cleanup,
            }) => {
                assert!(matches!(
                    accounting.as_ref(),
                    ResourceError::InvalidCgroupData(_)
                ));
                (true, Some(self.cleanup_error(cleanup)))
            }
            Err(cleanup) => (false, Some(self.cleanup_error(cleanup))),
        }
    }

    fn assert_attempt(&self, attempt: &Attempt, expected: &Outcome, label: &str) {
        let (accounting, cleanup) = self.errors(&attempt.result);
        assert_eq!(attempt.calls, expected.calls, "{label}: calls");
        assert_eq!(accounting, expected.accounting_error, "{label}: accounting");
        assert_eq!(cleanup, expected.cleanup_error, "{label}: cleanup");
    }

    fn assert_outcome(&self, attempt: &Attempt, expected: &Outcome, label: &str) {
        self.assert_attempt(attempt, expected, label);
        assert_eq!(self.snapshot(), expected.after, "{label}: state");
    }
}

fn remove(path: &Path, fail: bool) -> Result<(), ResourceError> {
    if !path.exists() {
        return Ok(());
    }
    if fail {
        return Err(ResourceError::CgroupCleanup {
            path: path.to_owned(),
            source: io::Error::other("oracle cleanup fault"),
        });
    }
    fs::remove_dir_all(path).unwrap();
    Ok(())
}

fn replay_overlap(fixture: &Fixture, case: &Case, root_fails: bool, faults: Faults) {
    let expected = case.overlap.as_ref().expect("overlap observations");
    thread::scope(|scope| {
        let (entered, within_root) = mpsc::channel();
        let (resume, held) = mpsc::channel();
        let root = scope.spawn(move || {
            fixture.terminate(Root::First, root_fails, || {
                entered.send(()).unwrap();
                held.recv_timeout(TIMEOUT).unwrap();
            })
        });
        within_root.recv_timeout(TIMEOUT).unwrap();
        let closing = scope.spawn(|| fixture.close(faults));
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if fixture.run.state.try_lock().is_ok_and(|state| state.closed) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{}: close did not publish while waiting",
                case.id
            );
            thread::yield_now();
        }
        assert_eq!(fixture.snapshot(), expected.before_release, "{}", case.id);
        assert!(matches!(
            fixture.run.close_gate.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        assert!(matches!(
            fixture.roots[0].cleanup_gate.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        assert!(fixture.roots[1].cleanup_gate.try_lock().is_ok());

        resume.send(()).unwrap();
        fixture.assert_attempt(&root.join().unwrap(), &expected.root_attempt, &case.id);
        fixture.assert_outcome(&closing.join().unwrap(), &case.first, &case.id);
    });
}

fn replay(case: &Case) {
    let fixture = Fixture::new(case.initial);
    match case.action {
        Action::Terminate {
            root,
            fail,
            account_fault,
        } => {
            fixture.counters(account_fault, BAD_COUNTERS);
            let attempt = fixture.terminate(root, fail, || {});
            fixture.assert_outcome(&attempt, &case.first, &case.id);
            fixture.counters(account_fault, NORMAL_COUNTERS);
            let retry = fixture.terminate(root, false, || {});
            fixture.assert_outcome(&retry, &case.retry, &case.id);
        }
        Action::Close {
            faults,
            account_fault,
        } => {
            fixture.counters(account_fault, BAD_COUNTERS);
            let attempt = fixture.close(faults);
            fixture.assert_outcome(&attempt, &case.first, &case.id);
            fixture.counters(account_fault, NORMAL_COUNTERS);
            let retry = fixture.close(Faults::default());
            fixture.assert_outcome(&retry, &case.retry, &case.id);
        }
        Action::Overlap { root_fails, faults } => {
            replay_overlap(&fixture, case, root_fails, faults);
            let retry = fixture.close(Faults::default());
            fixture.assert_outcome(&retry, &case.retry, &case.id);
        }
    }
    fixture.close(Faults::default()).result.unwrap();
}

#[test]
fn cgroup_lifecycle_matches_lean_oracle() {
    let mut ids = std::collections::BTreeSet::new();
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        replay(&case);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 356);
}
