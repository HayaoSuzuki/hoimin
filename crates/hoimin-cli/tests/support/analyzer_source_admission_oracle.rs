use super::analysis_request;
use camino::Utf8PathBuf;
use hoimin_cli::analyzer::{AnalyzerHandler, CandidateCursor, CandidateStore, discover_targets};
use hoimin_core::{
    AnalysisDiagnostic, AnalyzeFile, EffectFailed, EffectId, MutationCandidate, MutationOperator,
    MutationOperatorSelection, MutationProfile,
};
use hoimin_python::{AnalyzerDiagnostic, AnalyzerDiagnosticCode};
use std::{collections::BTreeSet, fs, num::NonZeroUsize};

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    id: String,
    targets: Vec<Source>,
    expected: Expected,
}

#[derive(serde::Deserialize)]
struct Source {
    path: String,
    bytes: Option<Vec<u8>>,
}

#[derive(serde::Deserialize)]
struct Expected {
    accepted: usize,
    candidates: Vec<String>,
    invalid_syntax: Vec<String>,
    fatal: Option<Fatal>,
}

#[derive(serde::Deserialize)]
struct Fatal {
    path: String,
    code: String,
    message: String,
    prefix: bool,
}

impl Fatal {
    fn check(&self, error: &EffectFailed, id: EffectId, case: &str) {
        assert_eq!(error.id, id, "{case}");
        assert_eq!(error.failure.code(), self.code, "{case}");
        let message = error.failure.message();
        if self.prefix {
            assert!(message.starts_with(&self.message), "{case}: {message:?}");
        } else {
            assert_eq!(message, self.message, "{case}");
        }
    }
}

#[tokio::test]
async fn discovery_and_runtime_match_lean_on_analyzer_source_admission() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/analyzer-source-admission.jsonl"
    ));
    let operator = MutationOperator::BooleanLiteral.as_str();
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!([operator])).unwrap();
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        let id = &case.id;
        assert_eq!(case.schema, 1, "{id}");
        assert!(ids.insert(case.id.clone()), "{id}");
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).unwrap();
        for source in &case.targets {
            if let Some(bytes) = &source.bytes {
                fs::write(root.join(&source.path), bytes).unwrap();
            }
        }
        let requests: Vec<_> = case
            .targets
            .iter()
            .enumerate()
            .map(|(index, source)| {
                analysis_request(
                    100 + u64::try_from(index).unwrap(),
                    &source.path,
                    index + 1 == case.targets.len(),
                )
            })
            .collect();
        let targets: Vec<_> = requests
            .iter()
            .map(|request| request.target.clone())
            .collect();
        let planned =
            discover_targets(&root, &targets, &operators, MutationProfile::Full, 16).await;
        let planned_candidates = if let Some(fatal) = &case.expected.fatal {
            let Err(error) = planned else {
                panic!("{id}: discovery unexpectedly succeeded");
            };
            fatal.check(&error, EffectId(0), id);
            None
        } else {
            let result = planned.expect(id);
            let diagnostics: Vec<_> = case
                .expected
                .invalid_syntax
                .iter()
                .map(|path| AnalyzerDiagnostic {
                    code: AnalyzerDiagnosticCode::InvalidSyntax,
                    path: Some(path.as_str().into()),
                    line: None,
                    column: None,
                    message: None,
                })
                .collect();
            assert_eq!(result.diagnostics, diagnostics, "{id}");
            assert!(!result.truncated, "{id}");
            check_candidates(&result.candidates, &case.expected.candidates, id);
            Some(result.candidates)
        };

        let handler = AnalyzerHandler::new(
            root,
            operators.clone(),
            MutationProfile::Full,
            NonZeroUsize::new(16).unwrap(),
        );
        let replayed = check_runtime(handler, requests, &case.expected, id).await;
        assert_eq!(replayed, planned_candidates, "{id}");
    }
    assert_eq!(ids.len(), 214);
}

async fn check_runtime(
    mut handler: AnalyzerHandler,
    requests: Vec<AnalyzeFile>,
    expected: &Expected,
    id: &str,
) -> Option<Vec<MutationCandidate>> {
    let mut spool = None;
    for (index, request) in requests.into_iter().enumerate() {
        let effect_id = request.id;
        let final_target = request.final_target;
        let path = request.target.path.clone();
        let result = handler.handle(request).await;
        if index == expected.accepted {
            let fatal = expected.fatal.as_ref().expect(id);
            assert_eq!(path.as_str(), fatal.path, "{id}");
            fatal.check(&result.expect_err(id), effect_id, id);
            assert!(spool.is_none(), "{id}");
            return None;
        }
        let result = result.expect(id);
        let diagnostics: Vec<_> = expected
            .invalid_syntax
            .iter()
            .filter(|expected| expected.as_str() == path.as_str())
            .map(|expected| AnalysisDiagnostic {
                code: "analyzer.invalid_syntax".into(),
                message: format!("{expected}: source could not be parsed"),
            })
            .collect();
        assert_eq!(result.id, effect_id, "{id}");
        assert_eq!(result.diagnostics, diagnostics, "{id}");
        assert!(!result.truncated, "{id}");
        assert_eq!(result.spool.is_some(), final_target, "{id}");
        spool = result.spool;
    }
    assert!(expected.fatal.is_none(), "{id}");
    let spool = spool.expect(id);
    let mut replayed = Vec::new();
    let mut cursor = CandidateCursor::START;
    while let Some((candidate, next)) = CandidateStore::replay_one(&spool, cursor).expect(id) {
        replayed.push(candidate);
        cursor = next;
    }
    Some(replayed)
}

fn check_candidates(candidates: &[MutationCandidate], paths: &[String], id: &str) {
    assert_eq!(candidates.len(), paths.len(), "{id}");
    for (index, (candidate, path)) in candidates.iter().zip(paths).enumerate() {
        assert_eq!(candidate.path.as_str(), path, "{id}");
        assert_eq!(
            candidate.sequence,
            u64::try_from(index).unwrap() + 1,
            "{id}"
        );
        assert_eq!(
            candidate.operator,
            MutationOperator::BooleanLiteral.as_str(),
            "{id}"
        );
        assert_eq!(candidate.original, "True", "{id}");
        assert_eq!(candidate.replacement, "False", "{id}");
    }
}
