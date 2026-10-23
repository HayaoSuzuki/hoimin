use super::*;
use crate::source_file::{WorkspaceRaceHook, install_workspace_race_hook};
use crate::workspace::WorkspaceHandler;

#[derive(Clone, Copy)]
enum Change {
    Append,
    Truncate,
}

impl Change {
    fn message(self) -> &'static str {
        match self {
            Self::Append => "file grew",
            Self::Truncate => "file shrank",
        }
    }
}

struct ChangeAfterBudget {
    path: Utf8PathBuf,
    operation: &'static str,
    change: Change,
}

impl WorkspaceRaceHook for ChangeAfterBudget {
    fn parent_opened(&self, operation: &'static str, _: &Utf8Path) {
        if operation != self.operation {
            return;
        }
        match self.change {
            Change::Append => fs::OpenOptions::new()
                .append(true)
                .open(&self.path)
                .unwrap()
                .write_all(&[b'x'; 256])
                .unwrap(),
            Change::Truncate => {
                fs::File::create(&self.path).unwrap();
            }
        }
    }
}

fn changing_hook(
    path: Utf8PathBuf,
    operation: &'static str,
    change: Change,
) -> crate::source_file::WorkspaceRaceHookGuard {
    install_workspace_race_hook(Arc::new(ChangeAfterBudget {
        path,
        operation,
        change,
    }))
}

#[test]
fn original_length_budget_rejects_manifest_changes_and_cleans_pending_snapshot() {
    for operation in ["snapshot-budget", "manifest-budget"] {
        for change in [Change::Append, Change::Truncate] {
            let source = tempfile::tempdir().unwrap();
            let source_root = Utf8Path::from_path(source.path()).unwrap();
            let file = source_root.join("target.py");
            fs::write(&file, b"12345").unwrap();
            let parent = tempfile::tempdir().unwrap();
            let coordinator = crate::workspace::ManagedRootCoordinator::open(
                Utf8Path::from_path(parent.path()).unwrap(),
            )
            .unwrap();
            let managed = Arc::new(
                ManagedRunRoot::create(&coordinator, crate::workspace::OwnerKind::PublicExecution)
                    .unwrap(),
            );
            let children_before = fs::read_dir(managed.path()).unwrap().count();
            let mut handler =
                WorkspaceHandler::new(source_root.to_owned(), vec![], 1, CopyOptions::default())
                    .with_managed_root(Arc::clone(&managed))
                    .with_max_copy_bytes(64);
            let _hook = changing_hook(file, operation, change);
            let error = handler
                .handle_preflight(hoimin_core::Preflight { id: EffectId(1) })
                .unwrap_err();
            assert!(
                error.failure.message().contains(change.message()),
                "{error:?}"
            );
            assert!(handler.plan.is_none());
            assert_eq!(
                fs::read_dir(managed.path()).unwrap().count(),
                children_before
            );
        }
    }
}

#[test]
fn original_length_budget_prevents_successful_original_verification() {
    for change in [Change::Append, Change::Truncate] {
        let source = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(source.path()).unwrap();
        let path = root.join("target.py");
        fs::write(&path, b"12345").unwrap();
        let mut handler = WorkspaceHandler::new(root.to_owned(), vec![], 1, CopyOptions::default());
        handler
            .handle_preflight(hoimin_core::Preflight { id: EffectId(1) })
            .unwrap();
        let _hook = changing_hook(path, "manifest-budget", change);
        let error = handler
            .handle_verify_originals(hoimin_core::VerifyOriginals {
                id: EffectId(2),
                checkpoint: hoimin_core::IntegrityCheckpoint::PostMaterialization,
            })
            .unwrap_err();
        assert!(
            error.failure.message().contains(change.message()),
            "{error:?}"
        );
    }
}

#[test]
fn original_length_budget_protects_duplicate_snapshot_entries() {
    for change in [Change::Append, Change::Truncate] {
        let source = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(source.path()).unwrap();
        let path = root.join("target.py");
        fs::write(&path, b"12345").unwrap();
        let snapshot = tempfile::tempdir().unwrap();
        let snapshot_root = Utf8Path::from_path(snapshot.path()).unwrap();
        let mut writer = SnapshotWriter::new(root, snapshot_root, 1, PreflightLimits::default());
        writer
            .write_stream(Utf8Path::new("target.py"), fs::File::open(&path).unwrap())
            .unwrap();
        let _hook = changing_hook(path.clone(), "snapshot-budget", change);
        let error = writer
            .write_stream(Utf8Path::new("target.py"), fs::File::open(&path).unwrap())
            .unwrap_err();
        assert!(error.to_string().contains(change.message()), "{error}");
        assert_eq!(fs::read(snapshot_root.join("target.py")).unwrap(), b"12345");
    }
}

#[test]
fn original_length_budget_preserves_stable_bytes_and_duplicate_includes() {
    for length in [0_u64, 1, 128 * 1024] {
        let source = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(source.path()).unwrap();
        let bytes: Vec<_> = (0..length)
            .map(|i| u8::try_from(i % 251).unwrap())
            .collect();
        fs::write(root.join("target.py"), &bytes).unwrap();
        let plan = WorkspacePlan::preflight(
            root,
            EffectId(1),
            1,
            CopyOptions {
                includes: vec!["target.py".into(), "target.py".into()],
                ..CopyOptions::default()
            },
        )
        .unwrap();
        plan.verify_originals().unwrap();
        assert_eq!(plan.manifest.logical_bytes(), length);
        assert_eq!(plan.manifest.entries()[0].blake3, blake3::hash(&bytes));
    }
}
