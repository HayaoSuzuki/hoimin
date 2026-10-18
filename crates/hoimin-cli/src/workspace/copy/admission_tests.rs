use super::*;
use crate::workspace::{WorkspaceHandler, manifest};

macro_rules! inventory_rejection {
    ($name:ident, $workers:expr, $cap:expr, $requested:expr) => {
        #[test]
        fn $name() {
            // Arrange: omitting copy admission would read and snapshot this file.
            let source = tempfile::tempdir().unwrap();
            fs::write(source.path().join("target.py"), b"12345").unwrap();
            let mut handler = WorkspaceHandler::new(
                Utf8Path::from_path(source.path()).unwrap().to_owned(),
                Vec::new(),
                $workers,
                CopyOptions::default(),
            )
            .with_max_copy_bytes($cap);
            manifest::reset_source_content_metrics();
            reset_snapshot_write_metrics();

            // Act.
            let result = handler.handle_preflight(hoimin_core::Preflight { id: EffectId(1) });

            // Assert: the independent five-byte fixture is charged once per worker.
            assert_eq!(snapshot_write_metrics(), (0, 0));
            assert_eq!(
                manifest::source_content_metrics(),
                manifest::SourceContentMetrics {
                    reads: 0,
                    read_bytes: 0,
                    hashes: 0,
                    hash_bytes: 0,
                }
            );
            assert!(matches!(
                result.unwrap_err().failure,
                hoimin_core::EffectFailure::CopyLimit {
                    requested: $requested,
                    allowance: $cap
                }
            ));
            assert!(handler.plan.is_none());
        }
    };
}

inventory_rejection!(copy_inventory_over_cap_writes_nothing, 1, 4, 5);
inventory_rejection!(copy_inventory_multiplies_workers_before_writing, 3, 14, 15);

macro_rules! admitted_copy {
    ($name:ident, $workers:expr, $cap:expr, $includes:expr, $expected:expr) => {
        #[test]
        fn $name() {
            // Arrange.
            let source = tempfile::tempdir().unwrap();
            fs::write(source.path().join("target.py"), b"12345").unwrap();
            let mut handler = WorkspaceHandler::new(
                Utf8Path::from_path(source.path()).unwrap().to_owned(),
                Vec::new(),
                $workers,
                CopyOptions {
                    includes: $includes,
                    ..CopyOptions::default()
                },
            )
            .with_max_copy_bytes($cap);
            reset_snapshot_write_metrics();

            // Act.
            let completed = handler
                .handle_preflight(hoimin_core::Preflight { id: EffectId(1) })
                .unwrap();
            let mut ledger = hoimin_core::BudgetLedger::new(hoimin_core::RunBudgets {
                memory: 1,
                copy: $cap,
                processes: 1,
            });
            let grant = hoimin_core::reserve_workspace_copy(&mut ledger, &completed).unwrap();

            // Assert: snapshot bytes are not included in the worker copy grant.
            assert_eq!(completed.per_worker_logical_bytes, 5);
            assert_eq!(completed.aggregate_logical_bytes, $expected);
            assert_eq!(grant.granted_allowance(), $expected);
            assert_eq!(ledger.reserved(hoimin_core::BudgetKind::Copy), $expected);
            assert_eq!(snapshot_write_metrics(), (1, 5));
        }
    };
}

admitted_copy!(copy_equality_is_admitted, 1, 5, vec![], 5);
admitted_copy!(copy_worker_equality_is_admitted, 3, 15, vec![], 15);
admitted_copy!(
    copy_duplicate_includes_are_charged_once,
    2,
    10,
    vec!["target.py".into(), "target.py".into()],
    10
);

#[test]
fn admitted_copy_keeps_two_content_passes() {
    // Arrange.
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("target.py"), b"12345").unwrap();
    let mut handler = WorkspaceHandler::new(
        Utf8Path::from_path(source.path()).unwrap().to_owned(),
        Vec::new(),
        1,
        CopyOptions::default(),
    )
    .with_max_copy_bytes(5);
    manifest::reset_source_content_metrics();

    // Act.
    handler
        .handle_preflight(hoimin_core::Preflight { id: EffectId(1) })
        .unwrap();

    // Assert.
    assert_eq!(
        manifest::source_content_metrics(),
        manifest::SourceContentMetrics {
            reads: 2,
            read_bytes: 10,
            hashes: 2,
            hash_bytes: 10,
        }
    );
}

#[test]
fn copy_source_growth_after_inventory_rolls_back_without_writes() {
    // Arrange: inventory admits four bytes, then the source grows to five.
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("target.py"), b"1234").unwrap();
    let source_root = Utf8Path::from_path(source.path()).unwrap();
    assert_eq!(
        inventory_logical_bytes(source_root, &CopyOptions::default()).unwrap(),
        4
    );
    fs::write(source.path().join("target.py"), b"12345").unwrap();
    let parent = tempfile::tempdir().unwrap();
    let coordinator =
        crate::workspace::ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap())
            .unwrap();
    let root = Arc::new(
        ManagedRunRoot::create(&coordinator, crate::workspace::OwnerKind::PublicExecution).unwrap(),
    );
    let children_before = fs::read_dir(root.path()).unwrap().count();
    reset_snapshot_write_metrics();

    // Act.
    let result = create_pending_disk_snapshot(
        source_root,
        &CopyOptions::default(),
        2,
        PreflightLimits {
            copy: Some(8),
            owned: None,
        },
        Some(&root),
    );

    // Assert.
    assert_eq!(snapshot_write_metrics(), (0, 0));
    assert_eq!(
        result.unwrap_err(),
        WorkspaceError::CopyAllowanceExceeded {
            observed: 10,
            allowance: 8,
        }
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), children_before);
    assert_eq!(fs::read(source_root.join("target.py")).unwrap(), b"12345");
}

