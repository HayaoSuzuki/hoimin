use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::analyzer::CandidateStore;
use hoimin_cli::session::SessionHandler;
use hoimin_cli::workspace::{CopyOptions, WorkerWorkspace, WorkspaceError, WorkspacePlan};
use hoimin_core::{
    ApplyMutation, BeginSession, BudgetLedger, ByteSpan, CandidateCursor, CandidateDescriptor,
    CandidateIdentity, CandidateValidationError, EffectId, LookupStoredResult, MutantResult,
    MutationCandidate, MutationStatus, PersistResult, ResourceMode, RunBudgets, RunFingerprint,
    reserve_workspace_copy, stable_mutant_id, validate_candidate,
};
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/candidate-span-preservation.jsonl");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateInput {
    schema: u32,
    path: Utf8PathBuf,
    start: u64,
    length: u64,
    original: Vec<u8>,
    replacement: Vec<u8>,
    operator: String,
    line: u32,
    column: u32,
    symbol: Option<String>,
    hash_mode: String,
    sequence: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u32,
    id: String,
    mode: String,
    scenario: String,
    source: Vec<u8>,
    boundaries: Vec<bool>,
    scalar_starts: Vec<bool>,
    maximum_offset: u64,
    candidate: CandidateInput,
    expected_valid: bool,
    expected_bytes: Vec<u8>,
    rejection: Option<String>,
}

fn expected_contract() -> BTreeMap<&'static str, (&'static str, &'static str)> {
    [
        ("ascii_public", ("strict", "apply")),
        ("multiline_public", ("strict", "apply")),
        ("multibyte_public", ("strict", "apply")),
        ("protocol_plan_roundtrip", ("internal-fixture", "transport")),
        (
            "spool_machine_session_projection",
            ("internal-fixture", "transport"),
        ),
        ("stale_hash", ("internal-fixture", "reject")),
        ("stale_original", ("internal-fixture", "reject")),
        ("offset_overflow", ("internal-fixture", "reject")),
        ("non_boundary", ("model-only", "reject")),
        ("bad_location", ("internal-fixture", "reject")),
        ("reset_before_second", ("internal-fixture", "reset")),
        ("fixture_failure", ("infrastructure-error", "harness")),
    ]
    .into_iter()
    .collect()
}

