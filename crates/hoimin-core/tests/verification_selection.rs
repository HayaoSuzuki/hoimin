use hoimin_core::{VerificationSampling, VerificationSelection, VerificationTopPolicy};
use proptest::prelude::*;
use serde_json::{Value, json};

fn selection(mode: &str, policy: &str, scope: &str) -> Value {
    json!({
        "mode": mode, "policy": policy, "requested": 3, "selected": 2,
        "scope": scope, "plan_truncated": false
    })
}

fn sample() -> Value {
    let mut value = selection("sample", "splitmix64_fisher_yates_v1", "sampled_candidates");
    value["sampling"] = json!({"population": 10, "seed": 42, "selected_ids": ["second", "first"]});
    value
}

#[test]
fn selection_rejects_policy_from_another_mode() {
    for (mode, scope, policy) in [
        ("candidate_ids", "explicit_candidates", "strict"),
        ("top", "retained_candidates", "explicit_candidates"),
        ("sample", "sampled_candidates", "file_round_robin_v1"),
    ] {
        // Arrange
        let mut value = if mode == "sample" {
            sample()
        } else {
            selection(mode, policy, scope)
        };
        value["policy"] = json!(policy);
        // Act
        let result = serde_json::from_value::<VerificationSelection>(value);
        // Assert
        assert!(result.is_err(), "mode={mode}, policy={policy}");
    }
}

#[test]
fn selection_rejects_scope_from_another_mode() {
    for (mode, policy, scope) in [
        (
            "candidate_ids",
            "explicit_candidates",
            "retained_candidates",
        ),
        ("top", "strict", "sampled_candidates"),
        (
            "sample",
            "splitmix64_fisher_yates_v1",
            "explicit_candidates",
        ),
    ] {
        // Arrange
        let mut value = if mode == "sample" {
            sample()
        } else {
            selection(mode, policy, scope)
        };
        value["scope"] = json!(scope);
        // Act
        let result = serde_json::from_value::<VerificationSelection>(value);
        // Assert
        assert!(result.is_err(), "mode={mode}, scope={scope}");
    }
}

#[test]
fn sample_requires_evidence() {
    for evidence in [None, Some(Value::Null)] {
        // Arrange
        let mut value = sample();
        value.as_object_mut().unwrap().remove("sampling");
        if let Some(evidence) = evidence {
            value["sampling"] = evidence;
        }
        // Act
        let result = serde_json::from_value::<VerificationSelection>(value);
        // Assert
        assert!(result.is_err());
    }
}

#[test]
fn non_sample_rejects_even_null_sampling_fields() {
    for (mode, policy, scope) in [
        (
            "candidate_ids",
            "explicit_candidates",
            "explicit_candidates",
        ),
        ("top", "strict", "retained_candidates"),
    ] {
        for evidence in [Value::Null, sample()["sampling"].clone()] {
            // Arrange
            let mut value = selection(mode, policy, scope);
            value["sampling"] = evidence;
            // Act
            let result = serde_json::from_value::<VerificationSelection>(value);
            // Assert
            assert!(result.is_err(), "mode={mode}");
        }
    }
}

#[test]
fn sample_rejects_a_truncated_population() {
    // Arrange
    let mut value = sample();
    value["plan_truncated"] = json!(true);
    // Act
    let result = serde_json::from_value::<VerificationSelection>(value);
    // Assert
    assert!(result.is_err());
}

#[test]
fn selection_rejects_zero_requested_count() {
    // Arrange
    let mut value = selection("top", "strict", "retained_candidates");
    value["requested"] = json!(0);
    value["selected"] = json!(0);
    // Act
    let result = serde_json::from_value::<VerificationSelection>(value);
    // Assert
    assert!(result.is_err());
}

#[test]
fn sample_rejects_zero_population() {
    // Arrange
    let mut value = sample();
    value["sampling"]["population"] = json!(0);
    // Act
    let result = serde_json::from_value::<VerificationSelection>(value);
    // Assert
    assert!(result.is_err());
}

#[test]
fn sample_rejects_empty_evidence() {
    // Arrange
    let mut value = sample();
    value["sampling"]["selected_ids"] = json!([]);
    value["selected"] = json!(0);
    // Act
    let result = serde_json::from_value::<VerificationSelection>(value);
    // Assert
    assert!(result.is_err());
}

#[test]
fn sample_rejects_repeated_ids() {
    // Arrange
    let mut value = sample();
    value["sampling"]["selected_ids"] = json!(["same", "same"]);
    // Act
    let result = serde_json::from_value::<VerificationSelection>(value);
    // Assert
    assert!(result.is_err());
}

#[test]
fn sample_rejects_more_ids_than_the_population() {
    // Arrange
    let mut value = sample();
    value["sampling"]["population"] = json!(1);
    // Act
    let result = serde_json::from_value::<VerificationSelection>(value);
    // Assert
    assert!(result.is_err());
}

