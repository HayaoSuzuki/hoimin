use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

use serde::Deserialize;

use super::{
    CleanupStrategy, ResourceError, begin_cgroup_member_cleanup, finish_cgroup_member_cleanup,
    run_cleanup_after_accounting,
};

const CORPUS: &str = include_str!("../../../../../formal/HoiminOracle/corpus/cgroup-cleanup.jsonl");
const LIVE_ROOT: i32 = 41;
const ACCOUNTING: &str = "oracle accounting";
const FINAL_ROUND: Round = Round {
    tree: Tree::Empty,
    members_ok: true,
    wait_ok: true,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    start: Start,
    accounting_failed: bool,
    rounds: Vec<Round>,
    expected: Observation,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent scripted response or ownership observation"
)]
struct Start {
    kernel_ok: bool,
    live_root: bool,
    group_ok: bool,
    members_ok: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct Round {
    tree: Tree,
    members_ok: bool,
    wait_ok: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Tree {
    Empty,
    Populated,
    Unreadable,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Observation {
    strategy: Option<Strategy>,
    calls: Vec<Call>,
    cleanup_error: Option<Failure>,
    accounting_error: bool,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Strategy {
    Kernel,
    LiveGroup,
    Membership,
}

impl From<CleanupStrategy> for Strategy {
    fn from(strategy: CleanupStrategy) -> Self {
        match strategy {
            CleanupStrategy::KernelRecursiveKill => Self::Kernel,
            CleanupStrategy::VerifiedLiveGroupThenMembershipPidLoop => Self::LiveGroup,
            CleanupStrategy::VerifiedMembershipPidLoop => Self::Membership,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Call {
    KernelKill,
    GroupSignal,
    MemberScan,
    InspectEmpty,
    InspectPopulated,
    InspectUnreadable,
    Wait,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Failure {
    GroupSignal,
    MemberScan,
    Inspect,
    Wait,
}

impl Failure {
    fn marker(self) -> &'static str {
        match self {
            Self::GroupSignal => "oracle group signal",
            Self::MemberScan => "oracle member scan",
            Self::Inspect => "oracle tree inspection",
            Self::Wait => "oracle wait",
        }
    }

    fn error(self) -> ResourceError {
        ResourceError::InvalidCgroupData(self.marker().to_owned())
    }

    fn require(self, succeeds: bool) -> Result<(), ResourceError> {
        if succeeds { Ok(()) } else { Err(self.error()) }
    }
}

fn cleanup_failure(error: &ResourceError) -> Failure {
    let ResourceError::InvalidCgroupData(marker) = error else {
        panic!("unexpected cleanup failure: {error:?}");
    };
    [
        Failure::GroupSignal,
        Failure::MemberScan,
        Failure::Inspect,
        Failure::Wait,
    ]
    .into_iter()
    .find(|failure| failure.marker() == marker)
    .unwrap_or_else(|| panic!("unexpected cleanup failure: {marker}"))
}

fn errors(result: &Result<(), ResourceError>) -> (bool, Option<Failure>) {
    match result {
        Ok(()) => (false, None),
        Err(ResourceError::CgroupAccountingAndCleanup {
            accounting,
            cleanup,
        }) => {
            assert!(
                matches!(accounting.as_ref(), ResourceError::InvalidCgroupData(marker) if marker == ACCOUNTING)
            );
            (true, Some(cleanup_failure(cleanup)))
        }
        Err(ResourceError::InvalidCgroupData(marker)) if marker == ACCOUNTING => (true, None),
        Err(cleanup) => (false, Some(cleanup_failure(cleanup))),
    }
}

fn begin(start: &Start, calls: &RefCell<Vec<Call>>) -> Result<CleanupStrategy, ResourceError> {
    begin_cgroup_member_cleanup(
        start.live_root.then_some(LIVE_ROOT),
        || {
            calls.borrow_mut().push(Call::KernelKill);
            start.kernel_ok
        },
        |pid| {
            assert_eq!(pid, LIVE_ROOT);
            calls.borrow_mut().push(Call::GroupSignal);
            Failure::GroupSignal.require(start.group_ok)
        },
        || {
            calls.borrow_mut().push(Call::MemberScan);
            Failure::MemberScan.require(start.members_ok)
        },
    )
}

fn finish(
    strategy: CleanupStrategy,
    rounds: &[Round],
    calls: &RefCell<Vec<Call>>,
) -> Result<(), ResourceError> {
    let mut input = rounds.iter().copied().chain(std::iter::once(FINAL_ROUND));
    let last = Cell::<Option<Round>>::new(None);
    finish_cgroup_member_cleanup(
        strategy,
        || {
            let round = last.get().expect("inspect the tree before scanning again");
            assert!(matches!(round.tree, Tree::Populated));
            calls.borrow_mut().push(Call::MemberScan);
            Failure::MemberScan.require(round.members_ok)
        },
        || {
            assert!(
                last.get().is_none(),
                "wait before inspecting an active tree again"
            );
            let round = input.next().expect("no more scripted tree observations");
            last.set(Some(round));
            let mut recorded = calls.borrow_mut();
            match round.tree {
                Tree::Empty => {
                    recorded.push(Call::InspectEmpty);
                    Ok(true)
                }
                Tree::Populated => {
                    recorded.push(Call::InspectPopulated);
                    Ok(false)
                }
                Tree::Unreadable => {
                    recorded.push(Call::InspectUnreadable);
                    Err(Failure::Inspect.error())
                }
            }
        },
        || {
            let round = last.take().expect("inspect the tree before waiting");
            assert!(matches!(round.tree, Tree::Populated));
            calls.borrow_mut().push(Call::Wait);
            Failure::Wait.require(round.wait_ok)
        },
    )
}

fn observe(case: &Case) -> Observation {
    let calls = RefCell::new(Vec::new());
    let accounting = case
        .accounting_failed
        .then(|| ResourceError::InvalidCgroupData(ACCOUNTING.to_owned()));
    let mut observed_strategy = None;
    let result = run_cleanup_after_accounting(accounting, || {
        let strategy = begin(&case.start, &calls)?;
        observed_strategy = Some(strategy.into());
        finish(strategy, &case.rounds, &calls)
    });
    let (accounting_error, cleanup_error) = errors(&result);
    Observation {
        strategy: observed_strategy,
        calls: calls.into_inner(),
        cleanup_error,
        accounting_error,
    }
}

#[test]
fn cgroup_cleanup_protocol_matches_lean_oracle() {
    let mut ids = BTreeSet::new();
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert_eq!(observe(&case), case.expected, "{}", case.id);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 384);
}
