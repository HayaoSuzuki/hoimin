use std::collections::{BTreeSet, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::session::SessionHandler;
use hoimin_core::{
    BeginSession, ByteSpan, EffectId, FinishSession, LoadSession, LookupStoredResult, MutantResult,
    MutationCandidate, MutationStatus, OutputSpoolRef, PersistResult, ResourceMode, RunFingerprint,
    SessionDiagnostic,
};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct RunObservation {
    run: String,
    fingerprint: String,
    finished: bool,
    complete: bool,
    ordinal: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ResultObservation {
    run: String,
    mutant: String,
    status: String,
    payload: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct OwnerObservation {
    run: String,
    handler: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Observation {
    event: String,
    verdict: String,
    error_code: Option<String>,
    selected_run: Option<String>,
    stored_result: Option<ResultObservation>,
    runs: Vec<RunObservation>,
    results: Vec<ResultObservation>,
    owners: Vec<OwnerObservation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
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
    class: CaseClass,
    expected: Vec<Observation>,
    actual: Vec<Observation>,
    detail: Option<String>,
}

fn corpus_text() -> &'static str {
    include_str!("../../../formal/HoiminOracle/corpus/session-recovery.jsonl")
}

fn known_role(value: &str, prefix: char) -> bool {
    matches!(
        (prefix, value),
        ('h', "h0" | "h1")
            | ('r', "r0" | "r1")
            | ('f', "f0" | "f1")
            | ('m', "m0" | "m1")
            | ('p', "p0" | "p1")
    )
}

fn known_event(event: &str) -> bool {
    let parts = event.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        ["open" | "drop", handler] => known_role(handler, 'h'),
        ["begin", handler, run, fingerprint] => {
            known_role(handler, 'h') && known_role(run, 'r') && known_role(fingerprint, 'f')
        }
        ["load", handler, fingerprint] => known_role(handler, 'h') && known_role(fingerprint, 'f'),
        ["lookup", handler, run, mutant] => {
            known_role(handler, 'h') && known_role(run, 'r') && known_role(mutant, 'm')
        }
        ["persist", handler, run, mutant, status, payload, diagnostic] => {
            known_role(handler, 'h')
                && known_role(run, 'r')
                && known_role(mutant, 'm')
                && matches!(
                    *status,
                    "killed"
                        | "survived"
                        | "timeout"
                        | "out_of_memory"
                        | "process_limit"
                        | "error"
                        | "not_run"
                )
                && known_role(payload, 'p')
                && matches!(*diagnostic, "valid" | "invalid_diagnostic")
        }
        ["finish", handler, run, complete] => {
            known_role(handler, 'h')
                && known_role(run, 'r')
                && matches!(*complete, "true" | "false")
        }
        _ => false,
    }
}

fn validate_case(case: &OracleCase) -> Result<(), String> {
    if case.schema != 1 {
        return Err(format!("unsupported schema {}", case.schema));
    }
    if case.mode != "strict" {
        return Err(format!("unknown mode {}", case.mode));
    }
    if case.schedule.is_empty() || case.schedule.len() != case.expected.len() {
        return Err(format!("case {} has invalid schedule length", case.id));
    }
    for (event, expected) in case.schedule.iter().zip(&case.expected) {
        if !known_event(event) {
            return Err(format!("unknown event {event}"));
        }
        if expected.event != *event {
            return Err(format!("case {} has mismatched observation event", case.id));
        }
        if !matches!(expected.verdict.as_str(), "accepted" | "rejected") {
            return Err(format!("unknown verdict {}", expected.verdict));
        }
        if expected.error_code.as_deref().is_some_and(|code| {
            !matches!(
                code,
                "session.commit"
                    | "session.duplicate_result"
                    | "session.finish.state"
                    | "session.lookup.complete"
                    | "session.lookup.state"
                    | "session.persist.complete"
                    | "session.persist.diagnostic"
                    | "session.resume.active"
            )
        }) {
            return Err(format!("case {} has an unknown error code", case.id));
        }
        let unknown_run = expected
            .selected_run
            .as_deref()
            .is_some_and(|run| !known_role(run, 'r'))
            || expected
                .runs
                .iter()
                .any(|run| !known_role(&run.run, 'r') || !known_role(&run.fingerprint, 'f'))
            || expected
                .owners
                .iter()
                .any(|owner| !known_role(&owner.run, 'r') || !known_role(&owner.handler, 'h'));
        if unknown_run {
            return Err(format!("case {} has an unknown state role", case.id));
        }
        if expected
            .results
            .iter()
            .chain(expected.stored_result.iter())
            .any(|result| {
                !known_role(&result.run, 'r')
                    || !known_role(&result.mutant, 'm')
                    || !known_role(&result.payload, 'p')
                    || !matches!(
                        result.status.as_str(),
                        "killed"
                            | "survived"
                            | "timeout"
                            | "out_of_memory"
                            | "process_limit"
                            | "error"
                            | "not_run"
                    )
            })
        {
            return Err(format!("case {} has an unknown result role", case.id));
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

fn run_id(role: &str) -> Result<&'static str, String> {
    match role {
        "r0" => Ok("oracle-r0"),
        "r1" => Ok("oracle-r1"),
        _ => Err(format!("unknown run role {role}")),
    }
}

fn run_role(value: &str) -> Result<String, String> {
    match value {
        "oracle-r0" => Ok("r0".to_owned()),
        "oracle-r1" => Ok("r1".to_owned()),
        _ => Err(format!("unknown real run id {value}")),
    }
}

fn mutant_id(role: &str) -> Result<&'static str, String> {
    match role {
        "m0" => Ok("oracle-m0"),
        "m1" => Ok("oracle-m1"),
        _ => Err(format!("unknown mutant role {role}")),
    }
}

fn mutant_role(value: &str) -> Result<String, String> {
    match value {
        "oracle-m0" => Ok("m0".to_owned()),
        "oracle-m1" => Ok("m1".to_owned()),
        _ => Err(format!("unknown real mutant id {value}")),
    }
}

fn fingerprint(role: &str) -> Result<RunFingerprint, String> {
    match role {
        "f0" => Ok(RunFingerprint::from_bytes([1; 32])),
        "f1" => Ok(RunFingerprint::from_bytes([2; 32])),
        _ => Err(format!("unknown fingerprint role {role}")),
    }
}

fn fingerprint_role(value: &[u8]) -> Result<String, String> {
    match value {
        value if value == [1; 32] => Ok("f0".to_owned()),
        value if value == [2; 32] => Ok("f1".to_owned()),
        _ => Err("unknown persisted fingerprint".to_owned()),
    }
}

fn status(value: &str) -> Result<MutationStatus, String> {
    match value {
        "killed" => Ok(MutationStatus::Killed),
        "survived" => Ok(MutationStatus::Survived),
        "timeout" => Ok(MutationStatus::Timeout),
        "out_of_memory" => Ok(MutationStatus::OutOfMemory),
        "process_limit" => Ok(MutationStatus::ProcessLimit),
        "error" => Ok(MutationStatus::Error),
        "not_run" => Ok(MutationStatus::NotRun),
        _ => Err(format!("unknown status {value}")),
    }
}

fn persist_request(
    id: u64,
    run: &str,
    mutant: &str,
    status_name: &str,
    payload: &str,
    diagnostic: &str,
) -> Result<PersistResult, String> {
    let real_mutant = mutant_id(mutant)?;
    let diagnostic_mutant = if diagnostic == "valid" {
        real_mutant
    } else {
        "oracle-invalid-mutant"
    };
    let payload_number = match payload {
        "p0" => 0,
        "p1" => 1,
        _ => return Err(format!("unknown payload role {payload}")),
    };
    Ok(PersistResult {
        id: EffectId(id),
        worker: 0,
        result: MutantResult {
            run_id: run_id(run)?.to_owned(),
            candidate: MutationCandidate {
                id: real_mutant.to_owned(),
                sequence: payload_number,
                path: "src/oracle.py".into(),
                span: ByteSpan {
                    start: payload_number,
                    length: 1,
                },
                original: "original".to_owned(),
                replacement: format!("replacement-{payload}"),
                operator: "oracle".to_owned(),
                line: 1,
                column: 0,
                symbol: None,
                file_hash: format!("hash-{payload}"),
            },
            status: status(status_name)?,
            termination: None,
            elapsed: Duration::from_millis(5),
            resource_mode: ResourceMode::Hard,
            output: Some(OutputSpoolRef {
                token: format!("output-{payload}"),
                retained: payload_number + 1,
                observed: payload_number + 2,
            }),
            diagnostics: vec![SessionDiagnostic {
                mutant_id: diagnostic_mutant.to_owned(),
                level: "warning".to_owned(),
                code: format!("diagnostic.{payload}"),
                message: format!("payload {payload}"),
            }],
        },
    })
}

struct Driver {
    _temp: tempfile::TempDir,
    path: PathBuf,
    handlers: HashMap<String, SessionHandler>,
    owners: HashMap<String, String>,
    next_effect: u64,
}

type EventOutcome = (
    &'static str,
    Option<String>,
    Option<String>,
    Option<(String, String)>,
);

#[derive(Clone, Copy)]
struct PersistEvent<'a> {
    handler: &'a str,
    run: &'a str,
    mutant: &'a str,
    status: &'a str,
    payload: &'a str,
    diagnostic: &'a str,
}

impl Driver {
    fn new() -> Result<Self, String> {
        let temp = tempfile::tempdir().map_err(|error| format!("create temp dir: {error}"))?;
        let path = temp.path().join("session.sqlite3");
        Ok(Self {
            _temp: temp,
            path,
            handlers: HashMap::new(),
            owners: HashMap::new(),
            next_effect: 0,
        })
    }

    fn effect_id(&mut self) -> u64 {
        let id = self.next_effect;
        self.next_effect += 1;
        id
    }

    fn handler(&mut self, role: &str) -> Result<&mut SessionHandler, String> {
        self.handlers
            .get_mut(role)
            .ok_or_else(|| format!("event references unopened handler {role}"))
    }

    fn apply(&mut self, event: &str) -> Result<Observation, String> {
        let parts = event.split(':').collect::<Vec<_>>();
        let id = self.effect_id();
        let (verdict, error_code, selected_run, return_stored) =
            self.dispatch(&parts, id, event)?;
        let (runs, results) = observe_database(&self.path)?;
        let stored_result = return_stored.and_then(|(run, mutant)| {
            results
                .iter()
                .find(|result| result.run == run && result.mutant == mutant)
                .cloned()
        });
        let mut owners = self
            .owners
            .iter()
            .map(|(run, handler)| OwnerObservation {
                run: run.clone(),
                handler: handler.clone(),
            })
            .collect::<Vec<_>>();
        owners.sort_by(|left, right| (&left.run, &left.handler).cmp(&(&right.run, &right.handler)));
        Ok(Observation {
            event: event.to_owned(),
            verdict: verdict.to_owned(),
            error_code,
            selected_run,
            stored_result,
            runs,
            results,
            owners,
        })
    }

    fn dispatch(&mut self, parts: &[&str], id: u64, event: &str) -> Result<EventOutcome, String> {
        match parts {
            ["open", handler] => self.open_handler(handler),
            ["drop", handler] => self.drop_handler(handler),
            ["begin", handler, run, fingerprint_name] => {
                self.begin(handler, run, fingerprint_name, id)
            }
            ["load", handler, fingerprint_name] => self.load(handler, fingerprint_name, id),
            ["lookup", handler, run, mutant] => self.lookup(handler, run, mutant, id),
            [
                "persist",
                handler,
                run,
                mutant,
                status_name,
                payload,
                diagnostic,
            ] => self.persist(
                PersistEvent {
                    handler,
                    run,
                    mutant,
                    status: status_name,
                    payload,
                    diagnostic,
                },
                id,
            ),
            ["finish", handler, run, complete] => self.finish(handler, run, complete, id),
            _ => Err(format!("unknown event {event}")),
        }
    }

    fn open_handler(&mut self, handler: &str) -> Result<EventOutcome, String> {
        if self.handlers.remove(handler).is_some() {
            self.owners.retain(|_, owner| owner != handler);
        }
        let opened = SessionHandler::open(&self.path)
            .map_err(|error| format!("open handler {handler}: {error}"))?;
        self.handlers.insert(handler.to_owned(), opened);
        Ok(("accepted", None, None, None))
    }

    fn drop_handler(&mut self, handler: &str) -> Result<EventOutcome, String> {
        self.handler(handler)?;
        self.handlers.remove(handler);
        self.owners.retain(|_, owner| owner != handler);
        Ok(("accepted", None, None, None))
    }

    fn begin(
        &mut self,
        handler: &str,
        run: &str,
        fingerprint_name: &str,
        id: u64,
    ) -> Result<EventOutcome, String> {
        let request = BeginSession {
            id: EffectId(id),
            run_id: run_id(run)?.to_owned(),
            fingerprint: fingerprint(fingerprint_name)?,
        };
        match self.handler(handler)?.begin(request) {
            Ok(value) => {
                self.owners.insert(run.to_owned(), handler.to_owned());
                Ok(("accepted", None, Some(run_role(&value.run_id)?), None))
            }
            Err(error) => Ok((
                "rejected",
                Some(error.failure.code().to_owned()),
                None,
                None,
            )),
        }
    }

    fn load(
        &mut self,
        handler: &str,
        fingerprint_name: &str,
        id: u64,
    ) -> Result<EventOutcome, String> {
        let request = LoadSession {
            id: EffectId(id),
            fingerprint: fingerprint(fingerprint_name)?,
        };
        match self.handler(handler)?.load(&request) {
            Ok(value) => {
                let selected = value
                    .resume
                    .map(|resume| run_role(&resume.run_id))
                    .transpose()?;
                if let Some(run) = &selected {
                    self.owners.insert(run.clone(), handler.to_owned());
                }
                Ok(("accepted", None, selected, None))
            }
            Err(error) => Ok((
                "rejected",
                Some(error.failure.code().to_owned()),
                None,
                None,
            )),
        }
    }

    fn lookup(
        &mut self,
        handler: &str,
        run: &str,
        mutant: &str,
        id: u64,
    ) -> Result<EventOutcome, String> {
        let request = LookupStoredResult {
            id: EffectId(id),
            worker: 0,
            run_id: run_id(run)?.to_owned(),
            mutant_id: mutant_id(mutant)?.to_owned(),
        };
        match self.handler(handler)?.lookup(&request) {
            Ok(value) => Ok((
                "accepted",
                None,
                None,
                value.result.map(|_| (run.to_owned(), mutant.to_owned())),
            )),
            Err(error) => Ok((
                "rejected",
                Some(error.failure.code().to_owned()),
                None,
                None,
            )),
        }
    }

    fn persist(&mut self, event: PersistEvent<'_>, id: u64) -> Result<EventOutcome, String> {
        let request = persist_request(
            id,
            event.run,
            event.mutant,
            event.status,
            event.payload,
            event.diagnostic,
        )?;
        match self.handler(event.handler)?.persist(&request) {
            Ok(_) => Ok((
                "accepted",
                None,
                None,
                Some((event.run.to_owned(), event.mutant.to_owned())),
            )),
            Err(error) => Ok((
                "rejected",
                Some(error.failure.code().to_owned()),
                None,
                None,
            )),
        }
    }

    fn finish(
        &mut self,
        handler: &str,
        run: &str,
        complete: &str,
        id: u64,
    ) -> Result<EventOutcome, String> {
        let request = FinishSession {
            id: EffectId(id),
            run_id: run_id(run)?.to_owned(),
            complete: complete == "true",
        };
        match self.handler(handler)?.finish(request) {
            Ok(value) => {
                if self.owners.get(run).is_some_and(|owner| owner == handler) {
                    self.owners.remove(run);
                }
                Ok(("accepted", None, Some(run_role(&value.run_id)?), None))
            }
            Err(error) => Ok((
                "rejected",
                Some(error.failure.code().to_owned()),
                None,
                None,
            )),
        }
    }
}

fn observe_database(path: &Path) -> Result<(Vec<RunObservation>, Vec<ResultObservation>), String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("open observation connection: {error}"))?;
    let mut run_statement = connection
        .prepare("SELECT id, run_id, fingerprint, finished, complete FROM runs ORDER BY id")
        .map_err(|error| format!("prepare run observation: {error}"))?;
    let runs = run_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })
        .map_err(|error| format!("query run observation: {error}"))?
        .map(|row| {
            let (ordinal, run, fingerprint, finished, complete) =
                row.map_err(|error| format!("read run observation: {error}"))?;
            Ok(RunObservation {
                run: run_role(&run)?,
                fingerprint: fingerprint_role(&fingerprint)?,
                finished,
                complete,
                ordinal: u64::try_from(ordinal - 1)
                    .map_err(|error| format!("invalid run ordinal: {error}"))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    drop(run_statement);

    let mut result_statement = connection
        .prepare(
            "SELECT r.run_id, r.mutant_id, r.status, c.replacement
             FROM results r JOIN candidates c USING(run_id, mutant_id)
             ORDER BY r.run_id, r.mutant_id",
        )
        .map_err(|error| format!("prepare result observation: {error}"))?;
    let results = result_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| format!("query result observation: {error}"))?
        .map(|row| {
            let (run, mutant, status, replacement) =
                row.map_err(|error| format!("read result observation: {error}"))?;
            let payload = replacement
                .strip_prefix("replacement-")
                .ok_or_else(|| format!("unknown replacement payload {replacement}"))?;
            if !known_role(payload, 'p') {
                return Err(format!("unknown replacement payload {replacement}"));
            }
            Ok(ResultObservation {
                run: run_role(&run)?,
                mutant: mutant_role(&mutant)?,
                status,
                payload: payload.to_owned(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((runs, results))
}

fn execute_case(case: &OracleCase) -> Result<Vec<Observation>, String> {
    let mut driver = Driver::new()?;
    case.schedule
        .iter()
        .map(|event| driver.apply(event))
        .collect()
}

fn classify_case(
    case: &OracleCase,
    execute: impl FnOnce() -> Result<Vec<Observation>, String>,
) -> CaseResult {
    match catch_unwind(AssertUnwindSafe(execute)) {
        Ok(Ok(actual)) => CaseResult {
            id: case.id.clone(),
            class: if actual == case.expected {
                CaseClass::Match
            } else {
                CaseClass::Mismatch
            },
            expected: case.expected.clone(),
            actual,
            detail: None,
        },
        Ok(Err(detail)) => CaseResult {
            id: case.id.clone(),
            class: CaseClass::InfrastructureError,
            expected: case.expected.clone(),
            actual: Vec::new(),
            detail: Some(detail),
        },
        Err(_) => CaseResult {
            id: case.id.clone(),
            class: CaseClass::InfrastructureError,
            expected: case.expected.clone(),
            actual: Vec::new(),
            detail: Some("adapter panicked".to_owned()),
        },
    }
}

fn run_case(case: &OracleCase) -> CaseResult {
    classify_case(case, || execute_case(case))
}

fn mismatch_detail(result: &CaseResult) -> String {
    let difference = result
        .expected
        .iter()
        .zip(&result.actual)
        .enumerate()
        .find(|(_, (expected, actual))| expected != actual);
    match difference {
        Some((index, (expected, actual))) => {
            format!("step {index}\nexpected: {expected:#?}\nactual: {actual:#?}")
        }
        None => format!(
            "observation count differs: expected {}, actual {}",
            result.expected.len(),
            result.actual.len()
        ),
    }
}

#[test]
fn corpus_is_well_formed() {
    assert_eq!(parse_corpus(corpus_text()).unwrap().len(), 18);
}

#[test]
fn corpus_rejects_unknown_schema_mode_and_event() {
    let first = corpus_text().lines().next().unwrap();
    let schema = first.replacen("\"schema\":1", "\"schema\":2", 1);
    assert!(
        parse_corpus(&schema)
            .unwrap_err()
            .contains("unsupported schema")
    );
    for mode in [
        "model-only",
        "internal-fixture",
        "infrastructure-error",
        "report",
    ] {
        let text = first.replacen("\"mode\":\"strict\"", &format!("\"mode\":\"{mode}\""), 1);
        assert!(parse_corpus(&text).unwrap_err().contains("unknown mode"));
    }
    let event = first.replacen("open:h0", "crash:h0", 2);
    assert!(parse_corpus(&event).unwrap_err().contains("unknown event"));
    let status = corpus_text()
        .lines()
        .find(|line| line.contains("\"status\":"))
        .unwrap();
    let status = status.replacen("\"status\":\"killed\"", "\"status\":\"unknown\"", 1);
    assert!(
        parse_corpus(&status)
            .unwrap_err()
            .contains("unknown result role")
    );
    let short = first.replacen("\"schedule\":[", "\"schedule\":[],\"ignored\":[", 1);
    assert!(parse_corpus(&short).is_err());
}

#[test]
fn corpus_rejects_duplicate_case_ids() {
    let first = corpus_text().lines().next().unwrap();
    assert!(
        parse_corpus(&format!("{first}\n{first}"))
            .unwrap_err()
            .contains("duplicate case id")
    );
}

#[test]
fn case_panics_are_infrastructure_errors() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let result = classify_case(&case, || panic!("fixture panic"));
    assert_eq!(result.class, CaseClass::InfrastructureError);
    assert_eq!(result.detail.as_deref(), Some("adapter panicked"));
}

#[test]
fn oracle_correspondence() {
    const KNOWN_MISMATCH: &str = "non_owner_incomplete_finish_releases_for_resume";

    let cases = parse_corpus(corpus_text()).unwrap();
    let selected = std::env::var("HOIMIN_SESSION_ORACLE_CASE").ok();
    let selected_cases = cases
        .iter()
        .filter(|case| selected.as_ref().is_none_or(|id| id == &case.id))
        .collect::<Vec<_>>();
    if let Some(id) = &selected {
        assert!(!selected_cases.is_empty(), "unknown oracle case {id}");
    }
    let results = selected_cases.into_iter().map(run_case).collect::<Vec<_>>();
    let failures = results
        .iter()
        .filter(|result| {
            let expected_class = if result.id == KNOWN_MISMATCH {
                CaseClass::Mismatch
            } else {
                CaseClass::Match
            };
            result.class != expected_class
        })
        .map(|result| {
            let detail = result
                .detail
                .clone()
                .unwrap_or_else(|| mismatch_detail(result));
            format!("{}: {:?}: {detail}", result.id, result.class)
        })
        .collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    if selected.as_ref().is_none_or(|id| id == KNOWN_MISMATCH) {
        let counterexample = results
            .iter()
            .find(|result| result.id == KNOWN_MISMATCH)
            .expect("known strict counterexample must remain in the corpus");
        assert_eq!(counterexample.class, CaseClass::Mismatch);
    }
}
