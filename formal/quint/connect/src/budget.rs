use hoimin_core::{BudgetError, BudgetKind, BudgetLedger, ReservationId, ReserveError, RunBudgets};
use proptest::prelude::*;
use quint_connect::runner::{self, RunConfig};
use quint_connect::{Driver, Result, State, Step, switch};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

const KINDS: [BudgetKind; 3] = [BudgetKind::Memory, BudgetKind::Copy, BudgetKind::Processes];
struct Bridge {
    ledger: BudgetLedger,
    issued: Vec<u64>,
    result: String,
    returned_id: Option<u64>,
    skip_release: bool,
}
impl Bridge {
    fn new(skip_release: bool) -> Self {
        Self {
            ledger: BudgetLedger::new(RunBudgets {
                memory: 2,
                copy: 2,
                processes: 2,
            }),
            issued: Vec::new(),
            result: "init".into(),
            returned_id: None,
            skip_release,
        }
    }
    fn reserve(&mut self, kind: u8, amount: u64) {
        self.returned_id = None;
        self.result = match self.ledger.reserve(KINDS[usize::from(kind)], amount) {
            Ok(id) => {
                self.issued.push(id.0);
                self.returned_id = Some(id.0);
                "ok"
            }
            Err(ReserveError::LimitReached(error)) => {
                assert_eq!(error.kind, KINDS[usize::from(kind)]);
                assert_eq!(error.requested, amount);
                assert_eq!(error.available, 2 - self.ledger.reserved(error.kind));
                "limit"
            }
            Err(other @ ReserveError::ReservationIdsExhausted) => {
                panic!("unexpected finite-workload error {other:?}")
            }
        }
        .into();
    }
    fn release(&mut self, id: u64) {
        self.returned_id = None;
        self.result = match self.ledger.release(ReservationId(id)) {
            Ok(()) => "ok",
            Err(BudgetError::AlreadyReleased(actual)) => {
                assert_eq!(actual.0, id);
                "released"
            }
            Err(BudgetError::UnknownReservation(actual)) => {
                assert_eq!(actual.0, id);
                "unknown"
            }
        }
        .into();
    }
}
#[derive(Debug, Deserialize, PartialEq)]
struct Snapshot {
    active: BTreeSet<(u64, u64, u64)>,
    released: BTreeSet<u64>,
    next_id: u64,
    totals: [u64; 3],
    result: String,
    returned_id: Option<u64>,
}
impl Driver for Bridge {
    type State = Snapshot;
    fn config() -> quint_connect::Config {
        quint_connect::Config {
            state: &[],
            nondet: &["dispatch"],
        }
    }
    fn step(&mut self, step: &Step) -> Result {
        switch!(step {
            Init => { *self=Self::new(self.skip_release); },
            Reserve(kind:u8,amount:u64) => { self.reserve(kind,amount); },
            Release(id:u64) => {
                if self.skip_release {self.skip_release=false;self.result="ok".into();self.returned_id=None;}
                else {self.release(id);}
            },
            Idle => {}
        })
    }
}
fn field<'a>(record: &'a Value, name: &str) -> &'a Value {
    let matches: Vec<_> = record
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| key.rsplit("::").next() == Some(name))
        .collect();
    assert_eq!(matches.len(), 1, "unambiguous model field {name}");
    matches[0].1
}
fn number(value: &Value) -> u64 {
    value
        .as_u64()
        .unwrap_or_else(|| value.as_str().unwrap().parse().unwrap())
}
fn set(value: &Value) -> &Vec<Value> {
    value.as_array().unwrap()
}
impl State<Bridge> for Snapshot {
    fn from_driver(driver: &Bridge) -> Result<Self> {
        let query_ids: BTreeSet<_> = driver.issued.iter().copied().chain(0..=4).collect();
        let active = query_ids
            .into_iter()
            .filter_map(|id| {
                driver.ledger.reservation(ReservationId(id)).map(|row| {
                    let kind = KINDS.iter().position(|kind| *kind == row.kind).unwrap();
                    (id, u64::try_from(kind).unwrap(), row.amount)
                })
            })
            .collect();
        Ok(Self {
            active,
            released: driver
                .issued
                .iter()
                .copied()
                .filter(|id| driver.ledger.reservation(ReservationId(*id)).is_none())
                .collect(),
            next_id: u64::try_from(driver.issued.len()).unwrap(),
            totals: KINDS.map(|kind| driver.ledger.reserved(kind)),
            result: driver.result.clone(),
            returned_id: driver.returned_id,
        })
    }
    fn from_spec(value: itf::Value) -> Result<Self> {
        let value = Value::deserialize(value)?;
        let active: BTreeSet<_> = set(field(&value, "active"))
            .iter()
            .map(|row| {
                (
                    number(&row["id"]),
                    number(&row["kind"]),
                    number(&row["amount"]),
                )
            })
            .collect();
        let totals = [0, 1, 2].map(|kind| {
            active
                .iter()
                .filter(|(_, k, _)| *k == kind)
                .map(|(_, _, amount)| amount)
                .sum()
        });
        let result = field(&value, "result").as_str().unwrap().to_owned();
        let returned_id = (field(&value, "op") == "reserve" && result == "ok")
            .then(|| number(field(&value, "id")));
        Ok(Self {
            active,
            released: set(field(&value, "released")).iter().map(number).collect(),
            next_id: number(field(&value, "nextId")),
            totals,
            result,
            returned_id,
        })
    }
}
fn run(seed: u32, schedule: &str, skip_release: bool) -> Result {
    runner::run_test(
        Bridge::new(skip_release),
        runner::Config {
            test_name: format!("budget seed={seed} schedule={schedule}"),
            gen_config: RunConfig {
                spec: format!("{}/../budget_connect.qnt", env!("CARGO_MANIFEST_DIR")),
                main: Some("BudgetConnect".into()),
                init: None,
                step: Some(schedule.into()),
                max_samples: Some(4),
                max_steps: Some(12),
                seed: seed.to_string(),
            },
        },
    )
}
#[rstest::rstest]
#[case::random("step")]
#[case::reuse("reuseStep")]
#[case::zero("zeroStep")]
fn live_budget_history_matches_public_ledger(#[case] schedule: &str) {
    // Arrange.
    let seed = 1172;
    // Act.
    let result = run(seed, schedule, false);
    // Assert.
    assert!(result.is_ok(), "{result:?}");
}
#[test]
fn live_budget_state_comparison_detects_missing_release() {
    // Arrange: report success without changing the actual ledger.
    let skip_release = true;
    // Act.
    let error = run(1172, "reuseStep", skip_release).unwrap_err();
    // Assert: compilation and generation failures cannot satisfy this control.
    assert!(
        error.to_string().contains("State invariant failed"),
        "{error:#}"
    );
}
proptest! {
    #![proptest_config(ProptestConfig {cases:8,max_shrink_iters:16,..ProptestConfig::default()})]
    #[test]
    fn generated_budget_seed_matches_real_reservations(seed in any::<u32>()) {
        // Arrange: four finite histories per seed.
        // Act.
        let result=run(seed,"step",false);
        // Assert.
        prop_assert!(result.is_ok(),"{result:?}");
    }
}
