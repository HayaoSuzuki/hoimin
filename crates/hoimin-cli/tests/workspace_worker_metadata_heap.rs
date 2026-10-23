#[path = "support/workspace_metadata_probe.rs"]
mod probe;

#[test]
fn retained_worker_metadata_does_not_multiply_file_inventory() {
    // Arrange/Act: real materialization, with grants and output capacity prepared.
    let (retained, _) = probe::measure("worker", 1_000, 4, 0);

    // Assert: fixed worker overhead allowance, far below four deep inventories.
    assert!(
        retained <= 256 * 1024 + 8 * 1024 * 4,
        "workers retained {retained} requested bytes"
    );
}
