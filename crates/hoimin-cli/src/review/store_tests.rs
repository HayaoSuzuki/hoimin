use super::model::*;
use super::store::ReviewStore;
use super::tests::{draft, key};

type InitializationHook = Box<dyn Fn(&str) -> Result<(), super::ReviewError>>;

thread_local! {
    static INITIALIZATION_HOOK: std::cell::RefCell<Option<InitializationHook>> =
        std::cell::RefCell::new(None);
}

pub(super) fn initialization_stage(stage: &str) -> Result<(), super::ReviewError> {
    INITIALIZATION_HOOK.with_borrow(|hook| match hook {
        Some(hook) => hook(stage),
        None => Ok(()),
    })
}

struct InitializationHookGuard;

impl Drop for InitializationHookGuard {
    fn drop(&mut self) {
        INITIALIZATION_HOOK.with_borrow_mut(|hook| *hook = None);
    }
}

fn with_initialization_hook<T>(hook: InitializationHook, action: impl FnOnce() -> T) -> T {
    INITIALIZATION_HOOK.with_borrow_mut(|slot| {
        assert!(slot.is_none());
        *slot = Some(hook);
    });
    let _guard = InitializationHookGuard;
    action()
}

// Publishing the empty database first would leave a non-retryable public file.
// Exercise SQLite and its real journal at each failure boundary, including unwind.
fn failed_initialization_is_retryable(stage: &'static str, panic: bool) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_initialization_hook(
            Box::new(move |current| {
                if current == stage {
                    assert!(!panic, "injected initialization interruption");
                    return Err(super::ReviewError::Invalid(
                        "injected initialization failure".into(),
                    ));
                }
                Ok(())
            }),
            || ReviewStore::open_write(&path, &key().project),
        )
    }));
    if panic {
        assert!(outcome.is_err());
    } else {
        assert!(outcome.unwrap().is_err());
    }
    assert!(!path.exists(), "public store exists after {stage} failure");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "private database or journal leaked"
    );
    let key = key();
    let mut store = ReviewStore::open_write(&path, &key.project).unwrap();
    store.append(&key, SourceState::Fresh, 0, draft()).unwrap();
    drop(store);
    let mut reopened = ReviewStore::open_read(&path, &key.project)
        .unwrap()
        .unwrap();
    assert_eq!(reopened.snapshot().unwrap()[0].1.history.len(), 1);
}

#[test]
fn store_initialization_error_after_creation_is_retryable() {
    failed_initialization_is_retryable("created", false);
}

#[test]
fn store_initialization_error_during_transaction_is_retryable() {
    failed_initialization_is_retryable("transaction", false);
}

#[test]
fn store_initialization_error_before_publication_is_retryable() {
    failed_initialization_is_retryable("publish", false);
}

#[test]
fn store_initialization_panic_after_creation_is_retryable() {
    failed_initialization_is_retryable("created", true);
}

#[test]
fn store_initialization_panic_during_transaction_is_retryable() {
    failed_initialization_is_retryable("transaction", true);
}

#[test]
fn store_initialization_panic_before_publication_is_retryable() {
    failed_initialization_is_retryable("publish", true);
}

#[test]
fn store_initialization_never_exposes_uninitialized_public_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let observed = path.clone();
    let store = with_initialization_hook(
        Box::new(move |_| {
            assert!(!observed.exists());
            assert!(ReviewStore::open_read(&observed, &key().project)?.is_none());
            Ok(())
        }),
        || ReviewStore::open_write(&path, &key().project),
    )
    .unwrap();
    drop(store);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(
        ReviewStore::open_read(&path, &key().project)
            .unwrap()
            .is_some()
    );
}

