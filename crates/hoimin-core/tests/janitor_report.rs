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
