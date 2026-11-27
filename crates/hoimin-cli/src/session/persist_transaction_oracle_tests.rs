use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use hoimin_core::{MutantResult, MutationOutcome, ProcessTermination, ResourceMode};
use serde::Deserialize;

use super::{
    Connection, EffectFailure, EffectId, PersistResult, SessionDiagnostic, SessionHandler,
    coverage_tests::{begin_request, executed_outcome, persist_request, reject_commit},
    params,
};

const CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/persist-transaction.jsonl");
const RUN: &str = "persist-oracle";
const SELECTED: &str = "selected";
const NEIGHBOR: &str = "neighbor";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    admission: Admission,
    stored: Image,
    fault: String,
    first: Observation,
    retry: Observation,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Admission {
    OpenRun,
    CompletedRun,
    NoOwner,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Image {
    Empty,
    Inconclusive,
    Determinate,
    First,
    Retry,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    outcome: String,
    before_commit: Option<Image>,
    after: Image,
    committed_writes: Vec<String>,
}

struct Write {
    name: &'static str,
    event: &'static str,
    table: &'static str,
    diagnostic: Option<&'static str>,
}

const WRITES: &[Write] = &[
    Write {
        name: "delete_diagnostics",
        event: "DELETE",
        table: "diagnostics",
        // One marker for the statement that deletes both saved diagnostics.
        diagnostic: Some("first"),
    },
    Write {
        name: "delete_result",
        event: "DELETE",
        table: "results",
        diagnostic: None,
    },
    Write {
        name: "delete_candidate",
        event: "DELETE",
        table: "candidates",
        diagnostic: None,
    },
    Write {
        name: "insert_candidate",
        event: "INSERT",
        table: "candidates",
        diagnostic: None,
    },
    Write {
        name: "insert_result",
        event: "INSERT",
        table: "results",
        diagnostic: None,
    },
    Write {
        name: "insert_first_diagnostic",
        event: "INSERT",
        table: "diagnostics",
        diagnostic: Some("first"),
    },
    Write {
        name: "insert_second_diagnostic",
        event: "INSERT",
        table: "diagnostics",
        diagnostic: Some("second"),
    },
];

impl Write {
    fn where_row(&self) -> String {
        let row = if self.event == "DELETE" { "OLD" } else { "NEW" };
        let mut filter = format!("{row}.run_id='{RUN}' AND {row}.mutant_id='{SELECTED}'");
        if let Some(code) = self.diagnostic {
            use std::fmt::Write as _;
            write!(filter, " AND {row}.code='{code}'").unwrap();
        }
        filter
    }

    fn error_stage(&self) -> (&'static str, &'static str) {
        match (self.event, self.table) {
            ("DELETE", _) => ("session.persist", "replace inconclusive result"),
            (_, "candidates") => ("session.duplicate_result", "insert candidate"),
            (_, "results") => ("session.persist", "insert result"),
            (_, "diagnostics") => ("session.persist", "insert diagnostic"),
            _ => unreachable!("{} {}", self.event, self.table),
        }
    }
}

fn payload(image: Image, mutant: &str) -> Option<PersistResult> {
    let (name, sequence, outcome) = match image {
        Image::Empty => return None,
        Image::Inconclusive => ("inconclusive", 1, MutationOutcome::NotRun),
        Image::Determinate => (
            "determinate",
            2,
            executed_outcome(ProcessTermination::Exit(2), false, "original"),
        ),
        Image::First => (
            "first",
            3,
            executed_outcome(ProcessTermination::Exit(3), false, "first attempt"),
        ),
        Image::Retry => (
            "retry",
            4,
            executed_outcome(ProcessTermination::Exit(0), false, "retry"),
        ),
    };
    let mut request = persist_request();
    let result = &mut request.result;
    RUN.clone_into(&mut result.run_id);
    mutant.clone_into(&mut result.candidate.id);
    result.candidate.sequence = sequence;
    result.candidate.replacement = name.to_owned();
    result.outcome = outcome;
    result.resource_mode = match image {
        Image::First | Image::Retry => ResourceMode::BestEffort,
        _ => ResourceMode::Hard,
    };
    result.diagnostics = ["first", "second"]
        .map(|code| SessionDiagnostic {
            mutant_id: mutant.to_owned(),
            level: "warning".to_owned(),
            code: code.to_owned(),
            message: format!("{name} {code} diagnostic"),
        })
        .into();
    Some(request)
}

fn seed(handler: &mut SessionHandler, witness: &Connection, case: &Case) -> MutantResult {
    handler
        .begin(super::BeginSession {
            run_id: RUN.into(),
            ..begin_request()
        })
        .unwrap();
    let neighbor = payload(Image::Determinate, NEIGHBOR).unwrap();
    handler.persist(&neighbor).unwrap();
    if let Some(saved) = payload(case.stored, SELECTED) {
        handler.persist(&saved).unwrap();
    }
    match case.admission {
        Admission::OpenRun => {}
        Admission::NoOwner => drop(handler.ownerships.remove(RUN).unwrap()),
        Admission::CompletedRun => {
            let changed = witness.execute(
                "UPDATE runs SET finished=1, complete=1 WHERE run_id=?1",
                [RUN],
            );
            assert_eq!(changed.unwrap(), 1);
        }
    }
    neighbor.result
}

fn install_trace(connection: &Connection) {
    connection
        .execute_batch("CREATE TEMP TABLE persisted_writes (step TEXT NOT NULL);")
        .unwrap();
    for write in WRITES {
        connection
            .execute_batch(&format!(
                "CREATE TEMP TRIGGER trace_{} BEFORE {} ON main.{} WHEN {}
                 BEGIN INSERT INTO persisted_writes VALUES('{}'); END;",
                write.name,
                write.event,
                write.table,
                write.where_row(),
                write.name,
            ))
            .unwrap();
    }
}

fn inject_fault(
    handler: &SessionHandler,
    witness: &Connection,
    fault: &str,
    request: &mut PersistResult,
) {
    match fault {
        "none" => {}
        "begin_busy" => {
            handler.connection.busy_timeout(Duration::ZERO).unwrap();
            witness.execute_batch("BEGIN IMMEDIATE").unwrap();
        }
        "candidate_overflow" => request.result.candidate.sequence = u64::MAX,
        "invalid_diagnostic" => {
            NEIGHBOR.clone_into(&mut request.result.diagnostics[1].mutant_id);
        }
        "commit_reject" => reject_commit(handler, "INSERT", "results"),
        stage => {
            let write = WRITES.iter().find(|write| write.name == stage).unwrap();
            handler
                .connection
                .execute_batch(&format!(
                    "CREATE TEMP TRIGGER abort_persist BEFORE {} ON main.{} WHEN {}
                     BEGIN SELECT RAISE(FAIL, 'oracle persist {}'); END;",
                    write.event,
                    write.table,
                    write.where_row(),
                    write.name,
                ))
                .unwrap();
        }
    }
}

fn clear_fault(connection: &Connection, witness: &Connection, fault: &str) {
    if fault == "begin_busy" {
        witness.execute_batch("ROLLBACK").unwrap();
    }
    connection
        .execute_batch(
            "DROP TRIGGER IF EXISTS temp.abort_persist; DROP TRIGGER IF EXISTS temp.reject_commit;",
        )
        .unwrap();
}

fn assert_image(reader: &SessionHandler, image: Image, id: &str) {
    let counts: (i64, i64, i64) = reader
        .connection
        .query_row(
            "SELECT (SELECT count(*) FROM candidates WHERE run_id=?1 AND mutant_id=?2),
                    (SELECT count(*) FROM results WHERE run_id=?1 AND mutant_id=?2),
                    (SELECT count(*) FROM diagnostics WHERE run_id=?1 AND mutant_id=?2)",
            params![RUN, SELECTED],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    if let Some(request) = payload(image, SELECTED) {
        assert_eq!(counts, (1, 1, 2), "{id}: {image:?}");
        assert_eq!(
            reader.read_result(RUN, SELECTED).unwrap(),
            request.result,
            "{id}: {image:?}",
        );
    } else {
        assert_eq!(counts, (0, 0, 0), "{id}: no partial rows");
    }
}

fn assert_ownership(
    handler: &SessionHandler,
    witness: &SessionHandler,
    admission: Admission,
    id: &str,
) {
    let owns = !matches!(admission, Admission::NoOwner);
    assert_eq!(handler.ownerships.contains_key(RUN), owns, "{id}");
    let lock = witness.acquire_ownership(EffectId(5), RUN, "probe persist ownership");
    if owns {
        assert_eq!(
            lock.err().unwrap().failure.code(),
            "session.resume.active",
            "{id}"
        );
    } else {
        drop(lock.unwrap());
    }
}

fn persisted_trace(connection: &Connection) -> Vec<String> {
    connection
        .prepare("SELECT step FROM persisted_writes ORDER BY rowid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn failure_outcome(failure: &EffectFailure) -> &'static str {
    let EffectFailure::SessionDatabase {
        code,
        operation,
        message,
    } = failure
    else {
        panic!("unexpected persist failure: {failure:?}");
    };
    let stage = (code.as_str(), operation.as_str());
    if let Some(marker) = message.strip_prefix("oracle persist ") {
        let write = WRITES.iter().find(|write| write.name == marker).unwrap();
        assert_eq!(stage, write.error_stage(), "{marker}");
        return write.name;
    }
    match stage {
        ("session.persist.owner", "validate session lifecycle") => "owner_required",
        ("session.persist", "begin result transaction") => "begin_error",
        ("session.persist.complete", "validate session lifecycle") => "run_complete",
        ("session.duplicate_result", "validate session lifecycle") => "duplicate",
        ("session.persist", "insert candidate")
            if *message == i64::try_from(u64::MAX).unwrap_err().to_string() =>
        {
            "candidate_overflow"
        }
        ("session.persist.diagnostic", "validate diagnostic") => "invalid_diagnostic",
        ("session.commit", "commit mutant result") => "commit_error",
        _ => panic!("unexpected persist stage: {stage:?} ({message})"),
    }
}

fn attempt(
    handler: &mut SessionHandler,
    witness: &SessionHandler,
    path: &Path,
    expected: &Observation,
    fault: &str,
    image: Image,
    id: &str,
) {
    handler
        .connection
        .execute_batch("DELETE FROM persisted_writes")
        .unwrap();
    let precommit = SessionHandler::open(path).unwrap();
    let visible = expected.before_commit;
    let label = id.to_owned();
    handler.persist_before_commit = Some(Box::new(move || {
        assert_image(
            &precommit,
            visible.expect("unexpected commit attempt"),
            &label,
        );
    }));
    let mut request = payload(image, SELECTED).unwrap();
    request.id = EffectId(request.result.candidate.sequence);
    request.worker = 7;
    inject_fault(handler, &witness.connection, fault, &mut request);
    let reply = handler.persist(&request);
    assert!(handler.connection.is_autocommit(), "{id}");
    let outcome = match reply {
        Ok(persisted) => {
            assert_eq!(persisted.id, request.id, "{id}");
            assert_eq!(persisted.worker, request.worker, "{id}");
            assert_eq!(persisted.run_id, RUN, "{id}");
            assert_eq!(persisted.mutant_id, SELECTED, "{id}");
            "persisted"
        }
        Err(error) => {
            assert_eq!(error.id, request.id, "{id}");
            failure_outcome(&error.failure)
        }
    };
    clear_fault(&handler.connection, &witness.connection, fault);
    let commit_reached = handler.persist_before_commit.take().is_none();
    assert_eq!(commit_reached, expected.before_commit.is_some(), "{id}");
    assert_eq!(outcome, expected.outcome, "{id}");
    assert_eq!(
        persisted_trace(&handler.connection),
        expected.committed_writes,
        "{id}"
    );
    assert_image(witness, expected.after, id);
}

fn replay(case: &Case) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    let witness = SessionHandler::open(&path).unwrap();
    let neighbor = seed(&mut handler, &witness.connection, case);
    assert_image(&witness, case.stored, &case.id);
    install_trace(&handler.connection);
    for (image, expected, fault) in [
        (Image::First, &case.first, case.fault.as_str()),
        (Image::Retry, &case.retry, "none"),
    ] {
        let id = format!("{}: {image:?}", case.id);
        attempt(&mut handler, &witness, &path, expected, fault, image, &id);
        assert_ownership(&handler, &witness, case.admission, &id);
        assert_eq!(
            witness.read_result(RUN, NEIGHBOR).unwrap(),
            neighbor,
            "{id}"
        );
    }
}

#[test]
fn persist_transaction_matches_lean_oracle() {
    let mut ids = BTreeSet::new();
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        replay(&case);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 108);
}
