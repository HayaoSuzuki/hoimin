use super::{ReviewDraft, ReviewKey, ReviewState, ReviewStore, draft, key};
use crate::review::ReviewError;
use hoimin_core::{CandidateIdentity, stable_mutant_id};
use rusqlite::{Connection, Statement, params, types::Value};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    id: String,
    candidates: Vec<Candidate>,
    orphans: u64,
    reads: Vec<Query>,
}

#[derive(serde::Deserialize)]
struct Candidate {
    slot: usize,
    key: Key,
    events: Events,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Key {
    Valid,
    Malformed,
    Foreign,
    Conflict,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Events {
    Consecutive(u64),
    Explicit(Vec<Event>),
}

#[derive(serde::Deserialize)]
struct Event {
    revision: Revision,
    draft: Draft,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(untagged)]
enum Revision {
    Blob(u64),
    Other(RevisionEncoding),
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum RevisionEncoding {
    Short,
    Text,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Draft {
    Valid,
    Malformed,
    Artifact,
    Reason,
    Evidence,
}

#[derive(serde::Deserialize)]
struct Query {
    selected: Option<usize>,
    expected: Expected,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Expected {
    Histories(Vec<History>),
    Rejection(String),
}

#[derive(serde::Deserialize)]
struct History {
    slot: usize,
    length: usize,
}

fn keys() -> [ReviewKey; 3] {
    let mut keys = ["first.py", "second.py", "absent.py"].map(|path| {
        let mut key = key();
        key.descriptor.path = path.into();
        key.candidate_id = stable_mutant_id(&CandidateIdentity::from(&key.descriptor)).to_string();
        key
    });
    keys[..2].sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    keys
}

fn key_json(shape: &Key, key: &ReviewKey, absent: &ReviewKey) -> String {
    match shape {
        Key::Valid => serde_json::to_string(key).unwrap(),
        Key::Malformed => "{".into(),
        Key::Foreign => serde_json::to_string(&ReviewKey {
            project: "/other-project".into(),
            ..key.clone()
        })
        .unwrap(),
        Key::Conflict => serde_json::to_string(absent).unwrap(),
    }
}

fn tagged_draft(slot: usize, revision: u64) -> ReviewDraft {
    let mut value = draft();
    value.reason = format!("candidate {slot}, revision {revision}");
    value.timestamp_ms = revision;
    value
}

fn insert_event(
    insert: &mut Statement<'_>,
    slot: usize,
    id: &str,
    revision: Revision,
    shape: Draft,
) {
    let (value, marker) = match revision {
        Revision::Blob(n) => (Value::Blob(n.to_be_bytes().to_vec()), n),
        Revision::Other(RevisionEncoding::Short) => (Value::Blob(vec![0]), 0),
        Revision::Other(RevisionEncoding::Text) => (Value::Text("00000001".into()), 0),
    };
    let json = if let Draft::Malformed = shape {
        "]".into()
    } else {
        let mut draft = tagged_draft(slot, marker);
        match shape {
            Draft::Artifact => draft.plan.content_hash.clear(),
            Draft::Reason => draft.reason.clear(),
            Draft::Evidence => draft.evidence.push("\0".into()),
            Draft::Valid | Draft::Malformed => {}
        }
        serde_json::to_string(&draft).unwrap()
    };
    insert.execute(params![id, value, json]).unwrap();
}

fn populate(path: &Path, case: &Case, keys: &[ReviewKey; 3]) {
    let mut db = Connection::open(path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON")
        .unwrap();
    let transaction = db.transaction().unwrap();
    {
        let mut insert = transaction
            .prepare("INSERT INTO review_events (candidate_id, revision, draft_json) VALUES (?1, ?2, ?3)")
            .unwrap();
        for row in case.candidates.iter().rev() {
            let key = &keys[row.slot];
            let id = &key.candidate_id;
            transaction
                .execute(
                    "INSERT INTO candidates (id, key_json) VALUES (?1, ?2)",
                    params![id, key_json(&row.key, key, &keys[2])],
                )
                .unwrap();
            match &row.events {
                Events::Consecutive(count) => {
                    for revision in (1..=*count).rev() {
                        insert_event(
                            &mut insert,
                            row.slot,
                            id,
                            Revision::Blob(revision),
                            Draft::Valid,
                        );
                    }
                }
                Events::Explicit(events) => {
                    for event in events.iter().rev() {
                        insert_event(&mut insert, row.slot, id, event.revision, event.draft);
                    }
                }
            }
        }
        for revision in 1..=case.orphans {
            insert_event(
                &mut insert,
                2,
                &keys[2].candidate_id,
                Revision::Blob(revision),
                Draft::Valid,
            );
        }
    }
    transaction.commit().unwrap();
    db.close().unwrap();
}

fn rejection(error: ReviewError, context: &str) -> &'static str {
    match error {
        ReviewError::Invalid(message) => match message.as_str() {
            "review event limit exceeded" => "event_limit",
            "invalid stored candidate identity" => "key_invalid",
            "stored candidate identity conflict" => "key_conflict",
            "stored candidate has no history" => "no_history",
            "invalid stored revision" => "revision_width",
            "noncontiguous review history" => "revision_order",
            "invalid review artifact reference" => "draft_artifact",
            "reason must be nonblank, safe text of at most 4096 UTF-8 bytes" => "draft_reason",
            "evidence permits at most 8 safe strings of 2048 UTF-8 bytes each" => "draft_evidence",
            "orphan review events" => "orphans",
            _ => panic!("{context}: unexpected validation error {message:?}"),
        },
        // The two local malformed fixtures have distinct JSON error categories.
        ReviewError::Json(error) if error.is_eof() => "key_json",
        ReviewError::Json(error) if error.is_syntax() => "draft_json",
        ReviewError::Database(rusqlite::Error::InvalidColumnType(
            2,
            column,
            rusqlite::types::Type::Text,
        )) if column == "revision" => "revision_type",
        error => panic!("{context}: unexpected error {error:?}"),
    }
}

fn verify_read(query: &Query, store: &mut ReviewStore, keys: &[ReviewKey; 3], context: &str) {
    let result = match query.selected {
        None => store.snapshot(),
        Some(slot) => store
            .candidate(&keys[slot].candidate_id)
            .map(|candidate| candidate.into_iter().collect()),
    };
    match (&query.expected, result) {
        (Expected::Rejection(expected), Err(error)) => {
            assert_eq!(rejection(error, context), expected, "{context}");
        }
        (Expected::Histories(expected), Ok(actual)) => {
            assert_histories(expected, &actual, keys, context);
        }
        (_, result) => panic!("{context}: expected other outcome, got {result:?}"),
    }
}

fn assert_histories(
    expected: &[History],
    actual: &[(ReviewKey, ReviewState)],
    keys: &[ReviewKey; 3],
    context: &str,
) {
    assert_eq!(actual.len(), expected.len(), "{context}");
    for (spec, (key, state)) in expected.iter().zip(actual) {
        assert_eq!(key, &keys[spec.slot], "{context}");
        assert_eq!(state.history.len(), spec.length, "{context}");
        for (revision, event) in (1..).zip(&state.history) {
            assert_eq!(event.revision, revision, "{context}");
            assert_eq!(event.draft, tagged_draft(spec.slot, revision), "{context}");
        }
    }
}

fn digest(path: &Path) -> blake3::Hash {
    blake3::Hasher::new()
        .update_reader(fs::File::open(path).unwrap())
        .unwrap()
        .finalize()
}

#[test]
fn review_store_snapshot_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/review-store-snapshot.jsonl"
    ));
    let keys = keys();
    let seed_dir = tempfile::tempdir().unwrap();
    let seed = seed_dir.path().join("seed.sqlite3");
    drop(ReviewStore::open_write(&seed, &keys[0].project).unwrap());
    let mut cases = BTreeSet::new();
    let mut reads = 0;
    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert!(cases.insert(case.id.clone()), "{}", case.id);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reviews.sqlite3");
        fs::copy(&seed, &path).unwrap();
        populate(&path, &case, &keys);
        let before = digest(&path);
        let mut store = ReviewStore::open_read(&path, &keys[0].project)
            .unwrap_or_else(|error| panic!("{}: fixture admission failed: {error}", case.id))
            .unwrap();
        for query in &case.reads {
            let context = format!("{}, selection {:?}", case.id, query.selected);
            verify_read(query, &mut store, &keys, &context);
            assert_eq!(digest(&path), before, "{context}: changed store bytes");
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "{context}");
            reads += 1;
        }
    }
    assert_eq!(cases.len(), 233);
    assert_eq!(reads, 932);
}
