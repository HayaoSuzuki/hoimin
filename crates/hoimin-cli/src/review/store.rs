use std::io::Read;
use std::path::Path;
use std::time::Duration;

use camino::Utf8Path;
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};

use super::ReviewError;
use super::model::{
    ReviewDraft, ReviewEvent, ReviewKey, ReviewState, SourceState, applicability,
    apply_update_owned, validate_draft,
};

const APPLICATION_ID: i64 = 0x484d_5256;
const DB_LIMIT: u64 = 16 * 1024 * 1024;
const EVENT_LIMIT: usize = 10_000;
const METADATA: &str = "CREATE TABLE metadata (project TEXT NOT NULL UNIQUE)";
const CANDIDATES: &str =
    "CREATE TABLE candidates (id TEXT PRIMARY KEY NOT NULL, key_json TEXT NOT NULL)";
const EVENTS: &str = "CREATE TABLE review_events (candidate_id TEXT NOT NULL REFERENCES candidates(id), revision BLOB NOT NULL CHECK(length(revision)=8), draft_json TEXT NOT NULL, PRIMARY KEY(candidate_id, revision))";

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct StoreWork {
    pub data_queries: usize,
    pub key_decodes: usize,
    pub draft_decodes: usize,
    pub cloned_events: usize,
}

#[cfg(test)]
thread_local! {
    pub(super) static STORE_WORK: std::cell::RefCell<StoreWork> = std::cell::RefCell::default();
}

fn data_query<'a>(
    connection: &'a Connection,
    sql: &str,
) -> rusqlite::Result<rusqlite::Statement<'a>> {
    #[cfg(test)]
    STORE_WORK.with_borrow_mut(|work| work.data_queries += 1);
    connection.prepare(sql)
}

struct ValidatedSnapshot {
    entries: Vec<(ReviewKey, ReviewState)>,
    event_count: usize,
}

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
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                initialize_store(path, project)?;
            }
            Err(error) => return Err(error.into()),
            Ok(_) => (),
        }
        check_file(path)?;
        // Reopen the public pathname even when our private initializer won:
        // future SQLite journals must belong to the published database.
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        configure(&connection)?;
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
        let result = read_snapshot(&transaction, &self.project, None)?;
        transaction.commit()?;
        Ok(result.entries)
    }

    pub(super) fn candidate(
        &mut self,
        candidate_id: &str,
    ) -> Result<Option<(ReviewKey, ReviewState)>, ReviewError> {
        let transaction = self.connection.transaction()?;
        let mut result = read_snapshot(&transaction, &self.project, Some(candidate_id))?;
        transaction.commit()?;
        Ok(result.entries.pop())
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
            let mut selected = read_snapshot(&transaction, &self.project, Some(&key.candidate_id))?;
            if selected.event_count >= EVENT_LIMIT {
                return Err(invalid("review event limit reached"));
            }
            let current = selected.entries.pop();
            let new_candidate = current.is_none();
            let (stored_key, state) =
                current.unwrap_or_else(|| (key.clone(), ReviewState::default()));
            let updated = apply_update_owned(
                state,
                applicability(key, &stored_key, source),
                expected_revision,
                draft,
            )?;
            let event = updated
                .history
                .last()
                .ok_or_else(|| invalid("missing review event"))?;
            if new_candidate {
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

// A private directory owns the database and every SQLite sidecar, including
// rollback journals during error/unwind. Forced termination can leave this
// directory behind; later initializers neither adopt nor remove those remnants.
fn initialize_store(path: &Path, project: &Utf8Path) -> Result<(), ReviewError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let directory = tempfile::Builder::new()
        .prefix(".hoimin-review-")
        .tempdir_in(parent)?;
    // Close tempfile's handle before SQLite opens the file (also on Windows).
    let temporary = tempfile::NamedTempFile::new_in(directory.path())?.into_temp_path();
    #[cfg(test)]
    super::store_tests::initialization_stage("created")?;
    let mut connection =
        Connection::open_with_flags(&temporary, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    configure(&connection)?;
    connection.pragma_update(None, "page_size", 4096)?;
    connection.pragma_update(None, "journal_mode", "DELETE")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "max_page_count", 4096)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for sql in [METADATA, CANDIDATES, EVENTS] {
        transaction.execute(sql, [])?;
        #[cfg(test)]
        super::store_tests::initialization_stage("transaction")?;
    }
    transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
    transaction.pragma_update(None, "user_version", 2)?;
    transaction.execute(
        "INSERT INTO metadata(project) VALUES (?1)",
        [project.as_str()],
    )?;
    transaction.commit()?;
    validate_schema(&connection, project)?;
    connection.close().map_err(|(_, error)| error)?;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&temporary)?
        .sync_all()?;
    #[cfg(test)]
    super::store_tests::initialization_stage("publish")?;
    // tempfile uses a no-replace rename or a hard-link fallback on Unix and
    // MoveFileExW without REPLACE_EXISTING on Windows. The fallback may leave
    // an old link, which is still owned by our private directory.
    match temporary.persist_noclobber(path) {
        Ok(()) => (),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.error.into()),
    }
    directory.close()?;
    Ok(())
}

