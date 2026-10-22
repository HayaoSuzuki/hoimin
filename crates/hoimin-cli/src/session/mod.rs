mod ownership;
mod schema;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Barrier;
use std::sync::{Arc, Mutex};

use hoimin_core::{
    BeginSession, EffectFailed, EffectFailure, EffectId, FINGERPRINT_SCHEMA_VERSION, FinishSession,
    LoadSession, LookupStoredResult, MutantResult, MutationOutcome, MutationStatus, PersistResult,
    ResourceMode, ResultPersisted, SessionDiagnostic, SessionFinished, SessionLoadOutcome,
    SessionLoaded, SessionResumeRef, SessionStarted, StoredResult, StoredResultLoaded,
    contract_ensure,
};
#[cfg(feature = "contracts")]
use hoimin_core::{ByteSpan, MutationCandidate};
use rusqlite::{
    Connection, ErrorCode, OptionalExtension, Transaction, TransactionBehavior, params,
};
use thiserror::Error;

pub use schema::SchemaError;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error("failed to open SQLite session: {0}")]
    Open(#[from] rusqlite::Error),
    #[error("SQLite session blocking task failed: {0}")]
    Task(String),
    #[error("failed to prepare session ownership: {0}")]
    Ownership(#[source] std::io::Error),
}

/// Resolved names owned by a persistent session, discovered without creating artifacts.
#[derive(Clone, Debug)]
pub struct SessionArtifacts {
    database: PathBuf,
}

impl SessionArtifacts {
    /// Resolves an existing database or the existing parent of a new database.
    ///
    /// # Errors
    /// Returns an I/O error for inaccessible paths, missing parents, or dangling symlinks.
    pub fn resolve(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let database = match std::fs::symlink_metadata(path) {
            Ok(_) => std::fs::canonicalize(path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = path.file_name().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "session database path has no file name",
                    )
                })?;
                let parent = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                std::fs::canonicalize(parent)?.join(name)
            }
            Err(error) => return Err(error),
        };
        if database.file_name().is_none() || database.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "session database path must name a file",
            ));
        }
        Ok(Self { database })
    }

    #[must_use]
    pub fn database(&self) -> &Path {
        &self.database
    }

    #[must_use]
    pub fn files(&self) -> [PathBuf; 4] {
        let companions = crate::artifacts::SqliteCompanions::new(&self.database);
        [
            self.database.clone(),
            companions.wal,
            companions.shm,
            companions.journal,
        ]
    }

    #[must_use]
    pub fn lock_directory(&self) -> PathBuf {
        ownership::lock_directory(&self.database)
    }

    /// Lexical and existing canonical ownership trees, without creating either.
    /// Resolve these at blocking preflight boundaries, before taking a snapshot.
    /// A missing tree will be created by ownership acquisition.
    ///
    /// # Errors
    /// Returns canonicalization errors other than `NotFound`, so an uncertain
    /// identity cannot silently remove protection of an existing tree.
    pub fn lock_trees(&self) -> std::io::Result<Vec<PathBuf>> {
        let lexical = self.lock_directory();
        let mut trees = vec![lexical.clone()];
        match std::fs::canonicalize(&lexical) {
            Ok(actual) if actual != lexical => trees.push(actual),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(trees)
    }
}

pub struct SessionHandler {
    connection: Connection,
    lock_directory: PathBuf,
    ownerships: HashMap<String, ownership::RunOwnership>,
    #[cfg(test)]
    persist_after_reads: Option<Arc<Barrier>>,
}

fn validate_resume_budget(
    id: EffectId,
    stored_limit: rusqlite::types::Value,
    requested: std::num::NonZeroUsize,
) -> Result<Vec<u8>, EffectFailed> {
    let rusqlite::types::Value::Blob(stored_limit) = stored_limit else {
        return Err(EffectFailed {
            id,
            failure: corrupt("invalid persisted mutant budget type"),
        });
    };
    let stored_bytes: [u8; 8] = stored_limit
        .as_slice()
        .try_into()
        .map_err(|_| EffectFailed {
            id,
            failure: corrupt("invalid persisted mutant budget length"),
        })?;
    let stored_budget = u64::from_be_bytes(stored_bytes);
    if stored_budget == 0 || stored_budget > requested.get() as u64 {
        return Err(EffectFailed {
            id,
            failure: corrupt("invalid persisted mutant budget"),
        });
    }
    Ok(stored_limit)
}

