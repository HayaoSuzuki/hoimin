#[path = "support/quint.rs"]
mod quint;

use rstest::rstest;
use serde_json::json;

#[rstest]
#[case::budget("Budget::BudgetModel::nextId")]
#[case::extended("BudgetExtended::BudgetModel::nextId")]
#[case::completion("Completion::CompletionModel::nextId")]
fn reads_independent_model_namespace(#[case] name: &str) {
    // Arrange.
    let encoded = json!({"#meta":{"format":"ITF","status":"ok"},"states":[
        {name: {"#bigint":"18446744073709551615"}}
    ]})
    .to_string();
    // Act.
    let states = quint::states(&encoded);
    // Assert.
    assert_eq!(
        states[0]["nextId"],
        json!({"#bigint":"18446744073709551615"})
    );
}

#[test]
#[should_panic(expected = "ambiguous ITF variable")]
fn rejects_colliding_variable_names() {
    // Arrange.
    let encoded = json!({"#meta":{"format":"ITF","status":"ok"},"states":[
        {"A::id":{"#bigint":"1"},"B::id":{"#bigint":"2"}}
    ]})
    .to_string();
    // Act / Assert.
    quint::states(&encoded);
}

#[test]
fn preserves_sets_and_full_width_integers() {
    // Arrange.
    let encoded = json!({"#set":[{"#bigint":"18446744073709551615"}]});
    // Act.
    let value = quint::number(&quint::set(&encoded)[0]);
    // Assert.
    assert_eq!(value, u64::MAX);
}
