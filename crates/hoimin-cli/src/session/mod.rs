mod schema;

use std::path::Path;
#[cfg(feature = "contracts")]
use std::time::Duration;

use hoimin_core::{
    BeginSession, EffectFailed, EffectFailure, EffectId, FinishSession, LoadSession,
    LookupStoredResult, MutantResult, MutationStatus, PersistResult, ResourceMode, ResultPersisted,
    SessionDiagnostic, SessionFinished, SessionLoaded, SessionResumeRef, SessionStarted,
    StoredResult, StoredResultLoaded, contract_ensure,
};
#[cfg(feature = "contracts")]
use hoimin_core::{ByteSpan, MutationCandidate, OutputSpoolRef};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, params};
use thiserror::Error;

pub use schema::SchemaError;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error("failed to open SQLite session: {0}")]
    Open(#[from] rusqlite::Error),
}

pub struct SessionHandler {
    connection: Connection,
}

impl SessionHandler {
    /// Opens and configures the `SQLite` database at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when `SQLite` cannot open the database or its schema cannot be
    /// configured.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let connection = Connection::open(path)?;
        schema::configure(&connection)?;
        Ok(Self { connection })
    }

    /// Finds the latest incomplete run compatible with `request`.
    ///
    /// # Errors
    ///
    /// Returns [`EffectFailed`] when the session database cannot be read or contains a corrupt
    /// run identifier.
    pub fn load(&mut self, request: &LoadSession) -> Result<SessionLoaded, EffectFailed> {
        let id = request.id;
        self.connection
            .query_row(
                "SELECT run_id FROM runs
                 WHERE fingerprint=?1 AND complete=0 ORDER BY id DESC LIMIT 1",
                [request.fingerprint.as_bytes().as_slice()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| failed(id, "session.read", "load resume run", &error))
            .and_then(|run_id| match run_id {
                Some(None) => Err(EffectFailed {
                    id,
                    failure: corrupt("NULL run ID"),
                }),
                Some(Some(run_id)) => Ok(Some(SessionResumeRef { run_id })),
                None => Ok(None),
            })
            .map(|resume| SessionLoaded { id, resume })
    }

    /// Looks up a stored mutant result for an incomplete run.
    ///
    /// # Errors
    ///
    /// Returns [`EffectFailed`] when the run is unavailable for resumption, the database cannot
    /// be read, or stored rows are inconsistent.
    pub fn lookup(
        &mut self,
        request: &LookupStoredResult,
    ) -> Result<StoredResultLoaded, EffectFailed> {
        let id = request.id;
        let worker = request.worker;
        let complete = self
            .connection
            .query_row(
                "SELECT complete FROM runs WHERE run_id=?1",
                [&request.run_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
            .map_err(|error| failed(id, "session.read", "lookup run state", &error))?;
        match complete {
            None => {
                return Err(state_failure(
                    id,
                    "session.lookup.state",
                    "run does not exist",
                ));
            }
            Some(None) => {
                return Err(EffectFailed {
                    id,
                    failure: corrupt("NULL complete flag"),
                });
            }
            Some(Some(1)) => {
                return Err(state_failure(
                    id,
                    "session.lookup.complete",
                    "completed run cannot be resumed",
                ));
            }
            Some(Some(0)) => {}
            Some(Some(_)) => {
                return Err(EffectFailed {
                    id,
                    failure: corrupt("invalid complete flag"),
                });
            }
        }

        let (candidates, results, status) =
            result_shape(&self.connection, &request.run_id, &request.mutant_id)
                .map_err(|error| failed(id, "session.read", "lookup stored result", &error))?;
        let result = decode_stored_result(&request.mutant_id, candidates, results, status)
            .map_err(|failure| EffectFailed { id, failure })?;
        Ok(StoredResultLoaded { id, worker, result })
    }

    /// Records a new incomplete run.
    ///
    /// # Errors
    ///
    /// Returns [`EffectFailed`] when the session database cannot start, write, or commit the run
    /// transaction.
    pub fn begin(&mut self, request: BeginSession) -> Result<SessionStarted, EffectFailed> {
        let id = request.id;
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| failed(id, "session.begin", "begin run transaction", &error))?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO fingerprints(digest, schema_version) VALUES (?1, ?2)",
                params![request.fingerprint.as_bytes().as_slice(), 1_i64],
            )
            .and_then(|_| {
                transaction.execute(
                    "INSERT INTO runs(run_id, fingerprint, complete) VALUES (?1, ?2, 0)",
                    params![request.run_id, request.fingerprint.as_bytes().as_slice()],
                )
            })
            .map_err(|error| failed(id, "session.begin", "insert run", &error))?;
        transaction
            .commit()
            .map_err(|error| failed(id, "session.commit", "commit run", &error))?;
        Ok(SessionStarted {
            id,
            run_id: request.run_id,
        })
    }

    /// Stores a mutant result, replacing an earlier inconclusive result when permitted.
    ///
    /// # Errors
    ///
    /// Returns [`EffectFailed`] when the run is complete, the result conflicts with stored data,
    /// diagnostics do not match the mutant, or the database transaction fails.
    pub fn persist(&mut self, request: &PersistResult) -> Result<ResultPersisted, EffectFailed> {
        let id = request.id;
        let worker = request.worker;
        let run_id = request.result.run_id.clone();
        let mutant_id = request.result.candidate.id.clone();
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| failed(id, "session.persist", "begin result transaction", &error))?;
        let complete = transaction
            .query_row(
                "SELECT complete FROM runs WHERE run_id=?1",
                [&run_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
            .map_err(|error| failed(id, "session.persist", "read run state", &error))?;
        match complete {
            Some(Some(1)) => {
                return Err(state_failure(
                    id,
                    "session.persist.complete",
                    "completed run cannot accept results",
                ));
            }
            Some(Some(0)) | None => {}
            Some(None) => {
                return Err(EffectFailed {
                    id,
                    failure: corrupt("NULL complete flag"),
                });
            }
            Some(Some(_)) => {
                return Err(EffectFailed {
                    id,
                    failure: corrupt("invalid complete flag"),
                });
            }
        }

        let (candidate_count, result_count, status) =
            result_shape(&transaction, &run_id, &mutant_id)
                .map_err(|error| failed(id, "session.persist", "read existing result", &error))?;
        if let Some(stored) =
            decode_stored_result(&mutant_id, candidate_count, result_count, status)
                .map_err(|failure| EffectFailed { id, failure })?
        {
            match stored.status {
                MutationStatus::Killed | MutationStatus::Survived => {
                    return Err(state_failure(
                        id,
                        "session.duplicate_result",
                        "determinate result cannot be replaced",
                    ));
                }
                MutationStatus::Timeout
                | MutationStatus::OutOfMemory
                | MutationStatus::ProcessLimit
                | MutationStatus::Error
                | MutationStatus::NotRun => {
                    delete_stored_result(&transaction, &run_id, &mutant_id).map_err(|error| {
                        failed(id, "session.persist", "replace inconclusive result", &error)
                    })?;
                }
            }
        }
        insert_candidate(&transaction, &request.result).map_err(|error| {
            let code = if is_constraint(&error) {
                "session.duplicate_result"
            } else {
                "session.persist"
            };
            failed(id, code, "insert candidate", &error)
        })?;
        insert_result(&transaction, &request.result)
            .map_err(|error| failed(id, "session.persist", "insert result", &error))?;
        persist_diagnostics(
            &transaction,
            &run_id,
            &mutant_id,
            &request.result.diagnostics,
            id,
        )?;
        transaction
            .commit()
            .map_err(|error| failed(id, "session.commit", "commit mutant result", &error))?;

        contract_ensure!(
            "session.commit.post",
            self.read_result(&run_id, &mutant_id) == Ok(request.result.clone()),
            (&run_id, &mutant_id)
        );
        Ok(ResultPersisted {
            id,
            worker,
            run_id,
            mutant_id,
        })
    }

    /// Marks a run as finished, optionally making it ineligible for resumption.
    ///
    /// # Errors
    ///
    /// Returns [`EffectFailed`] when the run is missing or already finished, or its transaction
    /// cannot be started, updated, or committed.
    pub fn finish(&mut self, request: FinishSession) -> Result<SessionFinished, EffectFailed> {
        let id = request.id;
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| failed(id, "session.finish", "begin finish transaction", &error))?;
        let statement = if request.complete {
            "UPDATE runs SET finished=1, complete=1 WHERE run_id=?1 AND complete=0"
        } else {
            "UPDATE runs SET finished=1, complete=0 WHERE run_id=?1 AND complete=0"
        };
        let changed = transaction
            .execute(statement, [&request.run_id])
            .map_err(|error| failed(id, "session.finish", "mark run finished", &error))?;
        if changed != 1 {
            return Err(EffectFailed {
                id,
                failure: EffectFailure::SessionDatabase {
                    code: "session.finish.state".to_owned(),
                    operation: "mark run finished".to_owned(),
                    message: "run is missing or already finished".to_owned(),
                },
            });
        }
        transaction
            .commit()
            .map_err(|error| failed(id, "session.commit", "commit run finish", &error))?;
        Ok(SessionFinished {
            id,
            run_id: request.run_id,
            complete: request.complete,
        })
    }

    #[cfg(feature = "contracts")]
    fn read_result(&self, run_id: &str, mutant_id: &str) -> Result<MutantResult, String> {
        let raw = self
            .connection
            .query_row(
                "SELECT c.sequence,c.path,c.span_start,c.span_length,c.original,c.replacement,
                        c.operator,c.line,c.column_number,c.symbol,c.file_hash,
                        r.status,r.elapsed_secs,r.elapsed_nanos,r.resource_mode,
                        r.output_token,r.output_retained,r.output_observed
                 FROM candidates c JOIN results r USING(run_id,mutant_id)
                 WHERE c.run_id=?1 AND c.mutant_id=?2",
                params![run_id, mutant_id],
                |row| {
                    Ok(RawResult {
                        sequence: row.get(0)?,
                        path: row.get(1)?,
                        span_start: row.get(2)?,
                        span_length: row.get(3)?,
                        original: row.get(4)?,
                        replacement: row.get(5)?,
                        operator: row.get(6)?,
                        line: row.get(7)?,
                        column: row.get(8)?,
                        symbol: row.get(9)?,
                        file_hash: row.get(10)?,
                        status: row.get(11)?,
                        elapsed_secs: row.get(12)?,
                        elapsed_nanos: row.get(13)?,
                        resource_mode: row.get(14)?,
                        output_token: row.get(15)?,
                        output_retained: row.get(16)?,
                        output_observed: row.get(17)?,
                    })
                },
            )
            .map_err(|error| error.to_string())?;
        let mut result = raw.into_result(run_id, mutant_id)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT mutant_id,level,code,message FROM diagnostics
                 WHERE run_id=?1 AND mutant_id=?2 ORDER BY id",
            )
            .map_err(|error| error.to_string())?;
        result.diagnostics = statement
            .query_map(params![run_id, mutant_id], |row| {
                Ok(SessionDiagnostic {
                    mutant_id: row.get(0)?,
                    level: row.get(1)?,
                    code: row.get(2)?,
                    message: row.get(3)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(result)
    }
}

fn insert_candidate(transaction: &Transaction<'_>, result: &MutantResult) -> rusqlite::Result<()> {
    let candidate = &result.candidate;
    transaction.execute(
        "INSERT INTO candidates(run_id,mutant_id,sequence,path,span_start,span_length,original,
         replacement,operator,line,column_number,symbol,file_hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            result.run_id,
            candidate.id,
            to_i64(candidate.sequence)?,
            candidate.path.as_str(),
            to_i64(candidate.span.start)?,
            to_i64(candidate.span.length)?,
            candidate.original,
            candidate.replacement,
            candidate.operator,
            i64::from(candidate.line),
            i64::from(candidate.column),
            candidate.symbol,
            candidate.file_hash,
        ],
    )?;
    Ok(())
}

fn insert_result(transaction: &Transaction<'_>, result: &MutantResult) -> rusqlite::Result<()> {
    let (token, retained, observed) = match &result.output {
        Some(output) => (
            Some(output.token.as_str()),
            Some(to_i64(output.retained)?),
            Some(to_i64(output.observed)?),
        ),
        None => (None, None, None),
    };
    transaction.execute(
        "INSERT INTO results(run_id,mutant_id,status,elapsed_secs,elapsed_nanos,resource_mode,
         output_token,output_retained,output_observed) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            result.run_id,
            result.candidate.id,
            status_name(result.status),
            to_i64(result.elapsed.as_secs())?,
            i64::from(result.elapsed.subsec_nanos()),
            resource_name(result.resource_mode),
            token,
            retained,
            observed,
        ],
    )?;
    Ok(())
}