fn check_file(path: &Path) -> Result<(), ReviewError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > DB_LIMIT {
        return Err(invalid(
            "review store must be a regular file of at most 16 MiB",
        ));
    }
    let file = crate::artifacts::open_regular_file(path)?
        .ok_or_else(|| invalid("review store must be a regular file of at most 16 MiB"))?;
    if file.metadata()?.len() > DB_LIMIT {
        return Err(invalid(
            "review store must be a regular file of at most 16 MiB",
        ));
    }
    let mut header = Vec::with_capacity(20);
    file.take(20).read_to_end(&mut header)?;
    // Review stores use DELETE mode. Even a read-only SQLite admission query
    // can create WAL/SHM files, so reject other journal formats before opening.
    if header.starts_with(b"SQLite format 3\0") && header.get(18..20) != Some(&[1, 1]) {
        return Err(invalid("review store requires DELETE journal mode"));
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
        || key.descriptor.schema_version != hoimin_core::CANDIDATE_SCHEMA_VERSION
    {
        return Err(invalid("invalid stored candidate identity"));
    }
    Ok(())
}

// Every operation validates the complete store in one ordered scan. Selection
// controls retention only: malformed unrelated candidates and orphan events must
// still reject a read or write. Drafts not requested by the caller are dropped
// immediately after validation rather than accumulating in a full snapshot.
fn read_snapshot(
    connection: &Connection,
    project: &Utf8Path,
    selected: Option<&str>,
) -> Result<ValidatedSnapshot, ReviewError> {
    let count: i64 = data_query(connection, "SELECT count(*) FROM review_events")?
        .query_row([], |row| row.get(0))?;
    let count = usize::try_from(count).map_err(|_| invalid("invalid event count"))?;
    if count > EVENT_LIMIT {
        return Err(invalid("review event limit exceeded"));
    }
    let mut statement = data_query(
        connection,
        "SELECT c.id, c.key_json, e.revision, e.draft_json FROM candidates c \
         LEFT JOIN review_events e ON e.candidate_id=c.id ORDER BY c.id, e.revision",
    )?;
    let mut rows = statement.query([])?;
    let mut result: Vec<(ReviewKey, ReviewState)> = Vec::new();
    let mut previous_id: Option<String> = None;
    let mut expected_revision = 1_u64;
    let mut retain = false;
    let mut events_seen = 0;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        if previous_id.as_deref() != Some(id.as_str()) {
            let json: String = row.get(1)?;
            #[cfg(test)]
            STORE_WORK.with_borrow_mut(|work| work.key_decodes += 1);
            let key: ReviewKey = serde_json::from_str(&json)?;
            validate_key(&key, project)?;
            if key.candidate_id != id {
                return Err(invalid("stored candidate identity conflict"));
            }
            retain = selected.is_none_or(|selected| selected == id);
            if retain {
                result.push((key, ReviewState::default()));
            }
            previous_id = Some(id);
            expected_revision = 1;
        }
        let revision: Option<Vec<u8>> = row.get(2)?;
        let revision = revision.ok_or_else(|| invalid("stored candidate has no history"))?;
        let revision = u64::from_be_bytes(
            revision
                .try_into()
                .map_err(|_| invalid("invalid stored revision"))?,
        );
        if revision != expected_revision {
            return Err(invalid("noncontiguous review history"));
        }
        expected_revision = expected_revision.saturating_add(1);
        let json: String = row.get(3)?;
        #[cfg(test)]
        STORE_WORK.with_borrow_mut(|work| work.draft_decodes += 1);
        let draft: ReviewDraft = serde_json::from_str(&json)?;
        validate_draft(&draft)?;
        events_seen += 1;
        if retain {
            let (_, state) = result
                .last_mut()
                .ok_or_else(|| invalid("missing review candidate"))?;
            state.history.push(ReviewEvent { revision, draft });
        }
    }
    if count != events_seen {
        return Err(invalid("orphan review events"));
    }
    Ok(ValidatedSnapshot {
        entries: result,
        event_count: count,
    })
}
