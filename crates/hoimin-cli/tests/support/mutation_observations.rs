use super::{analyze, descriptor, python_observe};
use camino::Utf8Path;
use hoimin_core::{MutationProfile, validate_candidate};

pub(super) async fn assert_change(
    source: &str,
    operator: &str,
    harness: &str,
    baseline: &str,
    expected: &str,
) {
    for newline in ["\n", "\r\n", "\r"] {
        let source = source.replace('\n', newline);
        let operators = serde_json::from_value(serde_json::json!([operator])).unwrap();
        let output = analyze(
            &hoimin_python::AnalyzeRequest {
                payload: hoimin_python::payload::PayloadBudget::default(),
                path: Utf8Path::new("subject.py"),
                lines: &[],
                symbols: &[],
                operators: &operators,
                profile: MutationProfile::Full,
                max_candidates: 10,
            },
            &source,
        );
        assert_eq!(output.diagnostics.len(), 0, "{source}");
        assert_eq!(output.candidates.len(), 1, "{operator}: {source}");
        let candidate = &output.candidates[0];
        validate_candidate(source.as_bytes(), &descriptor(&source, candidate)).unwrap();
        let mut mutant = source.as_bytes().to_vec();
        let start = usize::try_from(candidate.span.start).unwrap();
        let end = start + usize::try_from(candidate.span.length).unwrap();
        mutant.splice(start..end, candidate.replacement.bytes());
        let observations = python_observe(&[source.into_bytes(), mutant], harness)
            .await
            .unwrap();
        for observation in &observations {
            assert_eq!(observation["compiled"], true, "{operator}: {observation}");
            assert_eq!(
                observation["exception"],
                serde_json::Value::Null,
                "{operator}: {observation}"
            );
        }
        assert_eq!(observations[0]["stdout"], baseline, "{operator}");
        assert_eq!(observations[1]["stdout"], expected, "{operator}");
    }
}