fn persist_diagnostics(
    transaction: &Transaction<'_>,
    run_id: &str,
    mutant_id: &str,
    diagnostics: &[SessionDiagnostic],
    id: EffectId,
) -> Result<(), EffectFailed> {
    for diagnostic in diagnostics {
        if diagnostic.mutant_id != mutant_id {
            return Err(EffectFailed {
                id,
                failure: EffectFailure::SessionDatabase {
                    code: "session.persist.diagnostic".to_owned(),
                    operation: "validate diagnostic".to_owned(),
                    message: "diagnostic mutant ID does not match result".to_owned(),
                },
            });
        }
        insert_diagnostic(transaction, run_id, diagnostic)
            .map_err(|error| failed(id, "session.persist", "insert diagnostic", &error))?;
    }
    Ok(())
}

fn insert_diagnostic(
    transaction: &Transaction<'_>,
    run_id: &str,
    diagnostic: &SessionDiagnostic,
) -> rusqlite::Result<()> {
    transaction.execute(
        "INSERT INTO diagnostics(run_id,mutant_id,level,code,message) VALUES (?1,?2,?3,?4,?5)",
        params![
            run_id,
            diagnostic.mutant_id,
            diagnostic.level,
            diagnostic.code,
            diagnostic.message
        ],
    )?;
    Ok(())
}

