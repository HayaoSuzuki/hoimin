use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    BudgetLedger, EffectFailed, EffectId, IntegrityCheckpoint, Preflight, PreflightCompleted,
    ResourceMode, RunBudgets, RunConfig, VerifyOriginals, reserve_workspace_copy,
};
use serde::Deserialize;

use crate::cli::{ParsedCommand, parse_from};
use crate::fingerprint_inputs::{
    FingerprintInputError, FingerprintInputRecheckError, recheck_config,
};
use crate::plan::{VerifiedExecution, create, prepare_verify};
use crate::target::TargetHandler;
use crate::workspace::{CopyOptions, WorkspaceHandler, WorkspaceManifest};

use super::super::{prepare_manifest_fingerprint, recheck_fingerprint_inputs};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/fingerprint-preflight.jsonl");
type Files = BTreeMap<Utf8PathBuf, String>;

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    prepared: Files,
    excludes: Vec<String>,
    glob: String,
    exact: Vec<String>,
    expected_records: Files,
    copied_at_preparation: BTreeSet<Utf8PathBuf>,
    before_copy: Vec<Edit>,
    before_validation: Vec<Edit>,
    after_validation: Vec<Edit>,
    snapshot: Files,
    outcome: Outcome,
}

#[derive(Deserialize)]
struct Edit {
    path: Utf8PathBuf,
    content: Option<String>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Published,
    GlobUnmatched,
    ExactMissing,
    LiveRecordsChanged,
    SnapshotChanged,
    SourceMissing,
    OriginalsChanged,
}

fn apply_edits(root: &Utf8Path, edits: &[Edit]) {
    for edit in edits {
        let path = root.join(&edit.path);
        match &edit.content {
            Some(content) => fs::write(path, content).unwrap(),
            None => fs::remove_file(path).unwrap(),
        }
    }
}

async fn prepare(root: &Utf8Path, manifest: &Utf8Path, case: &Case) -> VerifiedExecution {
    let mut args = [
        "hoimin",
        "plan",
        "--root",
        root.as_str(),
        "--file",
        "src/calc.py",
        "--operators",
        "binary_add_sub",
        "--allow-best-effort-memory",
        "--fingerprint-include",
        &case.glob,
    ]
    .map(OsString::from)
    .to_vec();
    for exclude in &case.excludes {
        args.extend(["--exclude", exclude].map(OsString::from));
    }
    for exact in &case.exact {
        args.extend(["--fingerprint-file", exact].map(OsString::from));
    }
    args.extend(["--", "unused-test-command"].map(OsString::from));
    let ParsedCommand::Plan(args) = parse_from(args).unwrap() else {
        panic!("{}: expected a plan", case.id);
    };
    let (config, discovery) = args.into_configs().unwrap();
    let output = create(config, discovery)
        .await
        .unwrap_or_else(|err| panic!("{}: {err}", case.id));
    let [candidate] = output.manifest.candidates.as_slice() else {
        panic!("{}: expected one candidate", case.id);
    };
    fs::write(manifest, serde_json::to_vec(&output.manifest).unwrap()).unwrap();
    prepare_verify(manifest, std::slice::from_ref(&candidate.id))
        .await
        .unwrap_or_else(|err| panic!("{}: {err}", case.id))
        .into_execution()
}

fn classify_fingerprint_failure(
    config: &RunConfig,
    root: &Utf8Path,
    err: &EffectFailed,
) -> Outcome {
    assert_eq!(err.failure.code(), "plan.fingerprint_input.changed");
    match recheck_config(config, root) {
        Ok(()) => Outcome::SnapshotChanged,
        Err(live_error) => {
            assert_eq!(err.failure.message(), live_error.to_string());
            match live_error {
                FingerprintInputRecheckError::Resolution(FingerprintInputError::Unmatched(_)) => {
                    Outcome::GlobUnmatched
                }
                FingerprintInputRecheckError::Resolution(FingerprintInputError::NotFound(_)) => {
                    Outcome::ExactMissing
                }
                FingerprintInputRecheckError::RecordsChanged => Outcome::LiveRecordsChanged,
                unexpected => panic!("unexpected live error: {unexpected}"),
            }
        }
    }
}

fn check_pending_snapshot(manifest: &WorkspaceManifest, case: &Case) {
    assert_eq!(
        manifest.entries().len(),
        case.snapshot.len(),
        "{} pending snapshot",
        case.id
    );
    for (path, content) in &case.snapshot {
        let entry = manifest
            .entry(path)
            .unwrap_or_else(|| panic!("{} missing {path} in pending snapshot", case.id));
        assert_eq!(
            entry.blake3,
            blake3::hash(content.as_bytes()),
            "{}",
            case.id
        );
        assert_eq!(entry.size, content.len() as u64, "{}", case.id);
    }
}

