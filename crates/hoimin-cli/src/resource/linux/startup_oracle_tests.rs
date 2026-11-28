use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, TryLockError};

use serde::Deserialize;
use uuid::Uuid;

use super::lifecycle::{LinuxRunCgroup, RootCgroup, RootEntry, RootSignal, RunState};
use super::{CgroupEventCounters, ResourceError};

const CORPUS: &str = include_str!("../../../../../formal/HoiminOracle/corpus/cgroup-startup.jsonl");
const PID: i32 = 41;
const MAX_PROCESSES: u32 = 7;
const ACCOUNTING_COUNTERS: &[u8] = b"oom_kill bad_accounting\n";
const INITIAL_COUNTERS: &[u8] = b"oom_kill bad_initial\n";
const COUNTERS: &[u8] = b"oom_kill 0\n";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    action: Action,
    after: State,
    calls: Vec<Phase>,
    failure: Option<Phase>,
    cleanup_calls: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Prepare {
        closed: bool,
        fault: Phase,
    },
    Attach {
        pid: Option<u32>,
        registered: bool,
        timing: Timing,
        fault: Phase,
    },
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Timing {
    OpenRun,
    AlreadyClosed,
    CloseDuringWait,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    None,
    Closed,
    Validate,
    Accounting,
    Create,
    Memory,
    Pids,
    Oom,
    InitialCounters,
    MissingPid,
    PidOverflow,
    Wait,
    WriteMembership,
    MissingRoot,
    Resume,
}

impl Phase {
    fn marker(self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::Create => "create",
            Self::Memory => "memory",
            Self::Pids => "pids",
            Self::Oom => "oom",
            Self::Wait => "wait",
            Self::WriteMembership => "membership",
            Self::Resume => "resume",
            _ => unreachable!("phase does not use a callback: {self:?}"),
        }
    }
}

