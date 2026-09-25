//! Public correspondence for the Lean prepared-namespace policy. Expected
//! identity and candidate eligibility come only from the generated corpus.
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/prepared-annotation-import.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    id: String,
    mode: String,
    source: String,
    original: String,
    replacement: String,
    symbol: Option<String>,
    runtime_identity: [bool; 2],
    candidate_count: usize,
    run_check: bool,
    runtime_allowed: bool,
}

fn parse_cases(input: &str) -> Result<Vec<Case>, String> {
    let cases: Vec<Case> = input
        .lines()
        .map(|line| serde_json::from_str(line).map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;
    let mut ids = BTreeSet::new();
    for case in &cases {
        if case.schema != 1
            || case.mode != "strict"
            || case.id.is_empty()
            || !ids.insert(case.id.as_str())
            || case.source.is_empty()
            || case.original.is_empty()
            || case.replacement.is_empty()
            || case.source.matches("# target").count() != 1
            || case.candidate_count > 1
        {
            return Err(format!("invalid corpus case: {}", case.id));
        }
    }
    let mut expected_ids: BTreeSet<String> = [
        "plain",
        "global-both",
        "global-source",
        "global-destination",
        "explicit-import",
        "nonlocal",
        "generic-method",
        "integer-destination",
        "qualified-global",
        "qualified-prepared",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for scope in ["module", "class", "method", "lexical"] {
        for source in [false, true] {
            for destination in [false, true] {
                expected_ids.insert(format!("{scope}-{source}-{destination}"));
            }
        }
    }
    if ids.into_iter().map(str::to_owned).collect::<BTreeSet<_>>() != expected_ids {
        return Err("unexpected prepared annotation import case identities".into());
    }
    if cases.len() != 26 {
        return Err("incomplete prepared annotation import corpus".into());
    }
    Ok(cases)
}

fn target_line(case: &Case) -> u32 {
    u32::try_from(
        case.source
            .lines()
            .position(|line| line.contains("# target"))
            .unwrap()
            + 1,
    )
    .unwrap()
}

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

fn command(root: &Path, case: &Case, mode: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "hoimin".into(),
        mode.into(),
        "--root".into(),
        root.into(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        "type_sequence_iterable".into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1B".into(),
    ];
    let line = target_line(case);
    args.extend(["--line".into(), format!("subject.py:{line}-{line}").into()]);
    if mode == "run" {
        args.extend(["--format".into(), "json".into()]);
    }
    let identities = serde_json::to_string(&case.runtime_identity).unwrap();
    let check = if mode == "run" {
        format!(
            "import json, subject; assert list(subject.observed) == json.loads({identities:?}); assert subject.result"
        )
    } else {
        format!("import json, subject; assert list(subject.observed) == json.loads({identities:?})")
    };
    args.extend([
        "--".into(),
        python().into_os_string(),
        "-B".into(),
        "-c".into(),
        check.into(),
    ]);
    args
}

#[test]
fn prepared_annotation_import_corpus_rejects_schema_drift_and_missing_cases() {
    assert_eq!(parse_cases(CORPUS).unwrap().len(), 26);
    for bad in [
        CORPUS.replacen("\"schema\":1", "\"schema\":2", 1),
        CORPUS.replacen("module-false-false", "unknown-case", 1),
        CORPUS.replacen("\"mode\":\"strict\"", "\"mode\":\"model-only\"", 1),
        CORPUS.replacen("\"schema\":1", "\"extra\":true,\"schema\":1", 1),
        CORPUS.lines().skip(1).collect::<Vec<_>>().join("\n"),
        format!("{CORPUS}{}\n", CORPUS.lines().next().unwrap()),
    ] {
        assert!(parse_cases(&bad).is_err());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn prepared_annotation_import_public_plan_and_cpython_match_lean() {
    for case in parse_cases(CORPUS).unwrap() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("subject.py"), &case.source).unwrap();
        let observed = tokio::time::timeout(
            Duration::from_secs(15),
            tokio::process::Command::new(python())
                .current_dir(directory.path())
                .args([
                    "-B",
                    "-c",
                    "import json, subject; print(json.dumps(subject.observed))",
                ])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .expect("infrastructure error: CPython timeout")
        .expect("infrastructure error: CPython launch");
        assert!(
            observed.status.success(),
            "infrastructure error: {}: {}",
            case.id,
            String::from_utf8_lossy(&observed.stderr)
        );
        let identities: [bool; 2] = serde_json::from_slice(&observed.stdout)
            .expect("infrastructure error: invalid CPython observation");
        assert_eq!(
            identities, case.runtime_identity,
            "CPython mismatch: {}",
            case.id
        );

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = tokio::time::timeout(
            Duration::from_secs(15),
            hoimin_cli::run_with_io(
                command(directory.path(), &case, "plan"),
                &mut stdout,
                &mut stderr,
            ),
        )
        .await
        .expect("infrastructure error: plan timeout");
        assert_eq!(
            exit,
            0,
            "infrastructure error: {}: {}",
            case.id,
            String::from_utf8_lossy(&stderr)
        );
        let plan: PlanManifest =
            serde_json::from_slice(&stdout).expect("infrastructure error: plan JSON");
        assert_eq!(
            plan.candidates.len(),
            case.candidate_count,
            "semantic mismatch: {}",
            case.id
        );
        for candidate in plan.candidates {
            assert!(case.runtime_allowed, "unsafe emitted pair: {}", case.id);
            assert_eq!(candidate.operator, "type_sequence_iterable", "{}", case.id);
            assert_eq!(candidate.original, case.original, "{}", case.id);
            assert_eq!(candidate.replacement, case.replacement, "{}", case.id);
            assert_eq!(candidate.symbol, case.symbol, "{}", case.id);
            let line = target_line(&case);
            assert_eq!(candidate.line, line, "{}", case.id);
            let before_line: usize = case
                .source
                .split_inclusive('\n')
                .take(line as usize - 1)
                .map(str::len)
                .sum();
            let offset = before_line
                + case
                    .source
                    .lines()
                    .nth(line as usize - 1)
                    .unwrap()
                    .find(&case.original)
                    .unwrap();
            assert_eq!(
                usize::try_from(candidate.span.start).unwrap(),
                offset,
                "{}",
                case.id
            );
            assert_eq!(
                usize::try_from(candidate.span.length).unwrap(),
                case.original.len(),
                "{}",
                case.id
            );
            assert_eq!(
                &case.source[offset..offset + case.original.len()],
                case.original
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn prepared_annotation_import_public_run_does_not_count_custom_pairs() {
    let cases = parse_cases(CORPUS).unwrap();
    let selected: Vec<_> = cases.iter().filter(|case| case.run_check).collect();
    assert_eq!(selected.len(), 1, "original integer destination must run");
    for case in selected {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("subject.py"), &case.source).unwrap();
        // Run the baseline explicitly: an empty plan may not invoke its command.
        let args = command(directory.path(), case, "run");
        let check = args.last().unwrap();
        let baseline = tokio::time::timeout(
            Duration::from_secs(15),
            tokio::process::Command::new(python())
                .current_dir(directory.path())
                .args([OsString::from("-B"), OsString::from("-c"), check.clone()])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .expect("infrastructure error: baseline timeout")
        .unwrap();
        assert!(
            baseline.status.success(),
            "baseline {}: {}",
            case.id,
            String::from_utf8_lossy(&baseline.stderr)
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = tokio::time::timeout(
            Duration::from_secs(30),
            hoimin_cli::run_with_io(args, &mut stdout, &mut stderr),
        )
        .await
        .expect("infrastructure error: run timeout");
        assert_eq!(
            exit,
            0,
            "infrastructure error: {}: {}",
            case.id,
            String::from_utf8_lossy(&stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(report["summary"]["complete"], true, "{}", case.id);
        assert_eq!(report["summary"]["counts"]["killed"], 0, "{}", case.id);
        assert!(
            report["mutants"].as_array().unwrap().is_empty(),
            "{}",
            case.id
        );
    }
}
