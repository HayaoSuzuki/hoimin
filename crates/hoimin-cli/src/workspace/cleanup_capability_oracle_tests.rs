use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/cleanup-capability.jsonl");

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    strategy: String,
    schedule: Vec<String>,
    result: String,
    outside_writable: bool,
}

struct ExpectedCase {
    id: &'static str,
    mode: &'static str,
    scenario: &'static str,
    strategy: &'static str,
    schedule: &'static [&'static str],
    result: &'static str,
}

const EXPECTED_CASES: &[ExpectedCase] = &[
    ExpectedCase {
        id: "stable_entry",
        mode: "strict",
        scenario: "stable-entry",
        strategy: "post-inspection",
        schedule: &["inspect", "bind", "effect"],
        result: "complete",
    },
    ExpectedCase {
        id: "preexisting_outside_link",
        mode: "strict",
        scenario: "preexisting-outside-link",
        strategy: "post-inspection",
        schedule: &["swap", "inspect", "bind", "effect"],
        result: "complete",
    },
    ExpectedCase {
        id: "entry_swap_after_inspection",
        mode: "internal-fixture",
        scenario: "entry-swap-after-inspection",
        strategy: "post-inspection",
        schedule: &["inspect", "swap", "bind", "effect"],
        result: "rejected",
    },
    ExpectedCase {
        id: "entry_swap_after_binding",
        mode: "model-only",
        scenario: "entry-swap-after-binding",
        strategy: "post-inspection",
        schedule: &["inspect", "bind", "swap", "effect"],
        result: "complete",
    },
    ExpectedCase {
        id: "retained_wrapper_swap",
        mode: "internal-fixture",
        scenario: "retained-wrapper-swap",
        strategy: "retained",
        schedule: &["inspect", "swap", "bind", "effect"],
        result: "complete",
    },
];

fn parse_corpus(text: &str) -> Result<BTreeMap<String, OracleCase>, String> {
    let mut cases = BTreeMap::new();
    let mut ids = HashSet::new();
    for (index, line) in text.lines().enumerate() {
        let case: OracleCase =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", index + 1))?;
        let expected = EXPECTED_CASES
            .iter()
            .find(|expected| expected.id == case.id)
            .ok_or_else(|| format!("unknown case id: {}", case.id))?;
        if case.schema != 1
            || case.mode != expected.mode
            || case.scenario != expected.scenario
            || case.strategy != expected.strategy
            || !case
                .schedule
                .iter()
                .map(String::as_str)
                .eq(expected.schedule.iter().copied())
            || case.result != expected.result
            || case.outside_writable
        {
            return Err(format!("case contract mismatch: {}", case.id));
        }
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate case id: {}", case.id));
        }
        cases.insert(case.id.clone(), case);
    }
    for expected in EXPECTED_CASES {
        if !cases.contains_key(expected.id) {
            return Err(format!("missing case id: {}", expected.id));
        }
    }
    if cases.len() != EXPECTED_CASES.len() {
        return Err(format!(
            "expected {} cases, found {}",
            EXPECTED_CASES.len(),
            cases.len()
        ));
    }
    Ok(cases)
}

