use super::{ReviewStore, closed_foreign_wal, draft, key};
use crate::review::ReviewError;
use camino::Utf8Path;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    mode: String,
    id: String,
    fixture: Fixture,
    size: u64,
    known_application: bool,
    known_version: bool,
    shape: Shape,
    project: ProjectRows,
    integrity: Integrity,
    page_size: u32,
    read_expected: Outcome,
    write_expected: Outcome,
}

#[derive(Clone, Copy, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Fixture {
    Sqlite,
    Missing,
    Directory,
    Symlink,
    Empty,
    Opaque,
    ClosedWal,
    TruncatedHeader,
    MixedReadVersion,
    MixedWriteVersion,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Shape {
    Standard,
    ExtraTable,
    ExtraIndex,
    Incomplete,
    ChangedSql,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProjectRows {
    Matching,
    Wrong,
    Empty,
    Multiple,
}

#[derive(Clone, Copy, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Integrity {
    Valid,
    UnusedPage,
    InvalidRevision,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Admitted,
    Absent,
    Created,
    FileKindOrSize,
    JournalMode,
    SqliteError,
    Identity,
    UnexpectedObject,
    IncompleteSchema,
    Project,
    Integrity,
    PageSize,
    PageBudget,
}

fn configure_fixture(path: &Path, seed: &Path, case: &Case) {
    fs::copy(seed, path).unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    if case.page_size != 4096 {
        db.pragma_update(None, "page_size", case.page_size).unwrap();
        db.execute_batch("VACUUM").unwrap();
        let size: u32 = db
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .unwrap();
        assert_eq!(size, case.page_size);
    }
    if case.integrity == Integrity::InvalidRevision {
        let key = key();
        db.execute(
            "INSERT INTO candidates (id, key_json) VALUES (?1, ?2)",
            rusqlite::params![key.candidate_id, serde_json::to_string(&key).unwrap()],
        )
        .unwrap();
        // Only the stored revision violates an existing CHECK constraint.
        db.execute_batch("PRAGMA ignore_check_constraints=ON")
            .unwrap();
        db.execute(
            "INSERT INTO review_events (candidate_id, revision, draft_json) VALUES (?1, X'00', ?2)",
            rusqlite::params![key.candidate_id, serde_json::to_string(&draft()).unwrap()],
        )
        .unwrap();
        db.execute_batch("PRAGMA ignore_check_constraints=OFF")
            .unwrap();
    }
    let schema = match case.shape {
        Shape::Standard => "",
        Shape::ExtraTable => "CREATE TABLE extra (id INTEGER)",
        Shape::ExtraIndex => "CREATE INDEX extra ON review_events (revision)",
        Shape::Incomplete => "DROP TABLE review_events",
        Shape::ChangedSql =>
            "DROP TABLE review_events;
             CREATE TABLE review_events (candidate_id TEXT NOT NULL REFERENCES candidates(id), revision BLOB NOT NULL, draft_json TEXT NOT NULL, PRIMARY KEY(candidate_id, revision))",
    };
    db.execute_batch(schema).unwrap();
    let projects = match case.project {
        ProjectRows::Matching => "",
        ProjectRows::Wrong => "UPDATE metadata SET project='/different-project'",
        ProjectRows::Empty => "DELETE FROM metadata",
        ProjectRows::Multiple => "INSERT INTO metadata (project) VALUES ('/different-project')",
    };
    db.execute_batch(projects).unwrap();
    if !case.known_application {
        db.pragma_update(None, "application_id", 0).unwrap();
    }
    if !case.known_version {
        db.pragma_update(None, "user_version", 1).unwrap();
    }
    db.close().unwrap();
    if case.integrity == Integrity::UnusedPage {
        append_unused_page(path, u64::from(case.page_size));
    }
}

fn append_unused_page(path: &Path, page_size: u64) {
    let mut file = OpenOptions::new().write(true).open(path).unwrap();
    let size = file.metadata().unwrap().len();
    assert_eq!(size % page_size, 0);
    let pages = u32::try_from(size / page_size + 1).unwrap();
    // Update the SQLite header page count without adding a reference to the new page.
    file.seek(SeekFrom::Start(28)).unwrap();
    file.write_all(&pages.to_be_bytes()).unwrap();
    file.set_len(size + page_size).unwrap();
}

#[cfg(unix)]
fn write_symlink(path: &Path) {
    let target = path.with_file_name("symlink-target");
    fs::write(&target, b"opaque input for a symlink").unwrap();
    std::os::unix::fs::symlink(target, path).unwrap();
}

#[cfg(not(unix))]
fn write_symlink(_path: &Path) {
    unreachable!("symbolic link fixture is limited to Unix");
}