#[test]
fn store_initialization_preserves_a_file_created_before_publication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let competitor = path.clone();
    let result = with_initialization_hook(
        Box::new(move |stage| {
            if stage == "publish" {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&competitor)
                    .unwrap();
                file.write_all(b"another initializer's unrelated file")
                    .unwrap();
            }
            Ok(())
        }),
        || ReviewStore::open_write(&path, &key().project),
    );

    assert!(result.is_err());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"another initializer's unrelated file"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn store_initialization_opens_the_valid_publication_winner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let competitor = path.clone();
    let mut store = with_initialization_hook(
        Box::new(move |stage| {
            if stage == "publish" {
                let competitor = competitor.clone();
                std::thread::spawn(move || {
                    let key = key();
                    let mut winner = ReviewStore::open_write(&competitor, &key.project).unwrap();
                    winner.append(&key, SourceState::Fresh, 0, draft()).unwrap();
                })
                .join()
                .unwrap();
            }
            Ok(())
        }),
        || ReviewStore::open_write(&path, &key().project),
    )
    .unwrap();

    let key = key();
    assert_eq!(store.snapshot().unwrap()[0].1.history.len(), 1);
    assert!(store.append(&key, SourceState::Fresh, 0, draft()).is_err());
    store.append(&key, SourceState::Fresh, 1, draft()).unwrap();
    drop(store);
    let mut reopened = ReviewStore::open_read(&path, &key.project)
        .unwrap()
        .unwrap();
    assert_eq!(reopened.snapshot().unwrap()[0].1.history.len(), 2);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn store_rejects_changed_page_size_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    drop(ReviewStore::open_write(&path, &key().project).unwrap());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA page_size=8192; VACUUM;")
        .unwrap();
    drop(connection);
    let bytes = std::fs::read(&path).unwrap();

    let read = ReviewStore::open_read(&path, &key().project);
    let write = ReviewStore::open_write(&path, &key().project);

    assert!(read.is_err());
    assert!(write.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn committed_state_is_pinned_before_another_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let key = key();
    let mut a = ReviewStore::open_write(&path, &key.project).unwrap();
    let mut b = ReviewStore::open_write(&path, &key.project).unwrap();
    let mut later = draft();
    later.assessment = Assessment::NotUseful;

    let saved = a.append(&key, SourceState::Fresh, 0, draft()).unwrap();
    b.append(&key, SourceState::Fresh, 1, later).unwrap();
    let current = a.snapshot().unwrap();

    assert_eq!(current[0].1.history.last().unwrap().revision, 2);
    assert_eq!(saved.history.last().unwrap().revision, 1);
    assert_eq!(saved.history.len(), 1);
    assert_eq!(saved.history[0].draft.assessment, Assessment::Actionable);
}

#[test]
fn reading_missing_store_does_not_create_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let key = key();

    let store = ReviewStore::open_read(&path, &key.project).unwrap();

    assert!(store.is_none());
    assert!(!path.exists());
}

#[test]
fn store_persists_sequential_revisions_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let key = key();
    let mut store = ReviewStore::open_write(&path, &key.project).unwrap();

    let first = store.append(&key, SourceState::Fresh, 0, draft()).unwrap();
    let conflict = store.append(&key, SourceState::Fresh, 0, draft());
    let second = store.append(&key, SourceState::Fresh, 1, draft()).unwrap();
    let stale = store.append(&key, SourceState::Stale, 2, draft());
    let before_close = store.snapshot().unwrap();
    drop(store);
    let mut store = ReviewStore::open_read(&path, &key.project)
        .unwrap()
        .unwrap();
    let reopened = store.snapshot().unwrap();

    assert_eq!(first.history.last().unwrap().revision, 1);
    assert!(conflict.is_err());
    assert_eq!(second.history.last().unwrap().revision, 2);
    assert!(stale.is_err());
    assert_eq!(before_close[0].1.history.len(), 2);
    assert_eq!(reopened, before_close);
}

#[test]
fn store_rejects_other_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    drop(ReviewStore::open_write(&path, &key().project).unwrap());
    let bytes = std::fs::read(&path).unwrap();

    let result = ReviewStore::open_write(&path, camino::Utf8Path::new("/other"));

    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn store_concurrent_initialization_and_updates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let initialized = std::sync::Arc::new((std::sync::Mutex::new(0), std::sync::Condvar::new()));

    let workers: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            let initialized = initialized.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let key = key();
                let mut store = with_initialization_hook(
                    Box::new(move |stage| {
                        if stage == "publish" {
                            let (count, ready) = &*initialized;
                            let mut count = count.lock().unwrap();
                            *count += 1;
                            ready.notify_all();
                            let (count, _) = ready
                                .wait_timeout_while(
                                    count,
                                    std::time::Duration::from_secs(5),
                                    |count| *count < 2,
                                )
                                .unwrap();
                            assert_eq!(*count, 2, "both databases must initialize privately");
                        }
                        Ok(())
                    }),
                    || ReviewStore::open_write(&path, &key.project),
                )
                .unwrap();
                store.append(&key, SourceState::Fresh, 0, draft()).is_ok()
            })
        })
        .collect();
    let successes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .filter(|success| *success)
        .count();
    let mut store = ReviewStore::open_write(&path, &key().project).unwrap();
    let saved = store.snapshot().unwrap();

    assert_eq!(successes, 1);
    assert_eq!(saved[0].1.history.len(), 1);
}

#[test]
fn store_rejects_empty_or_corrupt_database_without_changing_bytes() {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in [
        ("empty", b"".as_slice()),
        ("bad", b"not a database".as_slice()),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();

        let result = ReviewStore::open_write(&path, &key().project);

        assert!(result.is_err(), "{name}");
        assert_eq!(std::fs::read(path).unwrap(), contents);
    }
}