fn result_shape(
    connection: &Connection,
    run_id: &str,
    mutant_id: &str,
) -> rusqlite::Result<(i64, i64, Option<String>)> {
    connection.query_row(
        "SELECT
             (SELECT count(*) FROM candidates WHERE run_id=?1 AND mutant_id=?2),
             (SELECT count(*) FROM results WHERE run_id=?1 AND mutant_id=?2),
             (SELECT status FROM results WHERE run_id=?1 AND mutant_id=?2)",
        params![run_id, mutant_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
}

fn decode_stored_result(
    mutant_id: &str,
    candidate_count: i64,
    result_count: i64,
    status: Option<String>,
) -> Result<Option<StoredResult>, EffectFailure> {
    match (candidate_count, result_count, status) {
        (0, 0, None) => Ok(None),
        (1, 1, Some(status)) => Ok(Some(StoredResult {
            mutant_id: mutant_id.to_owned(),
            status: decode_status(&status).ok_or_else(|| corrupt("unknown mutation status"))?,
        })),
        _ => Err(corrupt("candidate and result rows are inconsistent")),
    }
}

fn delete_stored_result(
    transaction: &Transaction<'_>,
    run_id: &str,
    mutant_id: &str,
) -> rusqlite::Result<()> {
    transaction.execute(
        "DELETE FROM diagnostics WHERE run_id=?1 AND mutant_id=?2",
        params![run_id, mutant_id],
    )?;
    transaction.execute(
        "DELETE FROM results WHERE run_id=?1 AND mutant_id=?2",
        params![run_id, mutant_id],
    )?;
    transaction.execute(
        "DELETE FROM candidates WHERE run_id=?1 AND mutant_id=?2",
        params![run_id, mutant_id],
    )?;
    Ok(())
}

fn to_i64(value: u64) -> rusqlite::Result<i64> {
    i64::try_from(value).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn is_constraint(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(value, _) if value.code == ErrorCode::ConstraintViolation)
}

fn status_name(status: MutationStatus) -> &'static str {
    match status {
        MutationStatus::Killed => "killed",
        MutationStatus::Survived => "survived",
        MutationStatus::Timeout => "timeout",
        MutationStatus::OutOfMemory => "out_of_memory",
        MutationStatus::ProcessLimit => "process_limit",
        MutationStatus::Error => "error",
        MutationStatus::NotRun => "not_run",
    }
}

fn decode_status(value: &str) -> Option<MutationStatus> {
    Some(match value {
        "killed" => MutationStatus::Killed,
        "survived" => MutationStatus::Survived,
        "timeout" => MutationStatus::Timeout,
        "out_of_memory" => MutationStatus::OutOfMemory,
        "process_limit" => MutationStatus::ProcessLimit,
        "error" => MutationStatus::Error,
        "not_run" => MutationStatus::NotRun,
        _ => return None,
    })
}

fn resource_name(mode: ResourceMode) -> &'static str {
    match mode {
        ResourceMode::Hard => "hard",
        ResourceMode::BestEffort => "best_effort",
    }
}