#[test]
fn cleanup_capability_corpus_contract_is_exact() {
    parse_corpus(CORPUS).unwrap();

    let crossed_mode = CORPUS.replacen("\"mode\":\"strict\"", "\"mode\":\"model-only\"", 1);
    assert!(parse_corpus(&crossed_mode).is_err());

    let unknown = CORPUS.replacen("\"id\":\"stable_entry\"", "\"id\":\"unknown\"", 1);
    assert!(parse_corpus(&unknown).is_err());

    let duplicate = format!("{CORPUS}{}\n", CORPUS.lines().next().unwrap());
    assert!(parse_corpus(&duplicate).is_err());

    let unknown_field = CORPUS.replacen("\"schema\":1", "\"extra\":false,\"schema\":1", 1);
    assert!(parse_corpus(&unknown_field).is_err());
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use camino::{Utf8Path, Utf8PathBuf};
    use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};

    use super::{OracleCase, parse_corpus};
    use crate::workspace::root::{WorkspaceRaceHook, install_workspace_race_hook};
    use crate::workspace::{CopyOptions, WorkerWorkspace, WorkspaceError, WorkspacePlan};

    struct Fixture {
        _project: tempfile::TempDir,
        worker: WorkerWorkspace,
    }

    impl Fixture {
        fn new() -> Self {
            let project = tempfile::tempdir().unwrap();
            let package = project.path().join("pkg");
            fs::create_dir(&package).unwrap();
            fs::write(package.join("a.py"), b"original\n").unwrap();
            let project_root = Utf8Path::from_path(project.path()).unwrap();
            let plan =
                WorkspacePlan::preflight(project_root, EffectId(35_700), 1, CopyOptions::default())
                    .unwrap();
            let mut ledger = BudgetLedger::new(RunBudgets {
                memory: 1,
                copy: plan.aggregate_bytes(),
                processes: 1,
            });
            let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
            let worker = plan
                .create_worker(&grant.create_worker(EffectId(35_701), 0).unwrap())
                .unwrap();
            Self {
                _project: project,
                worker,
            }
        }
    }

    struct CleanupPause {
        operation: &'static str,
        path: Utf8PathBuf,
        fired: AtomicBool,
        entered: SyncSender<()>,
        resume: Mutex<Receiver<()>>,
    }

    impl CleanupPause {
        fn new(
            operation: &'static str,
            path: Utf8PathBuf,
        ) -> (Arc<Self>, Receiver<()>, SyncSender<()>) {
            let (entered_tx, entered_rx) = sync_channel(0);
            let (resume_tx, resume_rx) = sync_channel(0);
            (
                Arc::new(Self {
                    operation,
                    path,
                    fired: AtomicBool::new(false),
                    entered: entered_tx,
                    resume: Mutex::new(resume_rx),
                }),
                entered_rx,
                resume_tx,
            )
        }
    }

    impl WorkspaceRaceHook for CleanupPause {
        fn parent_opened(&self, operation: &'static str, path: &Utf8Path) {
            if operation == self.operation
                && path == self.path
                && !self.fired.swap(true, Ordering::SeqCst)
            {
                self.entered.send(()).unwrap();
                self.resume
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
        }
    }

    fn case(id: &str) -> OracleCase {
        parse_corpus(super::CORPUS).unwrap().remove(id).unwrap()
    }

    fn set_mode(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn is_owner_writable(path: &Path) -> bool {
        fs::metadata(path).unwrap().permissions().mode() & 0o200 != 0
    }

    fn permission_fingerprint(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn result_name(result: &Result<(), WorkspaceError>) -> &'static str {
        if result.is_ok() {
            "complete"
        } else {
            "rejected"
        }
    }

    fn cleanup_in_thread(
        mut worker: WorkerWorkspace,
        hook: Arc<CleanupPause>,
    ) -> thread::JoinHandle<(WorkerWorkspace, Result<(), WorkspaceError>)> {
        thread::spawn(move || {
            let _guard = install_workspace_race_hook(hook);
            let result = worker.try_cleanup();
            (worker, result)
        })
    }

    #[test]
    fn strict_cleanup_cases_match_the_lean_oracle() {
        let stable = case("stable_entry");
        let stable_outside = tempfile::NamedTempFile::new().unwrap();
        set_mode(stable_outside.path(), 0o400);
        let mut fixture = Fixture::new();
        let result = fixture.worker.try_cleanup();
        assert_eq!(result_name(&result), stable.result);
        assert_eq!(
            is_owner_writable(stable_outside.path()),
            stable.outside_writable
        );
        result.unwrap();

        let preexisting = case("preexisting_outside_link");
        let preexisting_outside = tempfile::NamedTempFile::new().unwrap();
        set_mode(preexisting_outside.path(), 0o400);
        let mut fixture = Fixture::new();
        symlink(
            preexisting_outside.path(),
            fixture.worker.root().join("outside-link"),
        )
        .unwrap();
        let result = fixture.worker.try_cleanup();
        assert_eq!(result_name(&result), preexisting.result);
        assert_eq!(
            is_owner_writable(preexisting_outside.path()),
            preexisting.outside_writable
        );
        result.unwrap();
    }

    #[test]
    fn entry_swap_after_inspection_matches_the_lean_oracle() {
        let expected = case("entry_swap_after_inspection");
        let outside = tempfile::NamedTempFile::new().unwrap();
        set_mode(outside.path(), 0o400);
        let fixture = Fixture::new();
        let target = fixture.worker.root().join("race-target");
        fs::write(&target, b"owned\n").unwrap();
        set_mode(target.as_std_path(), 0o400);
        let (hook, entered, resume) = CleanupPause::new("cleanup-entry", target.clone());
        let cleanup = cleanup_in_thread(fixture.worker, hook);

        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        fs::remove_file(&target).unwrap();
        symlink(outside.path(), &target).unwrap();
        resume.send(()).unwrap();

        let (mut worker, result) = cleanup.join().unwrap();
        let actual_result = result_name(&result);
        let actual_outside_writable = is_owner_writable(outside.path());
        if result.is_err() {
            worker.try_cleanup().unwrap();
        }

        assert_eq!(
            (actual_result, actual_outside_writable),
            (expected.result.as_str(), expected.outside_writable)
        );
    }

    #[test]
    fn retained_wrapper_swap_matches_the_lean_permission_observation() {
        let expected = case("retained_wrapper_swap");
        let outside_owner = tempfile::tempdir().unwrap();
        let outside = outside_owner.path().join("outside");
        fs::create_dir(&outside).unwrap();
        set_mode(&outside, 0o300);
        let outside_permissions = permission_fingerprint(&outside);
        let fixture = Fixture::new();
        let wrapper = fixture
            .worker
            .root()
            .parent()
            .unwrap()
            .as_std_path()
            .to_owned();
        let wrapper_name = wrapper.file_name().unwrap().to_string_lossy();
        let held = wrapper
            .parent()
            .unwrap()
            .join(format!(".{wrapper_name}-issue-357-held"));
        set_mode(&wrapper, 0o300);
        let hook_path = Utf8PathBuf::from_path_buf(wrapper.clone()).unwrap();
        let (hook, entered, resume) = CleanupPause::new("cleanup-wrapper", hook_path);
        let cleanup = cleanup_in_thread(fixture.worker, hook);

        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        fs::rename(&wrapper, &held).unwrap();
        symlink(&outside, &wrapper).unwrap();
        resume.send(()).unwrap();

        let (mut worker, result) = cleanup.join().unwrap();
        let actual_outside_changed = permission_fingerprint(&outside) != outside_permissions;
        if result.is_err() {
            if fs::symlink_metadata(&wrapper).is_ok() {
                fs::remove_file(&wrapper).unwrap();
            }
            fs::rename(&held, &wrapper).unwrap();
            worker.try_cleanup().unwrap();
        } else if held.exists() {
            set_mode(&held, 0o700);
            fs::remove_dir_all(&held).unwrap();
        }
        set_mode(&outside, 0o700);

        assert_eq!(actual_outside_changed, expected.outside_writable);
    }

    #[test]
    fn final_removal_failure_is_retryable_without_reopening_cleanup_capabilities() {
        // A privileged process can traverse mode-000 directories, so it cannot exercise this
        // deterministic final-removal failure.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let fixture = Fixture::new();
        let wrapper = fixture
            .worker
            .root()
            .parent()
            .unwrap()
            .as_std_path()
            .to_owned();
        let hook_path = Utf8PathBuf::from_path_buf(wrapper.clone()).unwrap();
        let (hook, entered, resume) = CleanupPause::new("cleanup-final", hook_path);
        let cleanup = cleanup_in_thread(fixture.worker, hook);

        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        set_mode(&wrapper, 0o000);
        resume.send(()).unwrap();

        let (mut worker, result) = cleanup.join().unwrap();
        assert!(
            result.is_err(),
            "inaccessible wrapper must block final removal"
        );
        set_mode(&wrapper, 0o700);
        worker.try_cleanup().unwrap();
        assert!(!wrapper.exists());
    }
}
