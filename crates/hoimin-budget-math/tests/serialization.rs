use std::time::Duration;

use hoimin_budget_math::{MutantTimeout, NonZeroDuration};

#[test]
fn zero_duration_deserialization_is_rejected() {
    let result = serde_json::from_str::<NonZeroDuration>(r#"{"secs":0,"nanos":0}"#);

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("invalid zero or overflowing limit: duration")
    );
}

#[test]
fn positive_duration_retains_existing_json() {
    let duration = NonZeroDuration::new(Duration::new(7, 12)).unwrap();

    let result = serde_json::to_value(duration).unwrap();

    assert_eq!(result, serde_json::json!({"secs":7,"nanos":12}));
}

#[test]
fn fixed_timeout_retains_existing_json() {
    let timeout = MutantTimeout::Fixed(NonZeroDuration::new(Duration::new(7, 12)).unwrap());

    let result = serde_json::to_value(timeout).unwrap();

    assert_eq!(result, serde_json::json!({"fixed":{"secs":7,"nanos":12}}));
}

#[test]
fn auto_timeout_retains_existing_json() {
    let result = serde_json::to_value(MutantTimeout::Auto).unwrap();

    assert_eq!(result, serde_json::json!("auto"));
}

#[test]
fn maximum_duration_deserialization_is_preserved() {
    let value = serde_json::json!({"secs":u64::MAX,"nanos":999_999_999});

    let result = serde_json::from_value::<NonZeroDuration>(value).unwrap();

    assert_eq!(result.get(), Duration::MAX);
}

#[test]
fn fixed_zero_timeout_deserialization_is_rejected() {
    let result = serde_json::from_str::<MutantTimeout>(r#"{"fixed":{"secs":0,"nanos":0}}"#);

    assert!(result.is_err());
}
