use hoimin_core::DiskRunSummary;

#[test]
fn disk_summary_requires_janitor_observation_even_when_unmeasured() {
    // Arrange: omission must not be confused with an explicitly unmeasured janitor.
    let mut wire = serde_json::to_value(DiskRunSummary::unmeasured(10, 1)).unwrap();
    wire.as_object_mut().unwrap().remove("janitor");

    // Act.
    let decoded = serde_json::from_value::<DiskRunSummary>(wire);

    // Assert.
    assert!(decoded.is_err(), "missing janitor must be rejected");
}

#[test]
fn unmeasured_disk_summary_emits_explicit_null_janitor() {
    // Arrange.
    let disk = DiskRunSummary::unmeasured(10, 1);

    // Act.
    let wire = serde_json::to_value(disk).unwrap();

    // Assert.
    assert_eq!(wire.get("janitor"), Some(&serde_json::Value::Null));
}

#[test]
fn disk_summary_rejects_retired_count_beside_current_janitor() {
    // Arrange: a partially migrated producer must not silently lose its old count.
    let mut wire = serde_json::to_value(DiskRunSummary::unmeasured(10, 1)).unwrap();
    wire["stale_roots_reclaimed"] = serde_json::json!(999);

    // Act.
    let error = serde_json::from_value::<DiskRunSummary>(wire).unwrap_err();

    // Assert.
    assert!(
        error
            .to_string()
            .contains("unknown field `stale_roots_reclaimed`")
    );
}

#[test]
fn disk_summary_accepts_explicit_null_janitor() {
    // Arrange.
    let disk = DiskRunSummary::unmeasured(10, 1);
    let wire = serde_json::to_value(&disk).unwrap();

    // Act.
    let decoded = serde_json::from_value::<DiskRunSummary>(wire).unwrap();

    // Assert.
    assert_eq!(decoded, disk);
}
