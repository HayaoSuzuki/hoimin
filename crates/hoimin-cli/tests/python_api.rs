use camino::Utf8Path;
use hoimin_core::{MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};

#[test]
fn returned_candidate_keeps_its_payload_reservation_until_drop() {
    // Arrange: one Boolean candidate owns 4 path + 4 original + 5 replacement bytes.
    let budget = PayloadBudget::new(32);
    let operators: MutationOperatorSelection =
        serde_json::from_str("[\"boolean_literal\"]").unwrap();
    let request = AnalyzeRequest {
        payload: budget.clone(),
        path: Utf8Path::new("a.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    // Act: transfer a candidate out of the result, then try to reuse its reservation.
    let mut output = analyze_source_cancellable(&request, "x = True\n", || false).unwrap();
    let candidate = output.candidates.pop().unwrap();
    drop(output);
    let while_retained = budget.reserve([20]);
    drop(candidate);
    let after_drop = budget.reserve([32]);

    // Assert: the charged return value, not the result container, owns the bytes.
    assert!(while_retained.is_err());
    assert!(after_drop.is_ok());
}
