#[path = "support/proofreadtools_search/django.rs"]
mod django;
#[path = "support/proofreadtools_search/generator.rs"]
mod generator;
#[path = "support/proofreadtools_search/runner.rs"]
mod runner;

use generator::{generate_candidates, validate_sample};
use hoimin_core::validate_candidate;

const SOURCE: &str = "def _length_ratio_upper_bound(x: str, y: str) -> float:\n    return 2 * min(len(x), len(y)) / (len(x) + len(y))\n";

#[test]
fn generated_edits_are_confined_to_the_selected_django_function() {
    let source = "def untouched(x):\n    return x + 1\ndef selected(x):\n    return x * 2\n";
    let candidates =
        generator::candidate_space_for(source, "django/utils/demo.py", "selected").unwrap();
    assert_ne!(candidates.len(), 0);
    for candidate in candidates {
        assert_eq!(candidate.path, "django/utils/demo.py");
        assert_eq!(candidate.symbol.as_deref(), Some("selected"));
        assert!(candidate.line >= 4);
        validate_candidate(source.as_bytes(), &candidate).unwrap();
    }
    assert!(generator::candidate_space_for(source, "demo.py", "missing").is_err());
}

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

#[test]
fn candidate_shrinking_starts_at_the_real_edit_and_reduces_structure() {
    use proptest::strategy::ValueTree;
    let candidate = generate_candidates(SOURCE, 776, 32).unwrap().remove(0);
    let (space, mut tree) = generator::shrink_tree(SOURCE, &candidate).unwrap();
    assert_eq!(space[tree.current()], candidate);
    let initial = tree.current();
    let mut previous = initial;
    while tree.simplify() {
        assert!(tree.current() < previous);
        validate_candidate(SOURCE.as_bytes(), &space[tree.current()]).unwrap();
        previous = tree.current();
    }
    assert_eq!(tree.current(), 0);
}

#[test]
fn real_workspace_application_and_reset_preserve_original() {
    runner::exercise_fixture(SOURCE);
}

#[test]
fn incomplete_test_process_stops_before_witness_search() {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(runner::exercise_timeout(SOURCE));
}

#[test]
fn final_replay_prefers_retained_shrink_and_allows_negative_results() {
    use serde_json::json;
    assert!(runner::comparison_candidate(&[json!({"rows":[]})]).is_none());
    let rows = vec![
        json!({"rows":[{"id":"original","status":"survived_related_suite","witness":{"status":"witness"}}],
            "shrink_rows":[{"kept":true,"row":{"id":"smaller","status":"survived_related_suite","witness":{"status":"witness"}}}]
        }),
    ];
    assert_eq!(
        runner::comparison_candidate(&rows).unwrap()["id"],
        "smaller"
    );
}

#[test]
fn process_errors_and_incomplete_output_are_not_mutation_kills() {
    use hoimin_core::{
        EffectId, OutputSpoolRef, ProcessFinished, ProcessOutputState, ProcessTermination,
        ResourceMode,
    };
    let mut result = ProcessFinished {
        id: EffectId(1),
        worker: Some(0),
        termination: ProcessTermination::Exit(1),
        output_state: ProcessOutputState::Complete,
        output: OutputSpoolRef {
            token: "fixture".into(),
            retained: 10,
            observed: 10,
        },
        elapsed: std::time::Duration::ZERO,
        resource_mode: ResourceMode::Hard,
    };
    assert_eq!(
        runner::verdict(&result, "AssertionError\n1 failed"),
        "detected_assertion"
    );
    result.output.observed = 11;
    assert_eq!(
        runner::verdict(&result, "AssertionError\n1 failed"),
        "incomplete"
    );
    result.output.observed = 10;
    result.output_state = ProcessOutputState::CloseTimedOut;
    assert_eq!(
        runner::verdict(&result, "AssertionError\n1 failed"),
        "incomplete"
    );
    result.output_state = ProcessOutputState::Complete;
    assert_eq!(
        runner::verdict(&result, "ImportError AssertionError"),
        "test_failure_other"
    );
    result.termination = ProcessTermination::Timeout;
    assert_eq!(runner::verdict(&result, "AssertionError"), "incomplete");
    result.termination = ProcessTermination::Exit(0);
    assert_ne!(
        runner::verdict(&result, "1 skipped"),
        "survived_related_suite"
    );
}

#[test]
#[ignore = "explicit bounded research against the pinned local proofreadtools checkout"]
fn proofreadtools_experiment() {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(runner::experiment())
        .unwrap();
}

#[test]
#[ignore = "explicit bounded research against the pinned local Django checkout"]
fn django_experiment() {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(django::experiment())
        .unwrap();
}

#[test]
fn django_suite_verdict_requires_the_same_test_count_and_distinguishes_errors() {
    assert_eq!(
        django::suite_status(true, "Ran 20 tests\nOK (skipped=2)", 20, 2),
        "survived_related_suite"
    );
    assert_eq!(
        django::suite_status(true, "Ran 1 tests\nOK", 20, 2),
        "invalid_suite"
    );
    assert_eq!(
        django::suite_status(true, "Ran 20 tests\nOK (skipped=3)", 20, 2),
        "invalid_suite"
    );
    assert_eq!(
        django::suite_status(
            false,
            "Ran 20 tests\nAssertionError\nFAILED (failures=1, skipped=2)",
            20,
            2
        ),
        "detected_assertion"
    );
    assert_eq!(
        django::suite_status(
            false,
            "Ran 20 tests\nAssertionError\nFAILED (failures=1, errors=1, skipped=2)",
            20,
            2
        ),
        "test_failure_other"
    );
}
