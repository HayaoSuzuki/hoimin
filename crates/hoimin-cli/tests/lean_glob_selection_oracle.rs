//! The finite oracle's escaped-glob semantics are specific to Unix.
#![cfg(unix)]

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

use hoimin_core::{LineRange, LineSelection, Selection, TargetError, resolve_explicit};
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/glob-selection.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u32,
    mode: String,
    files: Vec<String>,
    pattern: String,
    selector: String,
    selected_path: String,
    expected_paths: Vec<String>,
    expected_error: bool,
    broken_paths: Vec<String>,
}

fn cases() -> Vec<Case> {
    let cases: Vec<Case> = CORPUS
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 20);
    let mut coordinates = BTreeSet::new();
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "strict");
        assert_eq!(case.files.len(), 5);
        assert!(["source", "file", "line"].contains(&case.selector.as_str()));
        assert!(coordinates.insert((&case.pattern, &case.selector, &case.selected_path)));
    }
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.expected_paths != case.broken_paths)
            .count(),
        6
    );
    cases
}

fn selection(root: &Path, case: &Case) -> Selection {
    let mut selection = Selection {
        root: root.to_str().unwrap().into(),
        excludes: vec![case.pattern.clone()],
        ..Selection::default()
    };
    match case.selector.as_str() {
        "source" => selection.sources.push("src".into()),
        "file" => selection.files.push(case.selected_path.clone().into()),
        "line" => selection.lines.push(LineSelection {
            path: case.selected_path.clone().into(),
            range: LineRange { start: 1, end: 1 },
        }),
        _ => unreachable!(),
    }
    selection
}

fn arguments(root: &Path, case: &Case, mode: &str) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("hoimin"),
        mode.into(),
        "--root".into(),
        root.as_os_str().to_owned(),
        "--exclude".into(),
        case.pattern.clone().into(),
        "--operators".into(),
        "binary_add_sub".into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1B".into(),
    ];
    match case.selector.as_str() {
        "source" => args.extend(["--source".into(), "src".into()]),
        "file" => args.extend(["--file".into(), case.selected_path.clone().into()]),
        "line" => args.extend([
            "--line".into(),
            format!("{}:1-1", case.selected_path).into(),
        ]),
        _ => unreachable!(),
    }
    let python = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".venv/bin/python");
    args.extend([
        "--".into(),
        python.into_os_string(),
        "-c".into(),
        "import runpy; assert runpy.run_path('src/[ab].py')['value'] == 3".into(),
    ]);
    args
}

fn assert_stage_paths(case: &Case, selected: &Selection) {
    let discovered = hoimin_cli::target::fs::discover_explicit(selected).unwrap();
    let paths: BTreeSet<_> = discovered.iter().map(|file| file.path.as_str()).collect();
    let expected: BTreeSet<_> = case.expected_paths.iter().map(String::as_str).collect();
    assert_eq!(paths, expected, "{case:?}");
    let resolved = resolve_explicit(selected, &discovered);
    if case.expected_error {
        assert!(
            matches!(resolved, Err(TargetError::MissingOrNonPythonFile(_))),
            "{case:?}: {resolved:?}"
        );
    } else {
        let resolved = resolved.unwrap();
        let paths: BTreeSet<_> = resolved.iter().map(|slice| slice.path.as_str()).collect();
        assert_eq!(paths, expected, "{case:?}");
    }
}

#[tokio::test]
async fn glob_discovery_core_and_public_plan_match_lean() {
    for case in cases() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("src")).unwrap();
        for file in &case.files {
            fs::write(directory.path().join(file), "value = 1 + 2\n").unwrap();
        }
        assert_stage_paths(&case, &selection(directory.path(), &case));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = hoimin_cli::run_with_io(
            arguments(directory.path(), &case, "plan"),
            &mut stdout,
            &mut stderr,
        )
        .await;
        if case.expected_error {
            assert_eq!(code, 2);
            assert!(
                String::from_utf8_lossy(&stderr)
                    .contains("target is missing or is not a Python file")
            );
            assert_eq!(stdout, Vec::<u8>::new());
            continue;
        }
        assert_eq!(code, 0, "{case:?}: {}", String::from_utf8_lossy(&stderr));
        let plan: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(plan["truncated"], false);
        assert_eq!(plan["diagnostics"], serde_json::json!([]));
        let candidates = plan["candidates"].as_array().unwrap();
        let paths: BTreeSet<_> = candidates
            .iter()
            .map(|candidate| candidate["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            case.expected_paths.iter().map(String::as_str).collect()
        );
        assert_eq!(candidates.len(), case.expected_paths.len());
        for candidate in candidates {
            assert_eq!(candidate["original"], "+");
            assert_eq!(candidate["replacement"], "-");
            assert_eq!(candidate["line"], 1);
            assert_eq!(
                candidate["span"],
                serde_json::json!({"start": 10, "length": 1})
            );
        }
    }
}

#[tokio::test]
async fn bracket_filename_public_run_keeps_the_mutation() {
    let case = cases()
        .into_iter()
        .find(|case| {
            case.selector == "file"
                && case.pattern == "src/[ab].py"
                && case.selected_path == "src/[ab].py"
        })
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(directory.path().join("src/[ab].py"), "value = 1 + 2\n").unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(
        arguments(directory.path(), &case, "run"),
        &mut stdout,
        &mut stderr,
    )
    .await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
    let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["counts"]["killed"], 1);
    assert_eq!(report["summary"]["complete"], true);
}