#[test]
fn non_sample_accepts_zero_selected_count() {
    for (mode, policy, scope) in [
        (
            "candidate_ids",
            "explicit_candidates",
            "explicit_candidates",
        ),
        ("top", "strict", "retained_candidates"),
    ] {
        // Arrange
        let mut value = selection(mode, policy, scope);
        value["selected"] = json!(0);
        // Act
        let result = serde_json::from_value::<VerificationSelection>(value);
        // Assert
        assert!(result.is_ok(), "{result:?}");
    }
}

#[test]
fn outer_selection_accepts_additive_metadata() {
    // Arrange
    let mut value = sample();
    value["future_annotation"] = json!({"source": "external"});
    // Act
    let result = serde_json::from_value::<VerificationSelection>(value);
    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn sampling_rejects_unknown_evidence_fields() {
    // Arrange
    let mut value = sample();
    value["sampling"]["future_annotation"] = json!(true);
    // Act
    let result = serde_json::from_value::<VerificationSelection>(value);
    // Assert
    assert!(result.is_err());
}

#[test]
fn constructors_reject_zero_requests() {
    // Arrange
    let policies = [
        VerificationTopPolicy::Strict,
        VerificationTopPolicy::FileRoundRobinV1,
        VerificationTopPolicy::LineRoundRobinV1,
    ];
    // Act
    let results = policies.map(|policy| VerificationSelection::top(0, 0, policy, false));
    let explicit = VerificationSelection::candidate_ids(0, 0, false);
    // Assert
    assert!(explicit.is_err());
    assert!(results.into_iter().all(|result| result.is_err()));
}

proptest! {
    #[test]
    fn sample_constructor_derives_wire_count(seed in any::<u64>(), count in 1usize..50, extra in 0usize..50) {
        // Arrange
        let ids: Vec<_> = (0..count).rev().map(|index| format!("id-{index}")).collect();
        let evidence = VerificationSampling::new(count + extra, seed, ids.clone()).unwrap();
        // Act
        let result = VerificationSelection::sample(count + extra, evidence).unwrap();
        let encoded = serde_json::to_value(result).unwrap();
        // Assert
        prop_assert_eq!(encoded, json!({
            "mode": "sample", "policy": "splitmix64_fisher_yates_v1", "scope": "sampled_candidates",
            "requested": count + extra, "selected": count, "plan_truncated": false,
            "sampling": {"population": count + extra, "seed": seed, "selected_ids": ids}
        }));
    }


    #[test]
    fn selection_rejects_selected_count_above_requested(requested in 1usize..1000, excess in 1usize..1000) {
        // Arrange
        let mut value = selection("top", "strict", "retained_candidates");
        value["requested"] = json!(requested);
        value["selected"] = json!(requested + excess);
        // Act
        let result = serde_json::from_value::<VerificationSelection>(value);
        // Assert
        prop_assert!(result.is_err());
    }

    #[test]
    fn sample_rejects_count_different_from_evidence(selected in 0usize..100) {
        prop_assume!(selected != 2);
        // Arrange
        let mut value = sample();
        value["requested"] = json!(100);
        value["selected"] = json!(selected);
        // Act
        let result = serde_json::from_value::<VerificationSelection>(value);
        // Assert
        prop_assert!(result.is_err());
    }

    #[test]
    fn non_sample_round_trip_preserves_current_wire(
        mode in 0usize..4, requested in 1usize..1000, selected_fraction in 0usize..1000, truncated in any::<bool>(),
    ) {
        // Arrange
        let (mode, policy, scope) = [
            ("candidate_ids", "explicit_candidates", "explicit_candidates"),
            ("top", "strict", "retained_candidates"),
            ("top", "file_round_robin_v1", "retained_candidates"),
            ("top", "line_round_robin_v1", "retained_candidates"),
        ][mode];
        let mut value = selection(mode, policy, scope);
        value["requested"] = json!(requested);
        value["selected"] = json!(selected_fraction.min(requested));
        value["plan_truncated"] = json!(truncated);
        // Act
        let decoded: VerificationSelection = serde_json::from_value(value.clone()).unwrap();
        let encoded = serde_json::to_value(decoded).unwrap();
        // Assert
        prop_assert_eq!(encoded, value);
    }

    #[test]
    fn sample_round_trip_preserves_order_and_seed(seed in any::<u64>(), count in 1usize..50, extra in 0usize..50) {
        // Arrange
        let ids: Vec<_> = (0..count).rev().map(|index| format!("candidate-{index}")).collect();
        let value = json!({
            "mode": "sample", "policy": "splitmix64_fisher_yates_v1", "scope": "sampled_candidates",
            "requested": count + extra, "selected": count, "plan_truncated": false,
            "sampling": {"population": count + extra, "seed": seed, "selected_ids": ids}
        });
        // Act
        let decoded: VerificationSelection = serde_json::from_value(value.clone()).unwrap();
        let encoded = serde_json::to_value(decoded).unwrap();
        // Assert
        prop_assert_eq!(encoded, value);
    }
}
