#[path = "../../../docs/maintenance/2026-10-20-issue-1007/probe.rs"]
mod probe;

#[test]
fn retained_manifest_clones_do_not_multiply_metadata() {
    // Arrange/Act: handles and fixture setup are excluded by the isolated probe.
    let (retained, _) = probe::measure("clone", 4_000, 16, 0);

    // Assert: the old deep clones retain about 7.5 MiB on this exact fixture.
    assert!(
        retained <= 16 * 1024,
        "clones retained {retained} requested bytes"
    );
}
