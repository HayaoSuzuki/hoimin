//! Model correspondence against independent OS file descriptions, not a fake lock.
use super::{AcquireError, RunOwnership};
use crate::quint_itf as quint;
use proptest::prelude::*;
use rstest::rstest;
#[derive(Clone, Copy, Debug)]
enum Command {
    Acquire(usize, usize),
    Release(usize, usize),
    Close(usize),
}
struct Harness {
    held: [[Option<RunOwnership>; 3]; 4],
    // Handles drop before their containing directory (also needed on Windows).
    directory: tempfile::TempDir,
}
impl Harness {
    fn new() -> Self {
        Self {
            held: std::array::from_fn(|_| std::array::from_fn(|_| None)),
            directory: tempfile::tempdir().unwrap(),
        }
    }
    fn apply(&mut self, command: Command) -> &'static str {
        match command {
            Command::Acquire(actor, run) => {
                match RunOwnership::acquire(self.directory.path(), &format!("run-{run}")) {
                    Ok(lease) => {
                        assert!(
                            self.held[actor][run].is_none(),
                            "second open must contend even for same actor"
                        );
                        self.held[actor][run] = Some(lease);
                        "ok"
                    }
                    Err(AcquireError::Active) => "busy",
                    Err(AcquireError::Io(error)) => panic!("lock infrastructure failure: {error}"),
                }
            }
            Command::Release(actor, run) => {
                self.held[actor][run] = None;
                "released"
            }
            Command::Close(actor) => {
                self.held[actor] = std::array::from_fn(|_| None);
                "closed"
            }
        }
    }
    fn probe(&self, owners: &[Option<usize>; 3]) {
        for (run, owner) in owners.iter().enumerate() {
            match RunOwnership::acquire(self.directory.path(), &format!("run-{run}")) {
                Ok(lease) => {
                    assert!(
                        owner.is_none(),
                        "owned run {run} admitted a second descriptor"
                    );
                    drop(lease);
                }
                Err(AcquireError::Active) => {
                    assert!(owner.is_some(), "unowned run {run} leaked a lock");
                }
                Err(AcquireError::Io(error)) => panic!("lock probe infrastructure: {error}"),
            }
        }
    }
}
fn history(commands: &[Command]) -> Vec<&'static str> {
    let mut actual = Harness::new();
    let mut owners = [None; 3];
    let mut results = Vec::new();
    for command in commands {
        let expected = match *command {
            Command::Acquire(actor, run) => {
                if owners[run].is_none() {
                    owners[run] = Some(actor);
                    "ok"
                } else {
                    "busy"
                }
            }
            Command::Release(actor, run) => {
                if owners[run] == Some(actor) {
                    owners[run] = None;
                }
                "released"
            }
            Command::Close(actor) => {
                for owner in &mut owners {
                    if *owner == Some(actor) {
                        *owner = None;
                    }
                }
                "closed"
            }
        };
        let got = actual.apply(*command);
        assert_eq!(got, expected, "{command:?}");
        actual.probe(&owners);
        results.push(got);
    }
    results
}
fn replay(trace: &str) -> Result<(), String> {
    let states = quint::states(trace);
    assert!(states.len() > 1);
    let mut actual = Harness::new();
    for row in states.iter().skip(1) {
        let actor = usize::try_from(quint::number(&row["actor"])).unwrap();
        let run = usize::try_from(quint::number(&row["runId"])).unwrap();
        let command = match row["op"].as_str().unwrap() {
            "acquire" => Command::Acquire(actor, run),
            "release" => Command::Release(actor, run),
            "close" => Command::Close(actor),
            other => return Err(format!("unknown operation {other}")),
        };
        let got = actual.apply(command);
        if got != row["result"].as_str().unwrap() {
            return Err(format!("{command:?}: {got}, expected {}", row["result"]));
        }
        let mut owners = [None; 3];
        for lease in quint::set(&row["leases"]) {
            owners[usize::try_from(quint::number(&lease["runId"])).unwrap()] =
                Some(usize::try_from(quint::number(&lease["actor"])).unwrap());
        }
        actual.probe(&owners);
    }
    Ok(())
}
#[rstest]
#[case::same_actor_contends(&[Command::Acquire(0,0),Command::Acquire(0,0),Command::Release(0,0),Command::Acquire(0,0)],&["ok","busy","released","ok"])]
#[case::transfer(&[Command::Acquire(0,0),Command::Acquire(1,0),Command::Close(0),Command::Acquire(1,0)],&["ok","busy","closed","ok"])]
#[case::independent_runs(&[Command::Acquire(0,0),Command::Acquire(1,1),Command::Release(1,0),Command::Acquire(1,0),Command::Close(0),Command::Acquire(1,0)],&["ok","ok","released","busy","closed","ok"])]
fn literal_ownership_history(#[case] commands: &[Command], #[case] expected: &[&str]) {
    // Arrange.
    let operations = commands;
    // Act.
    let actual = history(operations);
    // Assert.
    assert_eq!(actual, expected);
}
fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        (0..4usize, 0..3usize).prop_map(|(a, r)| Command::Acquire(a, r)),
        (0..4usize, 0..3usize).prop_map(|(a, r)| Command::Release(a, r)),
        (0..4usize).prop_map(Command::Close)
    ]
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn generated_history_matches_os_lock_ownership(commands in prop::collection::vec(command(),0..64)) {
        // Arrange: four independently owned handle collections and three run IDs.
        // Act.
        let outcomes=history(&commands);
        // Assert.
        prop_assert_eq!(outcomes.len(),commands.len());
    }
}
#[rstest]
#[case::trace_0(include_str!("../../../../../formal/quint/traces/ownership_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../formal/quint/traces/ownership_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../formal/quint/traces/ownership_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../formal/quint/traces/ownership_3.itf.json"))]
#[case::reacquire(include_str!("../../../../../formal/quint/traces/ownership_reacquireStep_0.itf.json"))]
fn model_matches_actual_os_locks(#[case] trace: &str) {
    // Arrange.
    let oracle = trace;
    // Act.
    let outcome = replay(oracle);
    // Assert.
    assert_eq!(outcome, Ok(()));
}