impl SessionHandler {
    /// Opens and configures the `SQLite` database at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when `SQLite` cannot open the database or its schema cannot be
    /// configured.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let artifacts = SessionArtifacts::resolve(path).map_err(SessionError::Ownership)?;
        let connection = schema::open(artifacts.database())?;
        let lock_directory = artifacts.lock_directory();
        Ok(Self {
            connection,
            lock_directory,
            ownerships: HashMap::new(),
            #[cfg(test)]
            persist_after_reads: None,
        })
    }

    /// Finds the latest incomplete run compatible with `request`.
    ///
    /// # Errors
    ///
    /// Returns [`EffectFailed`] when the session database cannot be read or contains a corrupt
    /// run identifier or persisted budget.
    pub fn load(&mut self, request: &LoadSession) -> Result<SessionLoaded, EffectFailed> {
        let id = request.id;
        let limit = (request.max_mutants.get() as u64).to_be_bytes();
        let run_id = self
            .connection
            .query_row(
                "SELECT run_id, max_mutants FROM runs
                 WHERE fingerprint=?1 AND complete=0
                   AND (max_mutants<=?2 OR typeof(max_mutants)!='blob'
                        OR length(max_mutants)!=8 OR max_mutants=zeroblob(8))
                 ORDER BY id DESC LIMIT 1",
                params![request.fingerprint.as_bytes().as_slice(), limit.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, rusqlite::types::Value>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| failed(id, "session.read", "load resume run", &error))?;
        let Some((run_id, stored_limit)) = run_id else {
            let latest_schema = self
                .connection
                .query_row(
                    "SELECT fingerprints.schema_version FROM runs
                     JOIN fingerprints ON fingerprints.digest=runs.fingerprint
                     WHERE runs.complete=0 ORDER BY runs.id DESC LIMIT 1",
                    [],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()
                .map_err(|error| failed(id, "session.read", "inspect resume schema", &error))?;
            if let Some(schema) = latest_schema {
                let schema = schema.ok_or_else(|| EffectFailed {
                    id,
                    failure: corrupt("NULL fingerprint schema version"),
                })?;
                if schema < i64::from(FINGERPRINT_SCHEMA_VERSION) {
                    let expected = FINGERPRINT_SCHEMA_VERSION;
                    return Err(EffectFailed::other(
                        id,
                        "session.resume.incompatible",
                        format!(
                            "incomplete session uses fingerprint schema {schema}, expected {expected}; start a new session"
                        ),
                    ));
                }
            }
            return Ok(SessionLoaded {
                id,
                outcome: SessionLoadOutcome::Fresh(self.fresh_resume_reason(request)?),
            });
        };
        let run_id = run_id.ok_or_else(|| EffectFailed {
            id,
            failure: corrupt("NULL run ID"),
        })?;
        let stored_limit = validate_resume_budget(id, stored_limit, request.max_mutants)?;
        let ownership = if self.ownerships.contains_key(&run_id) {
            None
        } else {
            Some(self.acquire_ownership(id, &run_id, "claim resume run")?)
        };
        let eligible = self
            .connection
            .execute(
                "UPDATE runs SET max_mutants=?3
                 WHERE run_id=?1 AND fingerprint=?2 AND complete=0 AND max_mutants=?4",
                params![
                    run_id,
                    request.fingerprint.as_bytes().as_slice(),
                    limit.as_slice(),
                    stored_limit.as_slice()
                ],
            )
            .map_err(|error| failed(id, "session.read", "re-read resume run", &error))?
            == 1;
        if !eligible {
            return Ok(SessionLoaded {
                id,
                outcome: SessionLoadOutcome::Fresh(
                    hoimin_core::ResumeFreshReason::CandidateChanged,
                ),
            });
        }
        if let Some(ownership) = ownership {
            self.ownerships.insert(run_id.clone(), ownership);
        }
        Ok(SessionLoaded {
            id,
            outcome: SessionLoadOutcome::Resumed(SessionResumeRef { run_id }),
        })
    }

    fn fresh_resume_reason(
        &self,
        request: &LoadSession,
    ) -> Result<hoimin_core::ResumeFreshReason, EffectFailed> {
        let limit = (request.max_mutants.get() as u64).to_be_bytes();
        let (eligible, budget_decreased, matching_complete, incomplete, any): (
            bool,
            bool,
            bool,
            bool,
            bool,
        ) = self
            .connection
            .query_row(
                "SELECT
                EXISTS(SELECT 1 FROM runs WHERE fingerprint=?1 AND complete=0 AND max_mutants<=?2),
                EXISTS(SELECT 1 FROM runs WHERE fingerprint=?1 AND complete=0 AND max_mutants>?2),
                EXISTS(SELECT 1 FROM runs WHERE fingerprint=?1 AND complete=1),
                EXISTS(SELECT 1 FROM runs WHERE complete=0),
                EXISTS(SELECT 1 FROM runs)",
                params![request.fingerprint.as_bytes().as_slice(), limit.as_slice()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .map_err(|error| {
                failed(request.id, "session.read", "inspect resume history", &error)
            })?;
        Ok(classify_fresh_history((
            eligible,
            budget_decreased,
            matching_complete,
            incomplete,
            any,
        )))
    }

    /// Looks up a stored mutant result for an owned, incomplete run.
    ///
    /// # Errors
    ///
    /// Returns [`EffectFailed`] when this handler does not own the run, the run is unavailable
    /// for resumption, the database cannot be read, or stored rows are inconsistent. Ownership
    /// must be acquired through `begin` or `load`, and is released by a successful `finish`.
    pub fn lookup(
        &mut self,
        request: &LookupStoredResult,
    ) -> Result<StoredResultLoaded, EffectFailed> {
        let id = request.id;
        self.require_ownership(id, &request.run_id, "session.lookup.owner")?;
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

        let (candidates, results, outcome_json) =
            result_shape(&self.connection, &request.run_id, &request.mutant_id)
                .map_err(|error| failed(id, "session.read", "lookup stored result", &error))?;
        let result = decode_stored_result(&request.mutant_id, candidates, results, outcome_json)
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
        let ownership = if self.ownerships.contains_key(&request.run_id) {
            None
        } else {
            Some(self.acquire_ownership(id, &request.run_id, "claim new run")?)
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| failed(id, "session.begin", "begin run transaction", &error))?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO fingerprints(digest, schema_version) VALUES (?1, ?2)",
                params![
                    request.fingerprint.as_bytes().as_slice(),
                    i64::from(FINGERPRINT_SCHEMA_VERSION),
                ],
            )
            .and_then(|_| {
                transaction.execute(
                    "INSERT INTO runs(run_id, fingerprint, complete, max_mutants)
                     VALUES (?1, ?2, 0, ?3)",
                    params![
                        request.run_id,
                        request.fingerprint.as_bytes().as_slice(),
                        (request.max_mutants.get() as u64).to_be_bytes().as_slice(),
                    ],
                )
            })
            .map_err(|error| failed(id, "session.begin", "insert run", &error))?;
        transaction
            .commit()
            .map_err(|error| failed(id, "session.commit", "commit run", &error))?;
        if let Some(ownership) = ownership {
            self.ownerships.insert(request.run_id.clone(), ownership);
        }
        Ok(SessionStarted {
            id,
            run_id: request.run_id,
        })
    }

    /// Stores a mutant result, replacing an earlier inconclusive result when permitted.
    ///
    /// # Errors
    ///
    /// Returns [`EffectFailed`] when this handler does not own the run, the run is complete,
    /// the result conflicts with stored data, diagnostics do not match the mutant, or the
    /// database transaction fails. Ownership must be acquired through `begin` or `load`, and
    /// is released by a successful `finish`.
    pub fn persist(&mut self, request: &PersistResult) -> Result<ResultPersisted, EffectFailed> {
        let id = request.id;
        self.require_ownership(id, &request.result.run_id, "session.persist.owner")?;
        let run_id = request.result.run_id.clone();
        let mutant_id = request.result.candidate.id.clone();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
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

        let (candidate_count, result_count, outcome_json) =
            result_shape(&transaction, &run_id, &mutant_id)
                .map_err(|error| failed(id, "session.persist", "read existing result", &error))?;
        #[cfg(test)]
        if let Some(barrier) = &self.persist_after_reads {
            barrier.wait();
            barrier.wait();
        }
        if let Some(stored) =
            decode_stored_result(&mutant_id, candidate_count, result_count, outcome_json)
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
            worker: request.worker,
            run_id,
            mutant_id,
        })
    }

    /// Marks a run as finished, optionally making it ineligible for resumption.
    ///
    /// # Errors
    ///
    /// Returns [`EffectFailed`] when this handler does not own the run, the run is missing or
    /// already complete, or its transaction cannot be started, updated, or committed. A
    /// successful finish releases ownership, so later finish calls require a new load first.
    pub fn finish(&mut self, request: FinishSession) -> Result<SessionFinished, EffectFailed> {
        let id = request.id;
        self.require_ownership(id, &request.run_id, "session.finish.owner")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
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
        self.ownerships.remove(&request.run_id);
        Ok(SessionFinished {
            id,
            run_id: request.run_id,
            complete: request.complete,
        })
    }

    fn require_ownership(
        &self,
        id: EffectId,
        run_id: &str,
        code: &str,
    ) -> Result<(), EffectFailed> {
        if self.ownerships.contains_key(run_id) {
            Ok(())
        } else {
            Err(state_failure(id, code, "handler does not own run"))
        }
    }

    fn acquire_ownership(
        &self,
        id: EffectId,
        run_id: &str,
        operation: &str,
    ) -> Result<ownership::RunOwnership, EffectFailed> {
        ownership::RunOwnership::acquire(&self.lock_directory, run_id).map_err(|error| {
            let (code, message) = match error {
                ownership::AcquireError::Active => (
                    "session.resume.active",
                    "session run is active in another process".to_owned(),
                ),
                ownership::AcquireError::Io(error) => ("session.ownership", error.to_string()),
            };
            EffectFailed {
                id,
                failure: EffectFailure::SessionDatabase {
                    code: code.to_owned(),
                    operation: operation.to_owned(),
                    message,
                },
            }
        })
    }

    #[cfg(feature = "contracts")]
    fn read_result(&self, run_id: &str, mutant_id: &str) -> Result<MutantResult, String> {
        let raw = self
            .connection
            .query_row(
                "SELECT c.sequence,c.path,c.span_start,c.span_length,c.original,c.replacement,
                        c.operator,c.line,c.column_number,c.symbol,c.file_hash,
                        r.outcome_json,r.resource_mode
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
                        outcome_json: row.get(11)?,
                        resource_mode: row.get(12)?,
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

#[derive(Clone)]
pub(crate) struct SessionDispatcher {
    handler: Arc<Mutex<SessionHandler>>,
}

impl SessionDispatcher {
    pub(crate) async fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let path = path.as_ref().to_owned();
        let handler = tokio::task::spawn_blocking(move || SessionHandler::open(path))
            .await
            .map_err(|error| SessionError::Task(error.to_string()))??;
        Ok(Self {
            handler: Arc::new(Mutex::new(handler)),
        })
    }

    pub(crate) async fn load(&self, request: LoadSession) -> Result<SessionLoaded, EffectFailed> {
        let id = request.id;
        self.call(id, move |handler| handler.load(&request)).await
    }

    pub(crate) async fn lookup(
        &self,
        request: LookupStoredResult,
    ) -> Result<StoredResultLoaded, EffectFailed> {
        let id = request.id;
        self.call(id, move |handler| handler.lookup(&request)).await
    }

    pub(crate) async fn begin(
        &self,
        request: BeginSession,
    ) -> Result<SessionStarted, EffectFailed> {
        let id = request.id;
        self.call(id, move |handler| handler.begin(request)).await
    }

    pub(crate) async fn persist(
        &self,
        request: PersistResult,
    ) -> Result<ResultPersisted, EffectFailed> {
        let id = request.id;
        self.call(id, move |handler| handler.persist(&request))
            .await
    }

    pub(crate) async fn finish(
        &self,
        request: FinishSession,
    ) -> Result<SessionFinished, EffectFailed> {
        let id = request.id;
        self.call(id, move |handler| handler.finish(request)).await
    }

    async fn call<T, F>(&self, id: EffectId, operation: F) -> Result<T, EffectFailed>
    where
        T: Send + 'static,
        F: FnOnce(&mut SessionHandler) -> Result<T, EffectFailed> + Send + 'static,
    {
        let handler = self.handler.clone();
        tokio::task::spawn_blocking(move || {
            let mut handler = handler.lock().map_err(|_| {
                EffectFailed::other(id, "session.dispatch", "session lock poisoned")
            })?;
            operation(&mut handler)
        })
        .await
        .map_err(|error| EffectFailed::other(id, "session.dispatch", error.to_string()))?
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
    let outcome_json = serde_json::to_string(&result.outcome)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    transaction.execute(
        "INSERT INTO results(run_id,mutant_id,outcome_json,resource_mode) VALUES (?1,?2,?3,?4)",
        params![
            result.run_id,
            result.candidate.id,
            outcome_json,
            resource_name(result.resource_mode)
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
             (SELECT outcome_json FROM results WHERE run_id=?1 AND mutant_id=?2)",
        params![run_id, mutant_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
}

fn decode_stored_result(
    mutant_id: &str,
    candidate_count: i64,
    result_count: i64,
    outcome_json: Option<String>,
) -> Result<Option<StoredResult>, EffectFailure> {
    match (candidate_count, result_count, outcome_json) {
        (0, 0, None) => Ok(None),
        (1, 1, Some(outcome_json)) => Ok(Some(StoredResult {
            mutant_id: mutant_id.to_owned(),
            status: serde_json::from_str::<MutationOutcome>(&outcome_json)
                .map_err(|_| corrupt("invalid mutation outcome"))?
                .status(),
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
    outcome_json: String,
    resource_mode: String,
}

#[cfg(feature = "contracts")]
impl RawResult {
    fn into_result(self, run_id: &str, mutant_id: &str) -> Result<MutantResult, String> {
        let nonnegative =
            |value: i64, name: &str| u64::try_from(value).map_err(|_| format!("negative {name}"));
        let resource_mode = match self.resource_mode.as_str() {
            "hard" => ResourceMode::Hard,
            "best_effort" => ResourceMode::BestEffort,
            _ => return Err("unknown resource mode".to_owned()),
        };
        let outcome = serde_json::from_str::<MutationOutcome>(&self.outcome_json)
            .map_err(|error| format!("invalid mutation outcome: {error}"))?;
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
            outcome,
            resource_mode,
            diagnostics: Vec::new(),
        })
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use hoimin_core::{ByteSpan, MutationCandidate, RunFingerprint};
    use std::time::Duration;

    #[test]
    fn persist_reserves_the_writer_before_reading_session_state() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("sessions.sqlite3");
        let mut owner = SessionHandler::open(&path).unwrap();
        for run_id in ["persisted-run", "competing-run"] {
            owner
                .begin(BeginSession {
                    max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
                    id: EffectId(1),
                    run_id: run_id.to_owned(),
                    fingerprint: RunFingerprint::from_bytes([1; 32]),
                })
                .unwrap();
        }
        let competitor = Connection::open(&path).unwrap();
        competitor.busy_timeout(Duration::ZERO).unwrap();

        let barrier = Arc::new(Barrier::new(2));
        owner.persist_after_reads = Some(barrier.clone());
        let persist = std::thread::spawn(move || owner.persist(&persist_request()));

        barrier.wait();
        let competing_write = competitor.execute(
            "UPDATE runs SET finished=1 WHERE run_id='competing-run'",
            [],
        );
        barrier.wait();
        let persisted = persist.join().expect("persist thread must not panic");

        assert!(
            matches!(
                competing_write,
                Err(rusqlite::Error::SqliteFailure(error, _))
                    if error.code == ErrorCode::DatabaseBusy
                        && error.extended_code == rusqlite::ffi::SQLITE_BUSY
            ),
            "the competing writer must lose the writer reservation: competitor={competing_write:?}, persist={persisted:?}"
        );
        persisted.expect("persist must keep its valid snapshot and commit");
    }

    fn persist_request() -> PersistResult {
        PersistResult {
            id: EffectId(2),
            worker: 0,
            result: MutantResult {
                run_id: "persisted-run".to_owned(),
                candidate: MutationCandidate {
                    id: "m1".to_owned(),
                    sequence: 0,
                    path: "src/example.py".into(),
                    span: ByteSpan {
                        start: 0,
                        length: 1,
                    },
                    original: "+".to_owned(),
                    replacement: "-".to_owned(),
                    operator: "binary".to_owned(),
                    line: 1,
                    column: 0,
                    symbol: None,
                    file_hash: "hash".to_owned(),
                },
                outcome: match MutationStatus::Killed {
                    hoimin_core::MutationStatus::Killed => hoimin_core::MutationOutcome::Reused {
                        status: hoimin_core::ReusedMutationStatus::Killed,
                    },
                    hoimin_core::MutationStatus::Survived => hoimin_core::MutationOutcome::Reused {
                        status: hoimin_core::ReusedMutationStatus::Survived,
                    },
                    hoimin_core::MutationStatus::NotRun => hoimin_core::MutationOutcome::NotRun,
                    status => hoimin_core::MutationOutcome::Executed {
                        termination: match status {
                            hoimin_core::MutationStatus::Timeout => {
                                hoimin_core::ProcessTermination::Timeout
                            }
                            hoimin_core::MutationStatus::OutOfMemory => {
                                hoimin_core::ProcessTermination::OutOfMemory
                            }
                            hoimin_core::MutationStatus::ProcessLimit => {
                                hoimin_core::ProcessTermination::ProcessLimit
                            }
                            hoimin_core::MutationStatus::Error => {
                                hoimin_core::ProcessTermination::Exit(1)
                            }
                            _ => unreachable!(),
                        },
                        output_state: if status == hoimin_core::MutationStatus::Error {
                            hoimin_core::ProcessOutputState::CloseTimedOut
                        } else {
                            hoimin_core::ProcessOutputState::Complete
                        },
                        output: hoimin_core::OutputSpoolRef {
                            token: "fixture-output".into(),
                            retained: 0,
                            observed: 0,
                        },
                        elapsed: Duration::from_millis(5),
                    },
                },
                resource_mode: ResourceMode::Hard,
                diagnostics: Vec::new(),
            },
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn conflicting_lock_does_not_block_the_async_deadline() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("locked.sqlite3");
        drop(SessionHandler::open(&path).unwrap());
        let blocker = Connection::open(&path).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
        let dispatcher = SessionDispatcher::open(path).await.unwrap();
        let operation = dispatcher.begin(BeginSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(91),
            run_id: "blocked".to_owned(),
            fingerprint: RunFingerprint::from_bytes([9; 32]),
        });
        tokio::pin!(operation);

        assert!(
            tokio::time::timeout(Duration::from_millis(200), &mut operation)
                .await
                .is_err(),
            "the real SQLite operation must still be waiting on the lock"
        );
        blocker.execute_batch("ROLLBACK").unwrap();

        let started = operation.await.unwrap();
        assert_eq!(started.id, EffectId(91));
        assert_eq!(started.run_id, "blocked");
    }
}

fn classify_fresh_history(
    (eligible, budget_decreased, matching_complete, incomplete, any): (
        bool,
        bool,
        bool,
        bool,
        bool,
    ),
) -> hoimin_core::ResumeFreshReason {
    use hoimin_core::ResumeFreshReason;
    if eligible {
        ResumeFreshReason::CandidateChanged
    } else if budget_decreased {
        ResumeFreshReason::BudgetDecreased
    } else if matching_complete {
        ResumeFreshReason::MatchingRunComplete
    } else if incomplete {
        ResumeFreshReason::FingerprintMismatch
    } else if any {
        ResumeFreshReason::NoIncompleteRun
    } else {
        ResumeFreshReason::NoPriorRun
    }
}

#[cfg(test)]
mod resume_history_tests {
    #[test]
    fn an_eligible_row_appearing_after_candidate_selection_is_not_reported_as_mismatch() {
        for budget in [false, true] {
            for completed in [false, true] {
                assert_eq!(
                    super::classify_fresh_history((true, budget, completed, true, true)),
                    hoimin_core::ResumeFreshReason::CandidateChanged
                );
            }
        }
    }
}