fn check_published(workspace: &mut WorkspaceHandler, preflight: &PreflightCompleted, case: &Case) {
    let bytes = case
        .snapshot
        .values()
        .map(|content| content.len() as u64)
        .sum();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: bytes,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, preflight).unwrap();
    workspace
        .handle_create_worker(grant.create_worker(EffectId(2), 0).unwrap())
        .unwrap_or_else(|err| panic!("{}: {err:?}", case.id));
    let root = workspace.worker(0).unwrap().root();
    let paths = case
        .prepared
        .keys()
        .chain(
            case.before_copy
                .iter()
                .chain(&case.before_validation)
                .chain(&case.after_validation)
                .map(|edit| &edit.path),
        )
        .collect::<BTreeSet<_>>();
    for path in paths {
        if let Some(content) = case.snapshot.get(path) {
            assert_eq!(
                &fs::read_to_string(root.join(path)).unwrap(),
                content,
                "{}",
                case.id
            );
        } else {
            assert!(
                !root.join(path).exists(),
                "{} unexpectedly copied {path}",
                case.id
            );
        }
    }
}

async fn check_case(case: &Case) {
    let temp = tempfile::tempdir().unwrap();
    let base = Utf8Path::from_path(temp.path()).unwrap();
    let root = base.join("project");
    fs::create_dir_all(root.join("src")).unwrap();
    for (path, content) in &case.prepared {
        fs::write(root.join(path), content).unwrap();
    }
    let verified = prepare(&root, &base.join("saved.json"), case).await;
    assert_eq!(
        verified.fingerprint_copy_inputs, case.copied_at_preparation,
        "{}",
        case.id
    );
    let config = verified.config;
    let expected = case
        .expected_records
        .iter()
        .map(|(path, content)| (path, blake3::hash(content.as_bytes()).to_hex().to_string()))
        .collect::<Vec<_>>();
    let records = config
        .fingerprint_inputs
        .iter()
        .map(|record| (&record.path, record.hash.clone()))
        .collect::<Vec<_>>();
    assert_eq!(records, expected, "{} protected input records", case.id);

    let targets = TargetHandler::resolve(&config.selection).await.unwrap();
    let mut workspace = WorkspaceHandler::new(
        root.clone(),
        Vec::new(),
        1,
        CopyOptions {
            includes: config.selection.includes.clone(),
            excludes: config.selection.excludes.clone(),
            literal_exclusions: Vec::new(),
        },
    );
    apply_edits(&root, &case.before_copy);
    let id = EffectId(1);
    let mut callback_called = false;
    let mut fingerprint_failure = None;
    let result = workspace.handle_preflight_validated(Preflight { id }, |root, manifest| {
        callback_called = true;
        check_pending_snapshot(manifest, case);
        apply_edits(root, &case.before_validation);
        if let Err(err) = recheck_fingerprint_inputs(
            &config,
            root,
            manifest,
            &verified.fingerprint_copy_inputs,
            id,
        ) {
            fingerprint_failure = Some(classify_fingerprint_failure(&config, root, &err));
            return Err(err);
        }
        prepare_manifest_fingerprint(&config, &targets, ResourceMode::BestEffort, manifest, id)?;
        apply_edits(root, &case.after_validation);
        Ok(())
    });
    assert!(callback_called, "{}: {result:?}", case.id);
    let outcome = match result {
        Ok(preflight) => {
            assert_eq!(preflight.id, id, "{}", case.id);
            check_published(&mut workspace, &preflight, case);
            Outcome::Published
        }
        Err(err) => {
            assert_eq!(err.id, id, "{}", case.id);
            assert_eq!(workspace.worker_count(), 0, "{}", case.id);
            let unpublished = workspace
                .handle_verify_originals(VerifyOriginals {
                    id,
                    checkpoint: IntegrityCheckpoint::PreAnalysis,
                })
                .unwrap_err();
            assert_eq!(
                unpublished.failure.code(),
                "workspace.worker.missing",
                "{}",
                case.id
            );
            match err.failure.code() {
                "plan.fingerprint_input.changed" => fingerprint_failure.unwrap(),
                "fingerprint.source.manifest_missing" => Outcome::SourceMissing,
                "workspace.original.changed" => Outcome::OriginalsChanged,
                unexpected => panic!("{}: {unexpected}: {err:?}", case.id),
            }
        }
    };
    assert_eq!(outcome, case.outcome, "{}", case.id);
}

#[tokio::test]
async fn fingerprint_preflight_matches_lean_oracle() {
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        check_case(&case).await;
    }
}
