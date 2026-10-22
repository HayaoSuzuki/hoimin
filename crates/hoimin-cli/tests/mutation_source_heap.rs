//! Serialized tests own allocator windows around real worker mutation applications.
use camino::Utf8Path;
use hoimin_cli::workspace::{CopyOptions, WorkspacePlan};
use hoimin_core::{
    BudgetLedger, ByteSpan, CandidateDescriptor, EffectId, MutationCandidate, RunBudgets,
    reserve_workspace_copy, validate_candidate,
};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

static ALLOCATION_WINDOW: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn equal_length_mutation_does_not_allocate_a_second_source_body() {
    let _window = ALLOCATION_WINDOW.lock().unwrap();
    let mut observations = Vec::new();
    for size in [64 * 1024, 256 * 1024, 1024 * 1024] {
        for offset in [1, size / 2, size - 4] {
            let project = tempfile::tempdir().unwrap();
            let mut source = vec![b'x'; size];
            source[0] = b'#';
            source[size - 1] = b'\n';
            source[offset..offset + 3].copy_from_slice(b"old");
            std::fs::write(project.path().join("source.py"), &source).unwrap();
            let root = Utf8Path::from_path(project.path()).unwrap();
            let plan =
                WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
            let mut ledger = BudgetLedger::new(RunBudgets {
                memory: 1,
                copy: plan.aggregate_bytes(),
                processes: 1,
            });
            let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
            let mut worker = plan
                .create_worker(&grant.create_worker(EffectId(2), 0).unwrap())
                .unwrap();
            let mut candidate = MutationCandidate {
                id: String::new(),
                sequence: 0,
                path: "source.py".into(),
                span: ByteSpan {
                    start: u64::try_from(offset).unwrap(),
                    length: 3,
                },
                original: "old".into(),
                replacement: "new".into(),
                operator: "test".into(),
                line: 1,
                column: u32::try_from(offset).unwrap(),
                symbol: None,
                file_hash: blake3::hash(&source).to_hex().to_string(),
            };
            candidate.id = validate_candidate(&source, &CandidateDescriptor::from(&candidate))
                .unwrap()
                .to_string();

            eprintln!("mutation-window begin size={size} offset={offset}");
            heap_tracking::begin();
            let result = worker.apply_mutation(&candidate);
            let peak = heap_tracking::finish();
            eprintln!("mutation-window end size={size} offset={offset} peak={peak}");
            result.unwrap();
            observations.push((size, offset, peak));

            let actual = worker.read("source.py").unwrap();
            assert_eq!(actual.len(), size);
            assert_eq!(&actual[..offset], &source[..offset]);
            assert_eq!(&actual[offset..offset + 3], b"new");
            assert_eq!(&actual[offset + 3..], &source[offset + 3..]);
            drop(actual);
            worker.reset().unwrap();
            assert_eq!(worker.read("source.py").unwrap(), source);
            assert_eq!(
                std::fs::read(project.path().join("source.py")).unwrap(),
                source
            );
        }
    }
    for (size, offset, peak) in observations {
        // Contract builds retain an independent original body for the actual
        // prefix/suffix postcondition; normal builds need only the acquired body.
        let bodies = if cfg!(feature = "contracts") { 2 } else { 1 };
        let bound = bodies * size + 32 * 1024;
        assert!(
            peak <= bound,
            "size={size} offset={offset} peak={peak} bound={bound}"
        );
    }
}

#[test]
fn issue_1082_wrong_size_apply_rejects_before_allocating_the_body() {
    let _window = ALLOCATION_WINDOW.lock().unwrap();
    let project = tempfile::tempdir().unwrap();
    let source = b"# old\n";
    std::fs::write(project.path().join("source.py"), source).unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: plan.aggregate_bytes(),
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
    let mut worker = plan
        .create_worker(&grant.create_worker(EffectId(2), 0).unwrap())
        .unwrap();
    let mut candidate = MutationCandidate {
        id: String::new(),
        sequence: 0,
        path: "source.py".into(),
        span: ByteSpan {
            start: 2,
            length: 3,
        },
        original: "old".into(),
        replacement: "new".into(),
        operator: "test".into(),
        line: 1,
        column: 2,
        symbol: None,
        file_hash: blake3::hash(source).to_hex().to_string(),
    };
    candidate.id = validate_candidate(source, &CandidateDescriptor::from(&candidate))
        .unwrap()
        .to_string();
    let target = worker.root().join("source.py");
    let enlarged = vec![b'x'; 128 * 1024];
    std::fs::write(&target, &enlarged).unwrap();
    let mut permissions = std::fs::metadata(&target).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&target, permissions).unwrap();

    heap_tracking::begin();
    let result = worker.apply_mutation(&candidate);
    let peak = heap_tracking::finish();
    eprintln!("wrong-size rejection peak={peak}");

    assert!(matches!(
        result,
        Err(hoimin_cli::workspace::WorkspaceError::MutationHashMismatch { .. })
    ));
    assert_eq!(std::fs::read(&target).unwrap(), enlarged);
    assert!(std::fs::metadata(&target).unwrap().permissions().readonly());
    assert_eq!(
        std::fs::read(project.path().join("source.py")).unwrap(),
        source
    );
    assert!(
        peak < 32 * 1024,
        "wrong-size rejection allocated {peak} bytes"
    );
}

#[test]
fn issue_1089_large_descriptor_apply_does_not_copy_metadata_bodies() {
    let _window = ALLOCATION_WINDOW.lock().unwrap();
    let mut observations = Vec::new();
    for size in [4 * 1024, 16 * 1024, 64 * 1024] {
        let project = tempfile::tempdir().unwrap();
        let original = format!("'{}'", "o".repeat(size - 3));
        let source = format!("{original}\n");
        let replacement = format!("'{}'", "r".repeat(size - 3));
        assert_eq!(source.len(), size);
        std::fs::write(project.path().join("source.py"), &source).unwrap();
        let root = Utf8Path::from_path(project.path()).unwrap();
        let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: plan.aggregate_bytes(),
            processes: 1,
        });
        let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
        let mut worker = plan
            .create_worker(&grant.create_worker(EffectId(2), 0).unwrap())
            .unwrap();
        let mut candidate = MutationCandidate {
            id: String::new(),
            sequence: 1,
            path: "source.py".into(),
            span: ByteSpan {
                start: 0,
                length: u64::try_from(original.len()).unwrap(),
            },
            original,
            replacement,
            operator: "string_literal_empty".into(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        };
        candidate.id = hoimin_core::CandidateIdentityRef::from(&candidate)
            .stable_mutant_id()
            .to_string();
        heap_tracking::begin();
        let result = worker.apply_mutation(&candidate);
        let peak = heap_tracking::finish();
        result.unwrap();
        eprintln!("issue_1089 apply size={size} peak={peak}");
        observations.push((size, peak));
        assert_eq!(
            std::fs::read_to_string(project.path().join("source.py")).unwrap(),
            source
        );
        assert_eq!(
            std::fs::read_to_string(worker.root().join("source.py")).unwrap(),
            format!("{}\n", candidate.replacement)
        );
    }
    let source_bodies = if cfg!(feature = "contracts") { 2 } else { 1 };
    assert!(
        observations
            .iter()
            .all(|(size, peak)| *peak < source_bodies * size + 32 * 1024),
        "metadata copies exceed source evidence allowance: {observations:?}"
    );
}
