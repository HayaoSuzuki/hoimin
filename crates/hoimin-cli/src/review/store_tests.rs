use super::model::*;
use super::store::ReviewStore;
use super::tests::{draft, key};

#[test]
fn store_history_and_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let key = key();
    assert!(
        ReviewStore::open_read(&path, &key.project)
            .unwrap()
            .is_none()
    );
    assert!(!path.exists());
    let mut store = ReviewStore::open_write(&path, &key.project).unwrap();
    assert_eq!(
        store
            .append(&key, SourceState::Fresh, 0, draft())
            .unwrap()
            .revision,
        1
    );
    assert!(store.append(&key, SourceState::Fresh, 0, draft()).is_err());
    assert_eq!(
        store
            .append(&key, SourceState::Fresh, 1, draft())
            .unwrap()
            .revision,
        2
    );
    assert!(store.append(&key, SourceState::Stale, 2, draft()).is_err());
    assert_eq!(store.snapshot().unwrap()[0].1.history.len(), 2);
    drop(store);
    let mut store = ReviewStore::open_read(&path, &key.project)
        .unwrap()
        .unwrap();
    assert_eq!(store.snapshot().unwrap()[0].1.history.len(), 2);
    assert!(ReviewStore::open_write(&path, camino::Utf8Path::new("/other")).is_err());
}

#[test]
fn store_concurrent_initialization_and_updates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let key = key();
                let mut store = ReviewStore::open_write(&path, &key.project).unwrap();
                store.append(&key, SourceState::Fresh, 0, draft()).is_ok()
            })
        })
        .collect();
    let successes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .filter(|success| *success)
        .count();
    assert_eq!(successes, 1);
    let mut store = ReviewStore::open_write(&path, &key().project).unwrap();
    assert_eq!(store.snapshot().unwrap()[0].1.history.len(), 1);
}

#[test]
fn store_rejects_foreign_corrupt_and_oversized() {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in [
        ("empty", b"".as_slice()),
        ("bad", b"not a database".as_slice()),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        assert!(ReviewStore::open_write(&path, &key().project).is_err());
        assert_eq!(std::fs::read(path).unwrap(), contents);
    }
    let path = dir.path().join("reviews.sqlite3");
    drop(ReviewStore::open_write(&path, &key().project).unwrap());
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE VIEW hostile AS SELECT 1").unwrap();
    drop(db);
    assert!(ReviewStore::open_write(&path, &key().project).is_err());
    let oversized = dir.path().join("oversized");
    std::fs::File::create(&oversized)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(ReviewStore::open_write(&oversized, &key().project).is_err());
}

#[test]
fn store_rollback_and_busy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.sqlite3");
    let key = key();
    let mut store = ReviewStore::open_write(&path, &key.project).unwrap();
    assert!(
        store
            .append_failing(&key, SourceState::Fresh, 0, draft())
            .is_err()
    );
    assert!(store.snapshot().unwrap().is_empty());
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(store.append(&key, SourceState::Fresh, 0, draft()).is_err());
    db.execute_batch("ROLLBACK").unwrap();
    assert!(store.snapshot().unwrap().is_empty());
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
    assert!(
        store
            .append(&key, SourceState::Fresh, 10_000, draft())
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap()[0].1.history.len(), 10_000);
}

#[test]
fn store_accepts_independent_candidates_and_rejects_unknown_version() {
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
    assert_eq!(a.snapshot().unwrap().len(), 2);
    drop((a, b));
    let db = rusqlite::Connection::open(&path).unwrap();
    db.pragma_update(None, "user_version", 99).unwrap();
    drop(db);
    assert!(ReviewStore::open_write(&path, &first.project).is_err());
}
