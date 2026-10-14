use std::path::Path;
use std::time::{Duration, Instant};

use camino::Utf8Path;
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};

use super::ReviewError;
use super::model::{
    ReviewDraft, ReviewEvent, ReviewKey, ReviewState, SourceState, applicability, apply_update,
    validate_draft,
};

const APPLICATION_ID: i64 = 0x484d_5256;
const DB_LIMIT: u64 = 16 * 1024 * 1024;
const EVENT_LIMIT: usize = 10_000;
const METADATA: &str = "CREATE TABLE metadata (project TEXT NOT NULL UNIQUE)";
const CANDIDATES: &str =
    "CREATE TABLE candidates (id TEXT PRIMARY KEY NOT NULL, key_json TEXT NOT NULL)";
const EVENTS: &str = "CREATE TABLE review_events (candidate_id TEXT NOT NULL REFERENCES candidates(id), revision BLOB NOT NULL CHECK(length(revision)=8), draft_json TEXT NOT NULL, PRIMARY KEY(candidate_id, revision))";

pub(super) struct ReviewStore {
    connection: Connection,
    project: camino::Utf8PathBuf,
}

fn invalid(message: &str) -> ReviewError {
    ReviewError::Invalid(message.into())
}

impl ReviewStore {
    pub(super) fn open_read(path: &Path, project: &Utf8Path) -> Result<Option<Self>, ReviewError> {
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => (),
        }
        check_file(path)?;
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        configure(&connection)?;
        validate_schema(&connection, project)?;
        Ok(Some(Self {
            connection,
            project: project.to_owned(),
        }))
    }

    pub(super) fn open_write(path: &Path, project: &Utf8Path) -> Result<Self, ReviewError> {
        let created = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(error) => return Err(error.into()),
        };
        check_file(path)?;
        let mut connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        configure(&connection)?;
        if created {
            connection.pragma_update(None, "page_size", 4096)?;
            connection.pragma_update(None, "journal_mode", "DELETE")?;
            connection.pragma_update(None, "synchronous", "FULL")?;
            connection.pragma_update(None, "max_page_count", 4096)?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for sql in [METADATA, CANDIDATES, EVENTS] {
                transaction.execute(sql, [])?;
            }
            transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
            transaction.pragma_update(None, "user_version", 2)?;
            transaction.execute(
                "INSERT INTO metadata(project) VALUES (?1)",
                [project.as_str()],
            )?;
            transaction.commit()?;
        } else {
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                let version: i64 =
                    connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
                if version != 0 || Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        validate_schema(&connection, project)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "max_page_count", 4096)?;
        Ok(Self {
            connection,
            project: project.to_owned(),
        })
    }

    pub(super) fn snapshot(&mut self) -> Result<Vec<(ReviewKey, ReviewState)>, ReviewError> {
        let transaction = self.connection.transaction()?;
        let result = read_snapshot(&transaction, &self.project)?;
        transaction.commit()?;
        Ok(result)
    }

    pub(super) fn append(
        &mut self,
        key: &ReviewKey,
        source: SourceState,
        expected_revision: u64,
        draft: ReviewDraft,
    ) -> Result<ReviewState, ReviewError> {
        self.append_inner(key, source, expected_revision, draft, false)
    }

    #[cfg(test)]
    pub(super) fn append_failing(
        &mut self,
        key: &ReviewKey,
        source: SourceState,
        expected_revision: u64,
        draft: ReviewDraft,
    ) -> Result<ReviewState, ReviewError> {
        self.append_inner(key, source, expected_revision, draft, true)
    }

    fn append_inner(
        &mut self,
        key: &ReviewKey,
        source: SourceState,
        expected_revision: u64,
        draft: ReviewDraft,
        fail_before_commit: bool,
    ) -> Result<ReviewState, ReviewError> {
        validate_draft(&draft)?;
        if key.project != self.project {
            return Err(invalid("store belongs to another project"));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = (|| {
            validate_schema(&transaction, &self.project)?;
            let snapshot = read_snapshot(&transaction, &self.project)?;
            let total: usize = snapshot.iter().map(|(_, state)| state.history.len()).sum();
            if total >= EVENT_LIMIT {
                return Err(invalid("review event limit reached"));
            }
            let current = snapshot
                .iter()
                .find(|(stored, _)| stored.candidate_id == key.candidate_id);
            let empty = ReviewState::default();
            let (stored_key, state) = current.map_or((key, &empty), |(key, state)| (key, state));
            let updated = apply_update(
                state,
                applicability(key, stored_key, source),
                expected_revision,
                draft,
            )?;
            let event = updated
                .history
                .last()
                .ok_or_else(|| invalid("missing review event"))?
                .clone();
            if current.is_none() {
                validate_key(key, &self.project)?;
                transaction.execute(
                    "INSERT INTO candidates(id, key_json) VALUES (?1, ?2)",
                    params![key.candidate_id, serde_json::to_string(key)?],
                )?;
            }
            transaction.execute(
                "INSERT INTO review_events(candidate_id, revision, draft_json) VALUES (?1, ?2, ?3)",
                params![
                    key.candidate_id,
                    event.revision.to_be_bytes().as_slice(),
                    serde_json::to_string(&event.draft)?
                ],
            )?;
            if fail_before_commit {
                return Err(invalid("injected pre-commit failure"));
            }
            Ok(updated)
        })();
        match result {
            Ok(event) => {
                transaction.commit()?;
                Ok(event)
            }
            Err(error) => {
                transaction.rollback()?;
                Err(error)
            }
        }
    }
}