#[test]
fn copy_stream_growth_rejects_before_over_cap_chunk() {
    // Arrange: the reader supplies two chunks after reporting a one-byte initial size.
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("data"), b"x").unwrap();
    let source_root = Utf8Path::from_path(source.path()).unwrap();
    let snapshot = tempfile::tempdir().unwrap();
    let snapshot_root = Utf8Path::from_path(snapshot.path()).unwrap();
    let block = crate::workspace::stream::BUFFER_BYTES;
    let bytes = vec![b'x'; 2 * block];
    let permissions = fs::metadata(source_root.join("data"))
        .unwrap()
        .permissions();
    let cap = 3 * block as u64;
    let mut writer = SnapshotWriter::new(
        source_root,
        snapshot_root,
        3,
        PreflightLimits {
            copy: Some(cap),
            owned: None,
        },
    );
    reset_snapshot_write_metrics();

    // Act.
    let result = writer.write_reader(
        Utf8Path::new("data"),
        &mut std::io::Cursor::new(bytes),
        1,
        permissions,
    );

    // Assert: one exactly admitted chunk remains; the second was never written.
    assert_eq!(
        fs::metadata(snapshot_root.join("data")).unwrap().len(),
        block as u64
    );
    assert_eq!(snapshot_write_metrics(), (1, block as u64));
    assert_eq!(
        result.unwrap_err(),
        WorkspaceError::CopyAllowanceExceeded {
            observed: 6 * block as u64,
            allowance: cap,
        }
    );
    assert!(writer.first_entries.is_empty());
    assert!(writer.files.is_empty());
    assert_eq!(writer.logical_bytes, 0);
}

#[test]
fn copy_admission_rejects_worker_multiplication_overflow() {
    // Arrange.
    let limits = PreflightLimits {
        copy: Some(u64::MAX),
        owned: None,
    };

    // Act.
    let result = limits.check(u64::MAX / 2 + 1, 2);

    // Assert.
    assert_eq!(result, Err(WorkspaceError::CopySizeOverflow));
}

#[test]
fn copy_inventory_rejection_precedes_content_validation() {
    // Arrange.
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("target.py"), b"12345").unwrap();
    let mut handler = WorkspaceHandler::new(
        Utf8Path::from_path(source.path()).unwrap().to_owned(),
        Vec::new(),
        1,
        CopyOptions::default(),
    )
    .with_max_copy_bytes(4);
    let validations = Cell::new(0);

    // Act.
    let result =
        handler.handle_preflight_validated(hoimin_core::Preflight { id: EffectId(1) }, |_, _| {
            validations.set(validations.get() + 1);
            Ok(())
        });

    // Assert.
    assert_eq!(validations.get(), 0);
    assert_eq!(result.unwrap_err().failure.code(), "workspace.copy.limit");
}

#[test]
fn copy_added_file_crossing_cap_rolls_back_partial_snapshot() {
    // Arrange: either enumeration order permits exactly the first four-byte file.
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("first.py"), b"1234").unwrap();
    let source_root = Utf8Path::from_path(source.path()).unwrap();
    assert_eq!(
        inventory_logical_bytes(source_root, &CopyOptions::default()).unwrap(),
        4
    );
    fs::write(source.path().join("added.py"), b"5678").unwrap();
    let parent = tempfile::tempdir().unwrap();
    let coordinator =
        crate::workspace::ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap())
            .unwrap();
    let root = Arc::new(
        ManagedRunRoot::create(&coordinator, crate::workspace::OwnerKind::PublicExecution).unwrap(),
    );
    let children_before = fs::read_dir(root.path()).unwrap().count();
    reset_snapshot_write_metrics();

    // Act.
    let result = create_pending_disk_snapshot(
        source_root,
        &CopyOptions::default(),
        1,
        PreflightLimits {
            copy: Some(6),
            owned: None,
        },
        Some(&root),
    );

    // Assert.
    assert_eq!(snapshot_write_metrics(), (1, 4));
    assert_eq!(
        result.unwrap_err(),
        WorkspaceError::CopyAllowanceExceeded {
            observed: 8,
            allowance: 6,
        }
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), children_before);
    assert_eq!(fs::read(source_root.join("first.py")).unwrap(), b"1234");
    assert_eq!(fs::read(source_root.join("added.py")).unwrap(), b"5678");
}

#[test]
fn owned_equality_is_rejected_even_when_copy_equality_is_admitted() {
    // Arrange.
    let limits = PreflightLimits {
        copy: Some(5),
        owned: Some(10),
    };

    // Act.
    let result = limits.check(5, 1);

    // Assert.
    assert_eq!(
        result,
        Err(WorkspaceError::OwnedWorkspaceLimit {
            planned: 10,
            limit: 10
        })
    );
}