fn failed(id: EffectId, code: &str, operation: &str, error: &rusqlite::Error) -> EffectFailed {
    EffectFailed {
        id,
        failure: database(code, operation, error),
    }
}

fn state_failure(id: EffectId, code: &str, message: &str) -> EffectFailed {
    EffectFailed {
        id,
        failure: EffectFailure::SessionDatabase {
            code: code.to_owned(),
            operation: "validate session lifecycle".to_owned(),
            message: message.to_owned(),
        },
    }
}

fn database(code: &str, operation: &str, error: &rusqlite::Error) -> EffectFailure {
    EffectFailure::SessionDatabase {
        code: code.to_owned(),
        operation: operation.to_owned(),
        message: error.to_string(),
    }
}

fn corrupt(message: &str) -> EffectFailure {
    EffectFailure::SessionDatabase {
        code: "session.corrupt".to_owned(),
        operation: "decode session row".to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(feature = "contracts")]
struct RawResult {
    sequence: i64,
    path: String,
    span_start: i64,
    span_length: i64,
    original: String,
    replacement: String,
    operator: String,
    line: i64,
    column: i64,
    symbol: Option<String>,
    file_hash: String,
    status: String,
    elapsed_secs: i64,
    elapsed_nanos: i64,
    resource_mode: String,
    output_token: Option<String>,
    output_retained: Option<i64>,
    output_observed: Option<i64>,
}

#[cfg(feature = "contracts")]
impl RawResult {
    fn into_result(self, run_id: &str, mutant_id: &str) -> Result<MutantResult, String> {
        let nonnegative =
            |value: i64, name: &str| u64::try_from(value).map_err(|_| format!("negative {name}"));
        let status = decode_status(&self.status).ok_or_else(|| "unknown status".to_owned())?;
        let resource_mode = match self.resource_mode.as_str() {
            "hard" => ResourceMode::Hard,
            "best_effort" => ResourceMode::BestEffort,
            _ => return Err("unknown resource mode".to_owned()),
        };
        let output = match (
            self.output_token,
            self.output_retained,
            self.output_observed,
        ) {
            (None, None, None) => None,
            (Some(token), Some(retained), Some(observed)) => Some(OutputSpoolRef {
                token,
                retained: nonnegative(retained, "retained")?,
                observed: nonnegative(observed, "observed")?,
            }),
            _ => return Err("inconsistent output columns".to_owned()),
        };
        Ok(MutantResult {
            run_id: run_id.to_owned(),
            candidate: MutationCandidate {
                id: mutant_id.to_owned(),
                sequence: nonnegative(self.sequence, "sequence")?,
                path: self.path.into(),
                span: ByteSpan {
                    start: nonnegative(self.span_start, "span start")?,
                    length: nonnegative(self.span_length, "span length")?,
                },
                original: self.original,
                replacement: self.replacement,
                operator: self.operator,
                line: u32::try_from(self.line).map_err(|_| "invalid line".to_owned())?,
                column: u32::try_from(self.column).map_err(|_| "invalid column".to_owned())?,
                symbol: self.symbol,
                file_hash: self.file_hash,
            },
            status,
            elapsed: Duration::new(
                nonnegative(self.elapsed_secs, "elapsed seconds")?,
                u32::try_from(self.elapsed_nanos)
                    .map_err(|_| "invalid elapsed nanos".to_owned())?,
            ),
            resource_mode,
            output,
            diagnostics: Vec::new(),
        })
    }
}
