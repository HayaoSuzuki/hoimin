use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use hoimin_core::{
    BudgetError, BudgetKind, BudgetLedger, CleanupFinished, EffectId, PreflightCompleted,
    ReservationId, ReserveError, RunBudgets, WorkspaceBudgetError, release_workspace_copy,
    reserve_workspace_copy,
};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Limits {
    memory: u64,
    copy: u64,
    processes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: u64,
    kind: String,
    amount: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Observation {
    event: String,
    verdict: String,
    error_code: Option<String>,
    allocated: Option<u64>,
    active: Vec<Entry>,
    released: Vec<u64>,
    totals: Limits,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    limits: Limits,
    max_id: u64,
    schedule: Vec<String>,
    expected: Vec<Observation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaseClass {
    Match,
    Mismatch,
    InfrastructureError,
}

#[derive(Debug)]
struct CaseResult {
    id: String,
    mode: String,
    class: CaseClass,
    expected: Vec<Observation>,
    actual: Vec<Observation>,
    detail: Option<String>,
}

fn corpus_text() -> &'static str {
    include_str!("../../../formal/HoiminOracle/corpus/budget-cleanup.jsonl")
}

fn validate_case(case: &OracleCase) -> Result<(), String> {
    if case.schema != 1 {
        return Err(format!("unsupported schema {}", case.schema));
    }
    if !matches!(case.mode.as_str(), "report" | "strict") {
        return Err(format!("unknown mode {}", case.mode));
    }
    if case.schedule.is_empty() || case.expected.is_empty() {
        return Err(format!("case {} has no executable steps", case.id));
    }
    if case.schedule.len() != case.expected.len() {
        return Err(format!(
            "case {} has {} scheduled events but {} observations",
            case.id,
            case.schedule.len(),
            case.expected.len()
        ));
    }
    if case
        .expected
        .iter()
        .flat_map(|step| step.active.iter())
        .any(|entry| entry.id > case.max_id)
    {
        return Err(format!("case {} has an active id above max_id", case.id));
    }
    for (event, expected) in case.schedule.iter().zip(&case.expected) {
        let known = event
            .strip_prefix("reserve:")
            .and_then(|rest| rest.split_once(':'))
            .is_some_and(|(kind, amount)| {
                matches!(kind, "memory" | "copy" | "processes") && amount.parse::<u64>().is_ok()
            })
            || event.strip_prefix("release:").is_some_and(|ids| {
                ids.is_empty() || ids.split(',').all(|id| id.parse::<u64>().is_ok())
            });
        if !known {
            return Err(format!("unknown event {event}"));
        }
        if expected.event != *event {
            return Err(format!(
                "case {} observation event {} does not match schedule {event}",
                case.id, expected.event
            ));
        }
        if !matches!(expected.verdict.as_str(), "accepted" | "rejected") {
            return Err(format!("unknown verdict {}", expected.verdict));
        }
        for entry in &expected.active {
            if !matches!(entry.kind.as_str(), "memory" | "copy" | "processes") {
                return Err(format!("unknown kind {}", entry.kind));
            }
        }
    }
    Ok(())
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let case: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        validate_case(&case)?;
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate case id {}", case.id));
        }
        cases.push(case);
    }
    if cases.is_empty() {
        return Err("corpus contains no cases".to_owned());
    }
    Ok(cases)
}

fn load_corpus() -> Result<Vec<OracleCase>, String> {
    parse_corpus(corpus_text())
}

struct Driver {
    ledger: BudgetLedger,
    roles: BTreeMap<u64, ReservationId>,
    allocations: Vec<(u64, ReservationId)>,
    released: Vec<u64>,
    next_role: u64,
    next_effect: u64,
}

impl Driver {
    fn new(limits: &Limits) -> Self {
        Self {
            ledger: BudgetLedger::new(RunBudgets {
                memory: limits.memory,
                copy: limits.copy,
                processes: limits.processes,
            }),
            roles: BTreeMap::new(),
            allocations: Vec::new(),
            released: Vec::new(),
            next_role: 0,
            next_effect: 0,
        }
    }

    fn effect_id(&mut self) -> EffectId {
        let id = EffectId(self.next_effect);
        self.next_effect += 1;
        id
    }