fn parse_corpus(input: &str) -> Vec<OracleCase> {
    let cases = input
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str::<OracleCase>(line)
                .unwrap_or_else(|error| panic!("line {}: {error}", index + 1))
        })
        .collect::<Vec<_>>();
    let contract = cases
        .iter()
        .map(|case| {
            (
                case.id.as_str(),
                (case.mode.as_str(), case.scenario.as_str()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(contract, expected_contract());
    assert_eq!(contract.len(), cases.len());
    cases.iter().for_each(validate_case_shape);
    cases
}

fn validate_case_shape(case: &OracleCase) {
    assert_eq!(case.schema, 1, "{}", case.id);
    assert_eq!(case.candidate.schema, 1, "{}", case.id);
    assert_eq!(case.maximum_offset, u64::MAX, "{}", case.id);
    assert_eq!(case.boundaries.len(), case.source.len() + 1, "{}", case.id);
    assert_eq!(case.scalar_starts.len(), case.source.len(), "{}", case.id);
    assert_eq!(case.boundaries.first(), Some(&true), "{}", case.id);
    assert_eq!(case.boundaries.last(), Some(&true), "{}", case.id);
    assert_eq!(case.candidate.sequence, 1, "{}", case.id);
    assert!(
        matches!(case.candidate.hash_mode.as_str(), "current" | "stale"),
        "{}",
        case.id
    );
    match (case.mode.as_str(), case.scenario.as_str()) {
        ("strict", "apply") | ("internal-fixture", "transport" | "reset") => {
            assert!(case.expected_valid, "{}", case.id);
            assert_eq!(case.rejection, None, "{}", case.id);
            assert_eq!(case.candidate.hash_mode, "current", "{}", case.id);
            assert_eq!(
                reference_replacement(case),
                case.expected_bytes,
                "{}",
                case.id
            );
        }
        ("internal-fixture" | "model-only", "reject") => {
            assert!(!case.expected_valid, "{}", case.id);
            assert!(case.rejection.is_some(), "{}", case.id);
            assert_eq!(case.expected_bytes, case.source, "{}", case.id);
        }
        ("infrastructure-error", "harness") => {
            assert!(!case.expected_valid, "{}", case.id);
            assert_eq!(case.expected_bytes, Vec::<u8>::new(), "{}", case.id);
            assert_eq!(case.rejection.as_deref(), Some("fixture"), "{}", case.id);
        }
        _ => panic!("crossed mode/scenario in {}", case.id),
    }
}

fn reference_replacement(case: &OracleCase) -> Vec<u8> {
    let start = usize::try_from(case.candidate.start).expect("valid case start");
    let end = start + usize::try_from(case.candidate.length).expect("valid case length");
    let mut bytes = case.source[..start].to_vec();
    bytes.extend_from_slice(&case.candidate.replacement);
    bytes.extend_from_slice(&case.source[end..]);
    bytes
}

fn descriptor(case: &OracleCase) -> Option<CandidateDescriptor> {
    Some(CandidateDescriptor {
        schema_version: case.candidate.schema,
        path: case.candidate.path.clone(),
        span: ByteSpan {
            start: case.candidate.start,
            length: case.candidate.length,
        },
        original: String::from_utf8(case.candidate.original.clone()).ok()?,
        replacement: String::from_utf8(case.candidate.replacement.clone()).ok()?,
        operator: case.candidate.operator.clone(),
        line: case.candidate.line,
        column: case.candidate.column,
        symbol: case.candidate.symbol.clone(),
        file_hash: if case.candidate.hash_mode == "current" {
            blake3::hash(&case.source).to_hex().to_string()
        } else {
            "0".repeat(64)
        },
    })
}

fn mutation_candidate(case: &OracleCase) -> Option<MutationCandidate> {
    let descriptor = descriptor(case)?;
    let id = stable_mutant_id(&CandidateIdentity::from(&descriptor)).to_string();
    Some(MutationCandidate {
        id,
        sequence: case.candidate.sequence,
        path: descriptor.path,
        span: descriptor.span,
        original: descriptor.original,
        replacement: descriptor.replacement,
        operator: descriptor.operator,
        line: descriptor.line,
        column: descriptor.column,
        symbol: descriptor.symbol,
        file_hash: descriptor.file_hash,
    })
}

#[test]
fn candidate_span_corpus_is_closed_and_typed() {
    assert_eq!(parse_corpus(CORPUS).len(), 12);
}

#[test]
fn candidate_span_corpus_rejects_unknown_crossed_and_ignored_fields() {
    let rows = CORPUS
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mut unknown = rows.clone();
    unknown[0]["future"] = serde_json::json!(true);
    assert!(std::panic::catch_unwind(|| parse_corpus(&render_rows(unknown))).is_err());
    let mut crossed = rows.clone();
    crossed[0]["mode"] = serde_json::json!("model-only");
    assert!(std::panic::catch_unwind(|| parse_corpus(&render_rows(crossed))).is_err());
    let mut duplicate = rows.clone();
    duplicate[0]["id"] = duplicate[1]["id"].clone();
    assert!(std::panic::catch_unwind(|| parse_corpus(&render_rows(duplicate))).is_err());
    let mut ignored = rows;
    ignored[11]["expected_bytes"] = serde_json::json!([1]);
    assert!(std::panic::catch_unwind(|| parse_corpus(&render_rows(ignored))).is_err());
}

fn render_rows(rows: Vec<serde_json::Value>) -> String {
    let mut output = String::new();
    for row in rows {
        output.push_str(&row.to_string());
        output.push('\n');
    }
    output
}

#[test]
fn owned_validator_matches_all_representable_lean_rows() {
    for case in parse_corpus(CORPUS)
        .iter()
        .filter(|case| case.scenario != "harness" && case.mode != "model-only")
    {
        let descriptor = descriptor(case).expect("Rust-representable descriptor");
        let result = validate_candidate(&case.source, &descriptor);
        assert_eq!(result.is_ok(), case.expected_valid, "{}", case.id);
        if let Err(error) = result {
            let expected = match case.rejection.as_deref() {
                Some("file_hash") => CandidateValidationError::FileHashMismatch,
                Some("original") => CandidateValidationError::OriginalMismatch,
                Some("span") => CandidateValidationError::SpanOutOfBounds,
                Some("location") => CandidateValidationError::LocationMismatch,
                rejection => panic!("unmapped rejection {rejection:?}"),
            };
            assert_eq!(error, expected, "{}", case.id);
        }
    }
}

#[test]
fn serde_spool_machine_and_session_preserve_the_lean_identity_projection() {
    let case = parse_corpus(CORPUS)
        .into_iter()
        .find(|case| case.id == "spool_machine_session_projection")
        .unwrap();
    let candidate = mutation_candidate(&case).unwrap();
    let encoded = serde_json::to_vec(&candidate).unwrap();
    let decoded: MutationCandidate = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, candidate);

    let directory = tempfile::tempdir().unwrap();
    let mut store = CandidateStore::new_in(1, directory.path()).unwrap();
    store.push(&candidate).unwrap();
    let reference = store.finish().unwrap();
    let (spooled, cursor) = CandidateStore::replay_one(&reference, CandidateCursor::START)
        .unwrap()
        .unwrap();
    assert_eq!(spooled, candidate);
    assert!(
        CandidateStore::replay_one(&reference, cursor)
            .unwrap()
            .is_none()
    );

    let effect = ApplyMutation {
        id: EffectId(3),
        worker: 0,
        candidate: spooled,
    };
    let effect: ApplyMutation =
        serde_json::from_slice(&serde_json::to_vec(&effect).unwrap()).unwrap();
    assert_eq!(effect.candidate, candidate);

    let database = directory.path().join("session.sqlite3");
    let mut session = SessionHandler::open(database).unwrap();
    session
        .begin(BeginSession {
            id: EffectId(4),
            run_id: "candidate-span".to_owned(),
            fingerprint: RunFingerprint::from_bytes([7; 32]),
        })
        .unwrap();
    session
        .persist(&PersistResult {
            id: EffectId(5),
            worker: 0,
            result: MutantResult {
                run_id: "candidate-span".to_owned(),
                candidate: candidate.clone(),
                status: MutationStatus::Killed,
                termination: None,
                elapsed: Duration::from_millis(1),
                resource_mode: ResourceMode::Hard,
                output: None,
                diagnostics: Vec::new(),
            },
        })
        .unwrap();
    let loaded = session
        .lookup(&LookupStoredResult {
            id: EffectId(6),
            worker: 0,
            run_id: "candidate-span".to_owned(),
            mutant_id: candidate.id.clone(),
        })
        .unwrap();
    assert_eq!(loaded.result.unwrap().mutant_id, candidate.id);
}

#[test]
fn production_stable_id_is_sensitive_to_every_identity_field_only() {
    let case = parse_corpus(CORPUS)
        .into_iter()
        .find(|case| case.id == "ascii_public")
        .unwrap();
    let descriptor = descriptor(&case).unwrap();
    let identity = CandidateIdentity::from(&descriptor);
    let baseline = stable_mutant_id(&identity);
    let variants = [
        CandidateIdentity {
            schema_version: 2,
            ..identity.clone()
        },
        CandidateIdentity {
            file_hash: "1".repeat(64),
            ..identity.clone()
        },
        CandidateIdentity {
            path: "src/other.py".into(),
            ..identity.clone()
        },
        CandidateIdentity {
            span: ByteSpan {
                start: identity.span.start + 1,
                ..identity.span
            },
            ..identity.clone()
        },
        CandidateIdentity {
            span: ByteSpan {
                length: identity.span.length + 1,
                ..identity.span
            },
            ..identity.clone()
        },
        CandidateIdentity {
            operator: "other".to_owned(),
            ..identity.clone()
        },
        CandidateIdentity {
            replacement: "*".to_owned(),
            ..identity.clone()
        },
    ];
    for variant in variants {
        assert_ne!(stable_mutant_id(&variant), baseline);
    }
    let mut metadata = descriptor;
    metadata.line += 1;
    metadata.column += 1;
    metadata.symbol = Some("other".to_owned());
    assert_eq!(
        stable_mutant_id(&CandidateIdentity::from(&metadata)),
        baseline
    );
}

struct Project {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn from_cases(cases: &[OracleCase]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        for case in cases {
            let path = root.join(case.candidate.path.as_std_path());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, &case.source).unwrap();
        }
        Self {
            _directory: directory,
            root,
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn python_executable() -> PathBuf {
    if cfg!(windows) {
        repo_root().join(".venv/Scripts/python.exe")
    } else {
        repo_root().join(".venv/bin/python")
    }
}

#[tokio::test]
async fn public_plan_and_verify_preserve_all_strict_candidate_fields() {
    let cases = parse_corpus(CORPUS)
        .into_iter()
        .filter(|case| case.mode == "strict")
        .collect::<Vec<_>>();
    let project = Project::from_cases(&cases);
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("plan"),
        OsString::from("--root"),
        project.root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--operators"),
        OsString::from("binary_add_sub"),
        OsString::from("--jobs"),
        OsString::from("1"),
        OsString::from("--allow-best-effort-memory"),
    ];
    for case in &cases {
        args.extend([
            OsString::from("--file"),
            case.candidate.path.as_os_str().to_owned(),
        ]);
    }
    args.extend([
        OsString::from("--"),
        python_executable().into_os_string(),
        OsString::from("-c"),
        OsString::from("pass"),
    ]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let candidates = manifest["candidates"].as_array().unwrap();
    let mut selected_ids = Vec::new();
    for case in &cases {
        let actual = candidates
            .iter()
            .find(|candidate| {
                candidate["path"] == case.candidate.path.as_str()
                    && candidate["span"]["start"] == case.candidate.start
                    && candidate["span"]["length"] == case.candidate.length
                    && candidate["operator"] == case.candidate.operator
            })
            .unwrap_or_else(|| panic!("missing strict candidate {}", case.id));
        assert_public_candidate(actual, case);
        selected_ids.push(actual["id"].as_str().unwrap().to_owned());
    }

    let plan = project.root.join("plan.json");
    std::fs::write(&plan, &stdout).unwrap();
    let mut verify = vec![
        OsString::from("hoimin"),
        OsString::from("verify"),
        plan.into_os_string(),
        OsString::from("--format"),
        OsString::from("json"),
    ];
    for id in &selected_ids {
        verify.extend([OsString::from("--candidate"), OsString::from(id)]);
    }
    stdout.clear();
    stderr.clear();
    let code = hoimin_cli::run_with_io(verify, &mut stdout, &mut stderr).await;
    assert_eq!(code, 1, "stderr={}", String::from_utf8_lossy(&stderr));
    let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let mutants = report["mutants"].as_array().unwrap();
    assert_eq!(mutants.len(), selected_ids.len());
    assert_eq!(
        mutants
            .iter()
            .map(|item| item["candidate"]["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        selected_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
    );
    for case in &cases {
        let actual = mutants
            .iter()
            .find(|item| item["candidate"]["path"] == case.candidate.path.as_str())
            .unwrap();
        assert_public_candidate(&actual["candidate"], case);
    }
}

fn assert_public_candidate(actual: &serde_json::Value, case: &OracleCase) {
    assert_eq!(actual["path"], case.candidate.path.as_str());
    assert_eq!(actual["span"]["start"], case.candidate.start);
    assert_eq!(actual["span"]["length"], case.candidate.length);
    assert_eq!(
        actual["original"],
        String::from_utf8(case.candidate.original.clone()).unwrap()
    );
    assert_eq!(
        actual["replacement"],
        String::from_utf8(case.candidate.replacement.clone()).unwrap()
    );
    assert_eq!(actual["operator"], case.candidate.operator);
    assert_eq!(actual["line"], u64::from(case.candidate.line));
    assert_eq!(actual["column"], u64::from(case.candidate.column));
    assert_eq!(
        actual["symbol"],
        serde_json::to_value(&case.candidate.symbol).unwrap()
    );
}

fn create_worker(project: &Project) -> WorkerWorkspace {
    let root = Utf8Path::from_path(&project.root).unwrap();
    let plan = WorkspacePlan::preflight(root, EffectId(20), 1, CopyOptions::default()).unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: plan.aggregate_bytes(),
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
    plan.create_worker(&grant.create_worker(EffectId(21), 0).unwrap())
        .unwrap()
}

#[test]
fn workspace_matches_apply_reject_unchanged_and_reset_rows() {
    let cases = parse_corpus(CORPUS);
    let represented = cases
        .iter()
        .filter(|case| case.mode != "model-only" && case.mode != "infrastructure-error")
        .cloned()
        .collect::<Vec<_>>();
    let project = Project::from_cases(&represented);
    let mut worker = create_worker(&project);
    for case in &represented {
        let candidate = mutation_candidate(case).unwrap();
        let before = worker.read(&candidate.path).unwrap();
        let result = worker.apply_mutation(&candidate);
        assert_eq!(result.is_ok(), case.expected_valid, "{}", case.id);
        let after = worker.read(&candidate.path).unwrap();
        assert_eq!(after, case.expected_bytes, "{}", case.id);
        if result.is_err() {
            assert_eq!(after, before, "{} changed bytes on rejection", case.id);
            if case.rejection.as_deref() == Some("location") {
                assert!(matches!(
                    result,
                    Err(WorkspaceError::MutationSpanInvalid { .. })
                ));
            }
        }
        worker.reset().unwrap();
    }

    let ascii = cases.iter().find(|case| case.id == "ascii_public").unwrap();
    let first = mutation_candidate(ascii).unwrap();
    worker.apply_mutation(&first).unwrap();
    worker.reset().unwrap();
    let mut second = first.clone();
    second.span = ByteSpan {
        start: 8,
        length: 1,
    };
    second.original = "2".to_owned();
    second.replacement = "3".to_owned();
    second.column = 8;
    worker.apply_mutation(&second).unwrap();
    assert_eq!(worker.read(&second.path).unwrap(), b"x = 1 + 3\n");
}
