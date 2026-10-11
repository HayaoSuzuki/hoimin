#[path = "support/proofreadtools_search/generator.rs"]
mod generator;

use generator::{generate_candidates, validate_sample};
use hoimin_core::validate_candidate;

const SOURCE: &str = "def _length_ratio_upper_bound(x: str, y: str) -> float:\n    return 2 * min(len(x), len(y)) / (len(x) + len(y))\n";

#[test]
fn generated_single_edits_are_valid_unique_and_reproducible() {
    let candidates = generate_candidates(SOURCE, 790, 32).unwrap();
    assert_eq!(candidates, generate_candidates(SOURCE, 790, 32).unwrap());
    assert_ne!(candidates.len(), 0);
    let mut ids = std::collections::BTreeSet::new();
    for candidate in &candidates {
        assert!(
            ids.insert(
                validate_candidate(SOURCE.as_bytes(), candidate)
                    .unwrap()
                    .to_string()
            )
        );
        let mut changed = SOURCE.to_owned();
        let start = usize::try_from(candidate.span.start).unwrap();
        changed.replace_range(
            start..start + usize::try_from(candidate.span.length).unwrap(),
            &candidate.replacement,
        );
        assert!(ruff_python_parser::parse_module(&changed).is_ok());
        assert_ne!(candidate.original, candidate.replacement);
    }
    assert!(
        candidates
            .iter()
            .any(|c| c.operator == "research.call_argument")
    );
    assert_eq!(generate_candidates(SOURCE, 790, 0).unwrap().len(), 0);
    assert!(generate_candidates("def broken(", 790, 32).is_err());
}

#[test]
fn unknown_sample_checkout_is_rejected() {
    let project = tempfile::tempdir().unwrap();
    assert!(validate_sample(camino::Utf8Path::from_path(project.path()).unwrap()).is_err());
}
