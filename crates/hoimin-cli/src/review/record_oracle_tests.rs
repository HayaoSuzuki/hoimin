use super::{draft, plan_fixture};
use crate::{
    cli::{ReviewArgs, ReviewCommonArgs, ReviewRecordArgs, ReviewShowArgs},
    plan::ReadOnlyPlan,
    review::{
        ReviewError, input,
        model::{
            ArtifactReference, ReviewDraft, ReviewEvent, ReviewKey, SourceState, UpdateRejection,
        },
        run,
        store::ReviewStore,
    },
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Write},
    path::Path,
};

const OUTPUT_ERROR: &str = "injected review output failure";

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    mode: String,
    id: String,
    requests: Vec<Request>,
    observations: Vec<Observation>,
}

#[derive(serde::Deserialize)]
struct Request {
    candidate: Candidate,
    expected_revision: u64,
    marker: u64,
    mode: Mode,
    source: Source,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Candidate {
    First,
    Second,
}

impl Candidate {
    fn index(self) -> usize {
        match self {
            Self::First => 0,
            Self::Second => 1,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Record,
    Rollback,
    Busy,
    FailWrite,
    FailNewline,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Debug, Default, Eq, PartialEq, serde::Deserialize)]
struct Store {
    present: bool,
    first: Vec<Event>,
    second: Vec<Event>,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
struct Event {
    revision: u64,
    marker: u64,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Reply {
    Recorded { revision: u64 },
    SavedOutput { revision: u64 },
    NotCurrent,
    RevisionConflict,
    RolledBack,
    Busy,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Output {
    Empty,
    Document,
    Line,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
struct Observation {
    store: Store,
    reply: Reply,
    output: Output,
}

fn events(history: &[ReviewEvent]) -> Vec<Event> {
    history
        .iter()
        .map(|event| Event {
            revision: event.revision,
            marker: event
                .draft
                .reason
                .strip_prefix("draft-")
                .unwrap()
                .parse()
                .unwrap(),
        })
        .collect()
}

fn prepare_source(plan: &ReadOnlyPlan, source: &Source) {
    let path = plan.project.join("sample.py");
    fs::write(&path, b"value = 1\n").unwrap();
    match source {
        Source::Fresh => {}
        Source::Stale => fs::write(&path, b"value = 9\n").unwrap(),
        Source::Unavailable => fs::remove_file(path).unwrap(),
    }
}

fn reopen(path: &Path, plan: &ReadOnlyPlan, keys: &[ReviewKey; 2]) -> Store {
    let Some(mut store) = ReviewStore::open_read(path, &plan.project).unwrap() else {
        return Store::default();
    };
    let mut histories = Store {
        present: true,
        ..Store::default()
    };
    for (key, state) in store.snapshot().unwrap() {
        assert_ne!(state.history.as_slice(), &[] as &[ReviewEvent]);
        if key == keys[0] {
            histories.first = events(&state.history);
        } else {
            assert_eq!(key, keys[1]);
            histories.second = events(&state.history);
        }
    }
    histories
}

fn attempt_draft(plan: &ReadOnlyPlan, marker: u64) -> ReviewDraft {
    let mut event = draft();
    event.reason = format!("draft-{marker}");
    event.plan = ArtifactReference {
        path: plan.canonical_path.clone(),
        content_hash: plan.content_hash.clone(),
    };
    event
}

fn injected_attempt(
    plan: &ReadOnlyPlan,
    path: &Path,
    key: &ReviewKey,
    request: &Request,
    id: &str,
) -> Reply {
    assert!(matches!(request.source, Source::Fresh));
    let mut store = ReviewStore::open_write(path, &plan.project).expect(id);
    let review = attempt_draft(plan, request.marker);
    let result = if request.mode == Mode::Busy {
        let mut lock_connection = rusqlite::Connection::open(path).expect(id);
        let lock = lock_connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .expect(id);
        let result = store.append(key, SourceState::Fresh, request.expected_revision, review);
        lock.rollback().expect(id);
        result
    } else {
        store.append_failing(key, SourceState::Fresh, request.expected_revision, review)
    };
    rejection(result.expect_err(id), id)
}

struct Writer {
    mode: Mode,
    bytes: Vec<u8>,
}

impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.mode == Mode::FailWrite || (self.mode == Mode::FailNewline && bytes == b"\n") {
            return Err(io::Error::other(OUTPUT_ERROR));
        }
        self.bytes.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn rejection(error: ReviewError, id: &str) -> Reply {
    match error {
        ReviewError::Update(UpdateRejection::NotCurrent) => Reply::NotCurrent,
        ReviewError::Update(UpdateRejection::RevisionConflict) => Reply::RevisionConflict,
        ReviewError::Invalid(message) if message == "injected pre-commit failure" => {
            Reply::RolledBack
        }
        ReviewError::Database(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            Reply::Busy
        }
        ReviewError::SavedOutput { revision, message } => {
            assert!(message.contains(OUTPUT_ERROR), "{id}: {message}");
            Reply::SavedOutput { revision }
        }
        error => panic!("{id}: unexpected review error {error:?}"),
    }
}

fn common(plan: &ReadOnlyPlan, path: &Path) -> ReviewCommonArgs {
    ReviewCommonArgs {
        plan: plan.canonical_path.clone(),
        store: path.to_owned(),
        report: None,
    }
}

fn displayed_history(output: &Value, key: &ReviewKey, id: &str) -> Vec<Event> {
    let rows = output["candidates"].as_array().expect(id);
    assert_eq!(rows.len(), 1, "{id}");
    assert_eq!(rows[0]["candidate"]["id"], key.candidate_id, "{id}");
    let history: Vec<ReviewEvent> = serde_json::from_value(rows[0]["history"].clone()).expect(id);
    if let Some(latest) = history.last() {
        assert_eq!(
            serde_json::to_value(latest).unwrap(),
            rows[0]["review"],
            "{id}"
        );
    }
    events(&history)
}

fn command_attempt(
    plan: &ReadOnlyPlan,
    path: &Path,
    key: &ReviewKey,
    request: &Request,
    id: &str,
) -> (Reply, Output) {
    let draft = attempt_draft(plan, request.marker);
    let args = ReviewArgs::Record(ReviewRecordArgs {
        common: common(plan, path),
        candidate: key.candidate_id.clone(),
        assessment: draft.assessment,
        reason: draft.reason,
        evidence: draft.evidence,
        expected_revision: request.expected_revision,
    });
    let mut writer = Writer {
        mode: request.mode,
        bytes: Vec::new(),
    };
    let result = run(&args, &mut writer);
    let (output, document) = if writer.bytes.is_empty() {
        (Output::Empty, None)
    } else {
        let output = if writer.bytes.ends_with(b"\n") {
            Output::Line
        } else {
            Output::Document
        };
        let document: Value = serde_json::from_slice(&writer.bytes).expect(id);
        assert_eq!(document["command"], "record", "{id}");
        (output, Some(document))
    };
    let reply = match result {
        Ok(code) => {
            assert_eq!(code, 0, "{id}");
            let history = displayed_history(document.as_ref().expect(id), key, id);
            let saved = history.last().expect(id);
            assert_eq!(saved.marker, request.marker, "{id}");
            Reply::Recorded {
                revision: saved.revision,
            }
        }
        Err(error) => rejection(error, id),
    };
    if let (Reply::SavedOutput { revision }, Some(document)) = (&reply, document.as_ref()) {
        let history = displayed_history(document, key, id);
        assert_eq!(
            history.last(),
            Some(&Event {
                revision: *revision,
                marker: request.marker
            }),
            "{id}"
        );
    }
    (reply, output)
}

fn check_show(plan: &ReadOnlyPlan, path: &Path, key: &ReviewKey, expected: &[Event], id: &str) {
    let args = ReviewArgs::Show(ReviewShowArgs {
        common: common(plan, path),
        candidate: key.candidate_id.clone(),
    });
    let mut stdout = Vec::new();
    assert_eq!(run(&args, &mut stdout).expect(id), 0, "{id}");
    let document: Value = serde_json::from_slice(&stdout).expect(id);
    assert_eq!(document["command"], "show", "{id}");
    assert_eq!(displayed_history(&document, key, id), expected, "{id}");
}

#[tokio::test]
async fn review_record_retries_match_lean_after_reopen() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/review-record.jsonl"
    ));
    let (_project, plan_path) = plan_fixture().await;
    let plan = input::load_plan(&plan_path).unwrap();
    let keys = [0, 1]
        .map(|index| input::review_key(&plan.project, &plan.manifest.candidates[index].candidate));
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let case: Case = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        assert_eq!(case.requests.len(), case.observations.len(), "{}", case.id);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reviews.sqlite3");
        for (index, (request, expected)) in case.requests.iter().zip(case.observations).enumerate()
        {
            let id = &format!("{} step {index}", case.id);
            prepare_source(&plan, &request.source);
            let key = &keys[request.candidate.index()];

            let (reply, output) = match request.mode {
                Mode::Rollback | Mode::Busy => (
                    injected_attempt(&plan, &path, key, request, id),
                    Output::Empty,
                ),
                _ => command_attempt(&plan, &path, key, request, id),
            };
            let actual = Observation {
                store: reopen(&path, &plan, &keys),
                reply,
                output,
            };

            assert_eq!(actual, expected, "{id}");
            if matches!(actual.reply, Reply::SavedOutput { .. }) {
                let history = match request.candidate {
                    Candidate::First => &expected.store.first,
                    Candidate::Second => &expected.store.second,
                };
                check_show(&plan, &path, key, history, id);
            }
        }
    }
    assert_eq!(ids.len(), 108);
}
