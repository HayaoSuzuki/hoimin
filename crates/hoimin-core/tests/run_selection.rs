use hoimin_core::{
    RunSelection, RunSelectionError, VerificationSampling, VerificationSelection,
    VerificationTopPolicy,
};

fn metadata(mode: &str, selected: usize) -> VerificationSelection {
    match mode {
        "explicit" => VerificationSelection::candidate_ids(3, selected, false).unwrap(),
        "top" => {
            VerificationSelection::top(3, selected, VerificationTopPolicy::Strict, false).unwrap()
        }
        "sample" => VerificationSelection::sample(
            3,
            VerificationSampling::new(10, 42, vec!["b".into(), "a".into()]).unwrap(),
        )
        .unwrap(),
        _ => unreachable!(),
    }
}

macro_rules! rejects_mode {
    ($name:ident, $constructor:ident, $mode:literal) => {
        #[test]
        fn $name() {
            // Arrange: metadata is valid alone but describes a different dispatch mode.
            let provenance = metadata($mode, 2);
            // Act.
            let result = RunSelection::$constructor(vec!["b".into(), "a".into()], provenance);
            // Assert.
            assert_eq!(result.unwrap_err(), RunSelectionError::ModeMismatch);
        }
    };
}
rejects_mode!(explicit_rejects_top, explicit, "top");
rejects_mode!(explicit_rejects_sample, explicit, "sample");
rejects_mode!(ordered_rejects_explicit, ordered, "explicit");

macro_rules! rejects_count {
    ($name:ident, $constructor:ident, $mode:literal) => {
        #[test]
        fn $name() {
            // Arrange: two input entries represent only one unique selected ID.
            let provenance = metadata($mode, 2);
            // Act.
            let result = RunSelection::$constructor(vec!["a".into(), "a".into()], provenance);
            // Assert.
            assert_eq!(
                result.unwrap_err(),
                RunSelectionError::SelectedCountMismatch
            );
        }
    };
}
rejects_count!(explicit_checks_deduplicated_count, explicit, "explicit");
rejects_count!(ordered_checks_deduplicated_count, ordered, "top");

macro_rules! rejects_sample {
    ($name:ident, [$($id:literal),*]) => {
        #[test]
        fn $name() {
            // Arrange.
            let provenance = metadata("sample", 2);
            // Act.
            let result = RunSelection::ordered(vec![$($id.into()),*], provenance);
            // Assert.
            assert_eq!(result.unwrap_err(), RunSelectionError::SampleIdsMismatch);
        }
    };
}
rejects_sample!(sample_rejects_reordered_ids, ["a", "b"]);
rejects_sample!(sample_rejects_different_membership, ["b", "c"]);
rejects_sample!(sample_rejects_duplicate_execution_ids, ["b", "a", "b"]);

macro_rules! accepts_empty {
    ($name:ident, $constructor:ident, $mode:literal) => {
        #[test]
        fn $name() {
            // Arrange.
            let provenance = metadata($mode, 0);
            // Act.
            let selection = RunSelection::$constructor(Vec::new(), provenance).unwrap();
            // Assert.
            assert!(selection.candidate_ids().unwrap().is_empty());
            assert_eq!(selection.verification_selection().unwrap().selected(), 0);
        }
    };
}
accepts_empty!(explicit_accepts_empty_selection, explicit, "explicit");
accepts_empty!(top_accepts_empty_selection, ordered, "top");

#[test]
fn top_deduplicates_preserving_first_requested_order() {
    // Arrange.
    let provenance = metadata("top", 2);
    // Act.
    let selection =
        RunSelection::ordered(vec!["b".into(), "a".into(), "b".into()], provenance).unwrap();
    // Assert.
    assert_eq!(selection.ordered_ids().unwrap(), ["b", "a"]);
    assert_eq!(selection.candidate_ids().unwrap().len(), 2);
}

#[test]
fn sample_preserves_seed_population_and_order() {
    // Arrange.
    let provenance = metadata("sample", 2);
    // Act.
    let selection = RunSelection::ordered(vec!["b".into(), "a".into()], provenance).unwrap();
    // Assert.
    assert_eq!(selection.ordered_ids().unwrap(), ["b", "a"]);
    let sample = selection
        .verification_selection()
        .unwrap()
        .sampling()
        .unwrap();
    assert_eq!(sample.selected_ids(), ["b", "a"]);
    assert_eq!(sample.seed(), 42);
    assert_eq!(sample.population(), 10);
}

#[test]
fn explicit_deduplicates_without_rewriting_requested_count() {
    // Arrange.
    let provenance = metadata("explicit", 2);
    // Act.
    let selection =
        RunSelection::explicit(vec!["b".into(), "a".into(), "b".into()], provenance).unwrap();
    // Assert.
    assert_eq!(
        selection
            .candidate_ids()
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(selection.verification_selection().unwrap().requested(), 3);
    assert_eq!(selection.verification_selection().unwrap().selected(), 2);
    assert!(selection.ordered_ids().is_none());
}