fn prepare(path: &Path, seed: &Path, case: &Case) {
    match case.fixture {
        Fixture::Sqlite => configure_fixture(path, seed, case),
        Fixture::Missing => {}
        Fixture::Directory => fs::create_dir(path).unwrap(),
        Fixture::Symlink => write_symlink(path),
        Fixture::Empty => fs::write(path, []).unwrap(),
        Fixture::Opaque => fs::write(path, b"not a SQLite database").unwrap(),
        Fixture::ClosedWal => closed_foreign_wal(path),
        Fixture::TruncatedHeader | Fixture::MixedReadVersion | Fixture::MixedWriteVersion => {
            let format = match case.fixture {
                Fixture::TruncatedHeader => &[1][..],
                Fixture::MixedReadVersion => &[1, 2],
                Fixture::MixedWriteVersion => &[2, 1],
                _ => unreachable!(),
            };
            let mut file = File::create(path).unwrap();
            file.write_all(b"SQLite format 3\0\x10\0").unwrap();
            file.write_all(format).unwrap();
            file.set_len(case.size).unwrap();
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Entry {
    Directory,
    Link(PathBuf),
    File { size: u64, hash: blake3::Hash },
}

fn contents(root: &Path) -> BTreeMap<OsString, Entry> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            let observed = if kind.is_dir() {
                Entry::Directory
            } else if kind.is_symlink() {
                Entry::Link(fs::read_link(entry.path()).unwrap())
            } else {
                assert!(kind.is_file());
                let file = File::open(entry.path()).unwrap();
                let size = file.metadata().unwrap().len();
                let hash = blake3::Hasher::new()
                    .update_reader(file)
                    .unwrap()
                    .finalize();
                Entry::File { size, hash }
            };
            (entry.file_name(), observed)
        })
        .collect()
}

fn rejected(error: ReviewError, id: &str) -> Outcome {
    match error {
        ReviewError::Invalid(message) => match message.as_str() {
            "review store must be a regular file of at most 16 MiB" => Outcome::FileKindOrSize,
            "review store requires DELETE journal mode" => Outcome::JournalMode,
            "unrecognized review database or schema version" => Outcome::Identity,
            "unexpected object in review database schema" => Outcome::UnexpectedObject,
            "review schema is incomplete" => Outcome::IncompleteSchema,
            "store belongs to another project" => Outcome::Project,
            "review database integrity check failed" => Outcome::Integrity,
            "review store requires 4096-byte pages" => Outcome::PageSize,
            "review database exceeds 16 MiB" => Outcome::PageBudget,
            _ => panic!("{id}: unexpected invalid input {message:?}"),
        },
        ReviewError::Database(rusqlite::Error::SqliteFailure(failure, _))
            if failure.code == rusqlite::ErrorCode::NotADatabase =>
        {
            Outcome::SqliteError
        }
        error => panic!("{id}: unexpected error {error:?}"),
    }
}

fn verify_admission(case: &Case, seed: &Path, project: &Utf8Path) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let path = root.join("reviews.sqlite3");
    prepare(&path, seed, case);
    let before = contents(root);
    let id = &case.id;

    let read = match ReviewStore::open_read(&path, project) {
        Ok(Some(mut store)) if case.integrity == Integrity::InvalidRevision => {
            match store.candidate(&key().candidate_id) {
                Err(ReviewError::Invalid(message)) => {
                    assert_eq!(message, "invalid stored revision", "{id}");
                }
                _ => panic!("{id}: malformed revision reached the candidate history"),
            }
            Outcome::Admitted
        }
        Ok(Some(_)) => Outcome::Admitted,
        Ok(None) => Outcome::Absent,
        Err(error) => rejected(error, id),
    };
    assert_eq!(read, case.read_expected, "{id}: read admission");
    assert_eq!(contents(root), before, "{id}: read changed files");

    let existed = fs::symlink_metadata(&path).map_or_else(
        |error| {
            assert_eq!(error.kind(), ErrorKind::NotFound, "{id}");
            false
        },
        |_| true,
    );
    let write = match ReviewStore::open_write(&path, project) {
        Ok(_) if existed => Outcome::Admitted,
        Ok(_) => Outcome::Created,
        Err(error) => rejected(error, id),
    };
    assert_eq!(write, case.write_expected, "{id}: write admission");
    if write == Outcome::Created {
        assert_eq!(contents(root).len(), 1, "{id}");
        assert!(
            ReviewStore::open_read(&path, project).expect(id).is_some(),
            "{id}"
        );
    } else {
        assert_eq!(
            contents(root),
            before,
            "{id}: write admission changed files"
        );
    }
}

#[test]
fn existing_review_store_admission_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/review-store-admission.jsonl"
    ));
    let directory = tempfile::tempdir().unwrap();
    let seed = directory.path().join("valid.sqlite3");
    let project = key().project;
    drop(ReviewStore::open_write(&seed, &project).unwrap());
    let mut ids = BTreeSet::new();
    let mut model_only = 0;
    let mut tested = 0;
    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        if case.mode == "model-only" {
            model_only += 1;
            continue;
        }
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        if case.fixture == Fixture::Symlink && !cfg!(unix) {
            continue;
        }
        verify_admission(&case, &seed, &project);
        tested += 1;
    }
    assert_eq!(ids.len(), 106);
    assert_eq!(model_only, 2);
    assert_eq!(tested, if cfg!(unix) { 104 } else { 103 });
}
