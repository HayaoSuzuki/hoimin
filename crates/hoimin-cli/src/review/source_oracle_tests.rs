use super::{ReviewError, SOURCE_WORK, SourceState, source_states};
use crate::source_file::{WorkspaceRaceHook, install_workspace_race_hook};
use camino::Utf8Path;
use hoimin_core::{
    ByteSpan, CandidateDescriptor, CandidateIdentity, MutationCandidate, normalized_relative_path,
    stable_mutant_id,
};
use std::{
    collections::BTreeSet,
    fs,
    sync::{Arc, Mutex},
};

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    mode: String,
    id: String,
    files: Vec<Source>,
    candidates: Vec<Input>,
    visits: Vec<String>,
    contexts: usize,
    first_error: Option<usize>,
    outcomes: Vec<Outcome>,
}

#[derive(serde::Deserialize)]
struct Source {
    path: String,
    bytes: Option<Vec<u8>>,
    start: u64,
    line: u32,
}

#[derive(serde::Deserialize)]
struct Input {
    path: String,
    hash_matches: bool,
    shape: Shape,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Shape {
    Valid,
    Mutation,
    Span,
    Original,
    Location,
    Identity,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Fresh,
    Stale,
    Unavailable,
    Invalid { message: String },
}

#[derive(Default)]
struct Visits(Mutex<Vec<String>>);

impl WorkspaceRaceHook for Visits {
    fn parent_opened(&self, operation: &'static str, path: &Utf8Path) {
        if operation == "read" {
            self.0.lock().unwrap().push(path.to_string());
        }
    }
}

fn make_candidate(source: &Source, input: &Input, sequence: u64) -> MutationCandidate {
    let hash_input = if input.hash_matches {
        source.bytes.as_deref().unwrap_or_default()
    } else {
        b"an earlier version"
    };
    let mut candidate = MutationCandidate {
        id: String::new(),
        sequence,
        path: input.path.as_str().into(),
        span: ByteSpan {
            start: source.start,
            length: 1,
        },
        original: "1".into(),
        replacement: "0".into(),
        operator: "integer_literal".into(),
        line: source.line,
        column: 0,
        symbol: None,
        file_hash: blake3::hash(hash_input).to_hex().to_string(),
    };
    match input.shape {
        Shape::Valid | Shape::Identity => {}
        Shape::Mutation => candidate.operator.clear(),
        Shape::Span => candidate.span.start = 200,
        Shape::Original => candidate.original = "9".into(),
        Shape::Location => candidate.line += 1,
    }
    candidate.id = if matches!(input.shape, Shape::Identity) {
        "incorrect-id".into()
    } else {
        stable_mutant_id(&CandidateIdentity::from(&CandidateDescriptor::from(
            &candidate,
        )))
        .to_string()
    };
    candidate
}

fn outcome(result: Result<SourceState, ReviewError>, case_id: &str) -> Outcome {
    match result {
        Ok(SourceState::Fresh) => Outcome::Fresh,
        Ok(SourceState::Stale) => Outcome::Stale,
        Ok(SourceState::Unavailable) => Outcome::Unavailable,
        Err(ReviewError::Invalid(message)) => Outcome::Invalid { message },
        Err(error) => panic!("{case_id}: unexpected review error {error:?}"),
    }
}

#[test]
fn grouped_review_source_checks_match_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/review-sources.jsonl"
    ));
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let case: Case = serde_json::from_str(row).unwrap();
        let id = &case.id;
        assert_eq!(case.schema, 1, "{id}");
        assert_eq!(case.mode, "internal-fixture", "{id}");
        assert!(ids.insert(id.clone()), "{id}");
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        for file in &case.files {
            if let Some(bytes) = &file.bytes {
                assert!(normalized_relative_path(&file.path), "{id}");
                fs::write(root.join(&file.path), bytes).unwrap();
            }
        }
        let candidates: Vec<_> = case
            .candidates
            .iter()
            .enumerate()
            .map(|(index, input)| {
                let file = case
                    .files
                    .iter()
                    .find(|file| file.path == input.path)
                    .expect(id);
                make_candidate(file, input, 1 + u64::try_from(index).unwrap())
            })
            .collect();
        let references: Vec<_> = candidates.iter().collect();
        let visits = Arc::new(Visits::default());
        let guard = install_workspace_race_hook(Arc::<Visits>::clone(&visits));
        SOURCE_WORK.with(|work| work.set((0, 0)));

        let states = source_states(root, &references);

        drop(guard);
        assert_eq!(*visits.0.lock().unwrap(), case.visits, "{id}");
        assert_eq!(
            SOURCE_WORK.with(std::cell::Cell::get),
            (case.contexts, case.contexts),
            "{id}"
        );
        assert_eq!(
            states.iter().position(Result::is_err),
            case.first_error,
            "{id}"
        );
        let actual: Vec<_> = states.into_iter().map(|state| outcome(state, id)).collect();
        assert_eq!(actual, case.outcomes, "{id}");
    }
    assert_eq!(ids.len(), 207);
}