#[test]
fn store_rejects_foreign_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    drop(ReviewStore::open_write(&path, &key().project).unwrap());
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE VIEW hostile AS SELECT 1").unwrap();
    drop(db);
    let bytes = std::fs::read(&path).unwrap();

    let result = ReviewStore::open_write(&path, &key().project);

    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn store_rejects_oversized_database() {
    let dir = tempfile::tempdir().unwrap();
    let oversized = dir.path().join("oversized");
    std::fs::File::create(&oversized)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();

    let result = ReviewStore::open_write(&oversized, &key().project);

    assert!(result.is_err());
}

#[test]
fn store_rolls_back_failed_append() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let key = key();
    let mut store = ReviewStore::open_write(&path, &key.project).unwrap();

    let result = store.append_failing(&key, SourceState::Fresh, 0, draft());

    assert!(result.is_err());
    assert_eq!(store.snapshot().unwrap(), Vec::new());
}

#[test]
fn store_busy_rejection_leaves_no_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let key = key();
    let mut store = ReviewStore::open_write(&path, &key.project).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();

    let result = store.append(&key, SourceState::Fresh, 0, draft());
    db.execute_batch("ROLLBACK").unwrap();

    assert!(result.is_err());
    assert_eq!(store.snapshot().unwrap(), Vec::new());
}

#[test]
fn store_event_limit_preserves_all_existing_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let key = key();
    let mut store = ReviewStore::open_write(&path, &key.project).unwrap();
    store.append(&key, SourceState::Fresh, 0, draft()).unwrap();
    let mut db = rusqlite::Connection::open(&path).unwrap();
    let transaction = db.transaction().unwrap();
    let json = serde_json::to_string(&draft()).unwrap();
    for revision in 2_u64..=10_000 {
        transaction
            .execute(
                "INSERT INTO review_events(candidate_id, revision, draft_json) VALUES (?1, ?2, ?3)",
                rusqlite::params![key.candidate_id, revision.to_be_bytes().as_slice(), json],
            )
            .unwrap();
    }
    transaction.commit().unwrap();

    let result = store.append(&key, SourceState::Fresh, 10_000, draft());

    assert!(result.is_err());
    assert_eq!(store.snapshot().unwrap()[0].1.history.len(), 10_000);
}

#[test]
fn store_accepts_independent_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let first = key();
    let mut second = first.clone();
    second.descriptor.replacement = "3".into();
    second.candidate_id =
        hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity::from(&second.descriptor))
            .to_string();
    let mut a = ReviewStore::open_write(&path, &first.project).unwrap();
    let mut b = ReviewStore::open_write(&path, &first.project).unwrap();

    a.append(&first, SourceState::Fresh, 0, draft()).unwrap();
    b.append(&second, SourceState::Fresh, 0, draft()).unwrap();
    let saved = a.snapshot().unwrap();

    assert_eq!(saved.len(), 2);
}

#[test]
fn store_rejects_unknown_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    drop(ReviewStore::open_write(&path, &key().project).unwrap());
    let db = rusqlite::Connection::open(&path).unwrap();
    db.pragma_update(None, "user_version", 99).unwrap();
    drop(db);
    let bytes = std::fs::read(&path).unwrap();

    let result = ReviewStore::open_write(&path, &key().project);

    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn reading_version_one_store_is_rejected_without_changes() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    drop(ReviewStore::open_write(&path, &key().project).unwrap());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    drop(connection);
    let before = std::fs::read(&path).unwrap();

    // Act
    let result = ReviewStore::open_read(&path, &key().project);

    // Assert
    assert!(result.is_err());
    // Assert
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn writing_version_one_store_is_rejected_without_changes() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    drop(ReviewStore::open_write(&path, &key().project).unwrap());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    drop(connection);
    let before = std::fs::read(&path).unwrap();

    // Act
    let result = ReviewStore::open_write(&path, &key().project);

    // Assert
    assert!(result.is_err());
    // Assert
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn reopening_store_preserves_report_reference_history() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let key = key();
    let mut value = draft();
    value.report = Some(
        serde_json::from_value(serde_json::json!({
            "artifact": {"path": "report.json", "content_hash": "b".repeat(64)},
            "run_id": "run-1"
        }))
        .unwrap(),
    );
    let mut store = ReviewStore::open_write(&path, &key.project).unwrap();
    store
        .append(&key, SourceState::Fresh, 0, value.clone())
        .unwrap();
    drop(store);

    // Act
    let mut reopened = ReviewStore::open_read(&path, &key.project)
        .unwrap()
        .unwrap();
    let snapshot = reopened.snapshot().unwrap();

    // Assert
    assert_eq!(snapshot[0].1.history[0].draft, value);
}