    fn apply(&mut self, event: &str) -> Result<Observation, String> {
        if let Some(rest) = event.strip_prefix("reserve:") {
            let (kind_name, amount) = rest
                .split_once(':')
                .ok_or_else(|| format!("malformed reserve event {event}"))?;
            let amount = amount
                .parse::<u64>()
                .map_err(|error| format!("invalid reserve amount in {event}: {error}"))?;
            return self.reserve(event, kind_name, amount);
        }
        if let Some(rest) = event.strip_prefix("release:") {
            let ids = if rest.is_empty() {
                Vec::new()
            } else {
                rest.split(',')
                    .map(|id| {
                        id.parse::<u64>()
                            .map_err(|error| format!("invalid release id in {event}: {error}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            return Ok(self.release(event, &ids));
        }
        Err(format!("unknown event {event}"))
    }

    fn reserve(
        &mut self,
        event: &str,
        kind_name: &str,
        amount: u64,
    ) -> Result<Observation, String> {
        let kind = parse_kind(kind_name)?;
        let result = if kind == BudgetKind::Copy {
            let effect_id = self.effect_id();
            reserve_workspace_copy(
                &mut self.ledger,
                &PreflightCompleted {
                    id: effect_id,
                    per_worker_logical_bytes: amount,
                    requested_workers: 1,
                    aggregate_logical_bytes: amount,
                    fingerprint: None,
                },
            )
            .map(|grant| grant.reservation_id())
            .map_err(workspace_reserve_code)
        } else {
            self.ledger
                .reserve(kind, amount)
                .map_err(reserve_error_code)
        };
        match result {
            Ok(real_id) => {
                let role = self.next_role;
                self.next_role += 1;
                self.roles.insert(role, real_id);
                self.allocations.push((role, real_id));
                Ok(self.observe(event, "accepted", None, Some(role)))
            }
            Err(code) => Ok(self.observe(event, "rejected", Some(code), None)),
        }
    }

    fn release(&mut self, event: &str, ids: &[u64]) -> Observation {
        let real_ids = ids
            .iter()
            .map(|role| {
                self.roles
                    .get(role)
                    .copied()
                    .unwrap_or(ReservationId(u64::MAX - role))
            })
            .collect();
        let effect_id = self.effect_id();
        let cleanup = CleanupFinished {
            id: effect_id,
            released_reservations: real_ids,
        };
        match release_workspace_copy(&mut self.ledger, &cleanup) {
            Ok(()) => {
                let mut released = ids.to_vec();
                released.extend_from_slice(&self.released);
                self.released = released;
                self.observe(event, "accepted", None, None)
            }
            Err(error) => self.observe(event, "rejected", Some(budget_error_code(error)), None),
        }
    }

    fn observe(
        &self,
        event: &str,
        verdict: &str,
        error_code: Option<String>,
        allocated: Option<u64>,
    ) -> Observation {
        let active = self
            .allocations
            .iter()
            .rev()
            .filter_map(|(role, real_id)| {
                self.ledger.reservation(*real_id).map(|reservation| Entry {
                    id: *role,
                    kind: kind_name(reservation.kind).to_owned(),
                    amount: reservation.amount,
                })
            })
            .collect();
        Observation {
            event: event.to_owned(),
            verdict: verdict.to_owned(),
            error_code,
            allocated,
            active,
            released: self.released.clone(),
            totals: Limits {
                memory: self.ledger.reserved(BudgetKind::Memory),
                copy: self.ledger.reserved(BudgetKind::Copy),
                processes: self.ledger.reserved(BudgetKind::Processes),
            },
        }
    }
}

fn parse_kind(value: &str) -> Result<BudgetKind, String> {
    match value {
        "memory" => Ok(BudgetKind::Memory),
        "copy" => Ok(BudgetKind::Copy),
        "processes" => Ok(BudgetKind::Processes),
        other => Err(format!("unknown kind {other}")),
    }
}

fn kind_name(value: BudgetKind) -> &'static str {
    match value {
        BudgetKind::Memory => "memory",
        BudgetKind::Copy => "copy",
        BudgetKind::Processes => "processes",
    }
}

fn reserve_error_code(error: ReserveError) -> String {
    match error {
        ReserveError::LimitReached(_) => "budget.limit",
        ReserveError::ReservationIdsExhausted => "budget.reservation_id.exhausted",
    }
    .to_owned()
}

fn workspace_reserve_code(error: WorkspaceBudgetError) -> String {
    match error {
        WorkspaceBudgetError::Reserve(error) => reserve_error_code(error),
        other => format!("infrastructure.unexpected_workspace_error.{}", other.code()),
    }
}

fn budget_error_code(error: BudgetError) -> String {
    match error {
        BudgetError::AlreadyReleased(_) => "budget.reservation.already_released",
        BudgetError::UnknownReservation(_) => "budget.reservation.unknown",
    }
    .to_owned()
}

fn run_case(case: &OracleCase) -> CaseResult {
    let mut driver = Driver::new(&case.limits);
    let mut actual = Vec::new();
    for event in &case.schedule {
        match driver.apply(event) {
            Ok(observation) => actual.push(observation),
            Err(error) => {
                return CaseResult {
                    id: case.id.clone(),
                    mode: case.mode.clone(),
                    class: CaseClass::InfrastructureError,
                    expected: case.expected.clone(),
                    actual,
                    detail: Some(error),
                };
            }
        }
    }
    let class = if actual == case.expected {
        CaseClass::Match
    } else {
        CaseClass::Mismatch
    };
    let detail = (class == CaseClass::Mismatch).then(|| {
        format!(
            "model max_id={} differs from the public Rust allocator frontier; expected={:?}; actual={actual:?}",
            case.max_id, case.expected
        )
    });
    CaseResult {
        id: case.id.clone(),
        mode: case.mode.clone(),
        class,
        expected: case.expected.clone(),
        actual,
        detail,
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn isolate_case(case: &OracleCase, execute: impl FnOnce() -> CaseResult) -> CaseResult {
    match catch_unwind(AssertUnwindSafe(execute)) {
        Ok(result) => result,
        Err(payload) => CaseResult {
            id: case.id.clone(),
            mode: case.mode.clone(),
            class: CaseClass::InfrastructureError,
            expected: case.expected.clone(),
            actual: Vec::new(),
            detail: Some(format!(
                "case panicked: {}",
                panic_message(payload.as_ref())
            )),
        },
    }
}

fn run_case_isolated(case: &OracleCase) -> CaseResult {
    isolate_case(case, || run_case(case))
}

#[test]
fn corpus_is_well_formed() {
    let cases = load_corpus().expect("the committed Lean budget corpus must parse");
    assert_eq!(cases.len(), 12);
    assert!(cases.iter().all(|case| !case.expected.is_empty()));
}

#[test]
fn case_panics_are_infrastructure_errors() {
    let case = load_corpus().unwrap().into_iter().next().unwrap();
    let result = isolate_case(&case, || panic!("deliberate oracle panic"));

    assert_eq!(result.class, CaseClass::InfrastructureError);
    assert_eq!(
        result.detail.as_deref(),
        Some("case panicked: deliberate oracle panic")
    );
}

#[test]
fn corpus_rejects_an_unknown_schema() {
    let invalid = corpus_text().replacen("\"schema\":1", "\"schema\":2", 1);
    let error = parse_corpus(&invalid).unwrap_err();
    assert!(error.contains("unsupported schema 2"), "{error}");
}

#[test]
fn corpus_rejects_duplicate_case_ids() {
    let first = corpus_text().lines().next().unwrap();
    let invalid = format!("{first}\n{first}\n");
    let error = parse_corpus(&invalid).unwrap_err();
    assert!(error.contains("duplicate case id"), "{error}");
}

#[test]
fn corpus_rejects_unknown_events_and_kinds() {
    let invalid_event = corpus_text().replace("reserve:memory:1", "reserve:future:1");
    let error = parse_corpus(&invalid_event).unwrap_err();
    assert!(error.contains("unknown event reserve:future:1"), "{error}");

    let invalid_kind = corpus_text().replacen("\"kind\":\"memory\"", "\"kind\":\"future\"", 1);
    let error = parse_corpus(&invalid_kind).unwrap_err();
    assert!(error.contains("unknown kind future"), "{error}");
}

#[test]
fn oracle_correspondence() {
    let cases = load_corpus().unwrap();
    let selected = std::env::var("HOIMIN_BUDGET_ORACLE_CASE").ok();
    let results: Vec<_> = cases
        .iter()
        .filter(|case| selected.as_ref().is_none_or(|id| id == &case.id))
        .map(run_case_isolated)
        .collect();

    assert!(
        !results.is_empty(),
        "the case filter selected no corpus case"
    );
    for result in &results {
        println!(
            "case={} class={:?} expected={:?} actual={:?} detail={:?}",
            result.id, result.class, result.expected, result.actual, result.detail
        );
    }
    assert!(
        results
            .iter()
            .all(|result| result.class != CaseClass::InfrastructureError),
        "{results:#?}"
    );
    assert!(
        results
            .iter()
            .all(|result| result.mode != "strict" || result.class == CaseClass::Match),
        "strict Lean budget oracle mismatch: {results:#?}"
    );
}