const CALLBACKS: [Phase; 8] = [
    Phase::Validate,
    Phase::Create,
    Phase::Memory,
    Phase::Pids,
    Phase::Oom,
    Phase::Wait,
    Phase::WriteMembership,
    Phase::Resume,
];

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum FileStage {
    Missing,
    Directory,
    Memory,
    Pids,
    Oom,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Registration {
    Absent,
    Inactive,
    Active,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct State {
    files: FileStage,
    registration: Registration,
    member_written: bool,
    closed: bool,
    cleaned: bool,
}

struct Fixture {
    run: LinuxRunCgroup,
    target: PathBuf,
    accounting: PathBuf,
    calls: RefCell<Vec<Phase>>,
    _temp: tempfile::TempDir,
}

impl Fixture {
    fn new(closed: bool, fault: Phase) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("run");
        fs::create_dir(&path).unwrap();
        let mut state = RunState {
            closed,
            ..RunState::default()
        };
        let accounting = path.join("accounting");
        create_directory(
            &accounting,
            if fault == Phase::Accounting {
                ACCOUNTING_COUNTERS
            } else {
                COUNTERS
            },
        );
        let root = root_at(&accounting, 1);
        state.roots.insert(
            root.id,
            RootEntry {
                root,
                signal: Arc::new(RootSignal::default()),
                active: true,
                counters: CgroupEventCounters::default(),
            },
        );
        Self {
            target: path.join("target"),
            run: LinuxRunCgroup {
                path,
                #[cfg(target_os = "linux")]
                diagnostics: Vec::new(),
                state: Mutex::new(state),
                close_gate: Mutex::new(()),
            },
            accounting,
            calls: RefCell::new(Vec::new()),
            _temp: temp,
        }
    }

    fn perform(&self, phase: Phase, fail: Phase) -> Result<(), ResourceError> {
        if phase == Phase::Wait {
            assert!(
                self.run.state.try_lock().is_ok(),
                "wait precedes the state lock"
            );
        } else {
            assert!(matches!(
                self.run.state.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
        }
        self.calls.borrow_mut().push(phase);
        if phase == fail {
            return Err(ResourceError::io(
                "oracle root startup",
                io::Error::other(phase.marker()),
            ));
        }
        Ok(())
    }

    fn prepare(&self, fault: Phase) -> Result<(), ResourceError> {
        let (root, signal) = self.run.prepare_root_with(
            MAX_PROCESSES,
            || self.perform(Phase::Validate, fault),
            |parent| {
                assert_eq!(parent, self.run.path);
                self.perform(Phase::Create, fault)?;
                let raw = if fault == Phase::InitialCounters {
                    INITIAL_COUNTERS
                } else {
                    COUNTERS
                };
                create_directory(&self.target, raw);
                Ok(self.target.clone())
            },
            |path| {
                assert_eq!(path, self.target.join("memory.max"));
                self.perform(Phase::Memory, fault)?;
                fs::write(path, b"8192").unwrap();
                Ok(())
            },
            |path, value| {
                assert_eq!(path.parent(), Some(self.target.as_path()));
                let phase = if path.ends_with("pids.max") {
                    assert_eq!(value, MAX_PROCESSES.to_string().as_bytes());
                    Phase::Pids
                } else {
                    assert!(path.ends_with("memory.oom.group"));
                    assert_eq!(value, b"1");
                    Phase::Oom
                };
                self.perform(phase, fault)?;
                fs::write(path, value).unwrap();
                Ok(())
            },
        )?;
        let state = self.run.state.lock().unwrap();
        assert!(Arc::ptr_eq(&root, &state.roots[&root.id].root));
        assert!(Arc::ptr_eq(&signal, &state.roots[&root.id].signal));
        Ok(())
    }

    fn attach(
        &self,
        pid: Option<u32>,
        registered: bool,
        timing: Timing,
        fault: Phase,
    ) -> Result<(), ResourceError> {
        create_directory(&self.target, COUNTERS);
        fs::write(self.target.join("memory.max"), b"8192").unwrap();
        fs::write(self.target.join("pids.max"), MAX_PROCESSES.to_string()).unwrap();
        fs::write(self.target.join("memory.oom.group"), b"1").unwrap();
        let root = root_at(&self.target, 2);
        let signal = Arc::new(RootSignal::default());
        if registered {
            self.run.state.lock().unwrap().roots.insert(
                root.id,
                RootEntry {
                    root: Arc::clone(&root),
                    signal: Arc::clone(&signal),
                    active: false,
                    counters: CgroupEventCounters::default(),
                },
            );
        }
        let resumed = self.run.attach_root_with(
            &root,
            &signal,
            pid,
            |pid| {
                assert_eq!(pid, PID);
                self.perform(Phase::Wait, fault)?;
                if timing == Timing::CloseDuringWait {
                    self.cleanup();
                }
                Ok(())
            },
            |path, pid| {
                assert_eq!(path, self.target.join("cgroup.procs"));
                assert_eq!(pid, PID);
                self.perform(Phase::WriteMembership, fault)?;
                fs::write(path, pid.to_string()).unwrap();
                Ok(())
            },
            |pid| {
                assert_eq!(pid, PID);
                assert_eq!(
                    fs::read_to_string(self.target.join("cgroup.procs")).unwrap(),
                    pid.to_string()
                );
                self.perform(Phase::Resume, fault)
            },
        )?;
        assert_eq!(resumed, PID);
        Ok(())
    }

    fn cleanup(&self) -> Vec<String> {
        if self.accounting.exists() {
            fs::write(self.accounting.join("memory.events"), COUNTERS).unwrap();
        }
        let mut calls = Vec::new();
        self.run
            .close_with(|path, pid| {
                assert!(pid.is_none());
                if path == self.target {
                    calls.push("root".to_owned());
                } else if path == self.run.path {
                    calls.push("run".to_owned());
                } else {
                    assert_eq!(path, self.accounting);
                }
                fs::remove_dir_all(path).unwrap();
                Ok(())
            })
            .unwrap();
        assert!(
            !self.run.path.exists(),
            "recursive cleanup removes unregistered paths too"
        );
        calls
    }

    fn snapshot(&self) -> State {
        let written = ["memory.max", "pids.max", "memory.oom.group"]
            .map(|file| self.target.join(file).exists());
        let files = match (self.target.exists(), written) {
            (false, [false, false, false]) => FileStage::Missing,
            (true, [false, false, false]) => FileStage::Directory,
            (true, [true, false, false]) => FileStage::Memory,
            (true, [true, true, false]) => FileStage::Pids,
            (true, [true, true, true]) => FileStage::Oom,
            other => panic!("invalid file order: {other:?}"),
        };
        let state = self.run.state.lock().unwrap();
        let entry = state
            .roots
            .values()
            .find(|entry| entry.root.path == self.target);
        let registration = match entry {
            None => Registration::Absent,
            Some(entry) if entry.active => Registration::Active,
            Some(_) => Registration::Inactive,
        };
        State {
            files,
            registration,
            member_written: self.target.join("cgroup.procs").exists(),
            closed: state.closed,
            cleaned: state.cleaned,
        }
    }
}

fn create_directory(path: &Path, memory: &[u8]) {
    fs::create_dir(path).unwrap();
    fs::write(path.join("memory.events"), memory).unwrap();
    fs::write(path.join("pids.events"), b"max 0\n").unwrap();
}

fn root_at(path: &Path, id: u128) -> Arc<RootCgroup> {
    Arc::new(RootCgroup {
        id: Uuid::from_u128(id),
        path: path.to_owned(),
        cleanup_gate: Mutex::new(()),
    })
}

fn failure(result: Result<(), ResourceError>) -> Option<Phase> {
    Some(match result.err()? {
        ResourceError::RunClosed => Phase::Closed,
        ResourceError::MissingProcessId => Phase::MissingPid,
        ResourceError::InvalidCgroupData(message) => match message.as_str() {
            "child pid overflow" => Phase::PidOverflow,
            "prepared root cgroup is missing" => Phase::MissingRoot,
            "oom_kill bad_accounting" => Phase::Accounting,
            "oom_kill bad_initial" => Phase::InitialCounters,
            unexpected => panic!("unexpected cgroup data: {unexpected}"),
        },
        ResourceError::Io {
            operation: "oracle root startup",
            source,
        } => {
            let marker = source.to_string();
            CALLBACKS
                .into_iter()
                .find(|phase| phase.marker() == marker)
                .unwrap()
        }
        other => panic!("unexpected startup failure: {other:?}"),
    })
}

fn replay(case: &Case) {
    let (fixture, result) = match case.action {
        Action::Prepare { closed, fault } => {
            let fixture = Fixture::new(closed, fault);
            let result = fixture.prepare(fault);
            (fixture, result)
        }
        Action::Attach {
            pid,
            registered,
            timing,
            fault,
        } => {
            let fixture = Fixture::new(timing == Timing::AlreadyClosed, fault);
            let result = fixture.attach(pid, registered, timing, fault);
            (fixture, result)
        }
    };
    assert_eq!(failure(result), case.failure, "{}: failure", case.id);
    assert_eq!(*fixture.calls.borrow(), case.calls, "{}: calls", case.id);
    assert_eq!(fixture.snapshot(), case.after, "{}: state", case.id);
    assert_eq!(
        fixture.cleanup(),
        case.cleanup_calls,
        "{}: cleanup",
        case.id
    );
}

#[test]
fn cgroup_startup_matches_lean_oracle() {
    let mut ids = BTreeSet::new();
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        replay(&case);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 106);
}