fn check_file(path: &Path) -> Result<(), ReviewError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > DB_LIMIT {
        return Err(invalid(
            "review store must be a regular file of at most 16 MiB",
        ));
    }
    Ok(())
}

fn configure(connection: &Connection) -> Result<(), ReviewError> {
    connection.busy_timeout(Duration::from_secs(1))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "trusted_schema", "OFF")?;
    Ok(())
}

fn validate_schema(connection: &Connection, project: &Utf8Path) -> Result<(), ReviewError> {
    let app: i64 = connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if app != APPLICATION_ID || version != 2 {
        return Err(invalid("unrecognized review database or schema version"));
    }
    let mut statement =
        connection.prepare("SELECT type, name, tbl_name, sql FROM sqlite_schema ORDER BY name")?;
    let objects = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut tables = 0;
    for object in objects {
        let (kind, name, table, sql) = object?;
        let expected = match name.as_str() {
            "metadata" => Some(METADATA),
            "candidates" => Some(CANDIDATES),
            "review_events" => Some(EVENTS),
            _ => None,
        };
        if kind == "table" && expected.is_some() && sql.as_deref() == expected {
            tables += 1;
        } else if !(kind == "index"
            && sql.is_none()
            && name.starts_with("sqlite_autoindex_")
            && matches!(table.as_str(), "metadata" | "candidates" | "review_events"))
        {
            return Err(invalid("unexpected object in review database schema"));
        }
    }
    if tables != 3 {
        return Err(invalid("review schema is incomplete"));
    }
    let projects = connection
        .prepare("SELECT project FROM metadata")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if projects != [project.as_str()] {
        return Err(invalid("store belongs to another project"));
    }
    let mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if mode != "delete" {
        return Err(invalid("review store requires DELETE journal mode"));
    }
    let check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if check != "ok" {
        return Err(invalid("review database integrity check failed"));
    }
    let pages: i64 = connection.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let page_size: i64 = connection.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    if page_size != 4096 {
        return Err(invalid("review store requires 4096-byte pages"));
    }
    if pages.saturating_mul(page_size) > i64::try_from(DB_LIMIT).unwrap_or(i64::MAX) {
        return Err(invalid("review database exceeds 16 MiB"));
    }
    Ok(())
}

fn validate_key(key: &ReviewKey, project: &Utf8Path) -> Result<(), ReviewError> {
    let id = hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity::from(&key.descriptor));
    if key.project != project
        || id.as_str() != key.candidate_id
        || !hoimin_core::normalized_relative_path(key.descriptor.path.as_str())
        || key.descriptor.schema_version != 1
    {
        return Err(invalid("invalid stored candidate identity"));
    }
    Ok(())
}

fn read_snapshot(
    connection: &Connection,
    project: &Utf8Path,
) -> Result<Vec<(ReviewKey, ReviewState)>, ReviewError> {
    let count: i64 =
        connection.query_row("SELECT count(*) FROM review_events", [], |row| row.get(0))?;
    let count = usize::try_from(count).map_err(|_| invalid("invalid event count"))?;
    if count > EVENT_LIMIT {
        return Err(invalid("review event limit exceeded"));
    }
    let mut statement = connection.prepare("SELECT id, key_json FROM candidates ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut result = Vec::new();
    let mut events_seen = 0;
    for row in rows {
        let (id, json) = row?;
        let key: ReviewKey = serde_json::from_str(&json)?;
        validate_key(&key, project)?;
        if key.candidate_id != id {
            return Err(invalid("stored candidate identity conflict"));
        }
        let mut events = connection.prepare("SELECT revision, draft_json FROM review_events WHERE candidate_id=?1 ORDER BY revision")?;
        let rows = events.query_map([&id], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut history = Vec::new();
        for row in rows {
            let (revision, json) = row?;
            let revision = u64::from_be_bytes(
                revision
                    .try_into()
                    .map_err(|_| invalid("invalid stored revision"))?,
            );
            if revision
                != u64::try_from(history.len())
                    .unwrap_or(u64::MAX)
                    .saturating_add(1)
            {
                return Err(invalid("noncontiguous review history"));
            }
            let draft: ReviewDraft = serde_json::from_str(&json)?;
            validate_draft(&draft)?;
            history.push(ReviewEvent { revision, draft });
        }
        if history.is_empty() {
            return Err(invalid("stored candidate has no history"));
        }
        events_seen += history.len();
        result.push((key, ReviewState { history }));
    }
    if count != events_seen {
        return Err(invalid("orphan review events"));
    }
    Ok(result)
}
