use std::fs;

use camino::Utf8Path;
use hoimin_cli::workspace::{CopyOptions, WorkspacePlan};
use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};

fn create_worker(plan: &WorkspacePlan) -> hoimin_cli::workspace::WorkerWorkspace {
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: plan.aggregate_bytes(),
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
    plan.create_worker(&grant.create_worker(EffectId(2), 0).unwrap())
        .unwrap()
}

#[test]
fn binary_boundaries_and_same_length_changes_restore_from_the_snapshot() {
    for size in [0_usize, 1, 65_535, 65_536, 65_537, 131_073] {
        let project = tempfile::tempdir().unwrap();
        let expected = (0..size)
            .map(|index| index.to_le_bytes()[0])
            .collect::<Vec<_>>();
        fs::write(project.path().join("binary.dat"), &expected).unwrap();
        let root = Utf8Path::from_path(project.path()).unwrap();
        let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
        let mut worker = create_worker(&plan);
        let path = worker.root().join("binary.dat");
        assert_eq!(fs::read(&path).unwrap(), expected);
        // Restore always uses the accepted snapshot, even after the source changes.
        fs::write(project.path().join("binary.dat"), b"later source").unwrap();
        worker.reset().unwrap();
        assert_eq!(fs::read(&path).unwrap(), expected);
        if size != 0 {
            for index in [0, size / 2, size - 1] {
                let mut changed = expected.clone();
                changed[index] ^= 0xff;
                fs::write(&path, changed).unwrap();
                worker.reset().unwrap();
                assert_eq!(fs::read(&path).unwrap(), expected);
            }
        }
        for changed in [vec![], vec![0; size + 1]] {
            fs::write(&path, changed).unwrap();
            worker.reset().unwrap();
            assert_eq!(fs::read(&path).unwrap(), expected);
        }
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        worker.reset().unwrap();
        assert_eq!(fs::read(path).unwrap(), expected);
    }
}

#[test]
fn reset_replaces_an_outside_hardlink_without_writing_its_referent() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("target.bin"), b"original").unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
    let mut worker = create_worker(&plan);
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel");
    fs::write(&sentinel, b"external").unwrap();
    let path = worker.root().join("target.bin");
    fs::remove_file(&path).unwrap();
    fs::hard_link(&sentinel, &path).unwrap();
    worker.reset().unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"original");
    assert_eq!(fs::read(&sentinel).unwrap(), b"external");
    fs::write(&path, b"new worker").unwrap();
    assert_eq!(fs::read(sentinel).unwrap(), b"external");
}
