use std::{collections::BTreeSet, fs, path::Path};

use camino::Utf8Path;
use hoimin_cli::workspace::{CopyOptions, LiteralExclusion, WorkspaceError, WorkspacePlan};
use hoimin_core::EffectId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    ParsedCommand, Project, create, insert_test_min_free_space, parse_from, plan_args, preview_cli,
    write_json,
};

const EDIT_ORIGINAL: &str = r#"
import json
import shutil
import sys
from pathlib import Path

root = Path(sys.argv[1])
spec = json.loads(sys.argv[2])
phase = "mutant" if "1 - 2" in Path("src/calc.py").read_text() else "baseline"
for edit in spec[phase]:
    dst = root / edit["path"]
    if edit["kind"] == "write":
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(edit["content"].encode("utf-8"))
    elif edit["kind"] == "directory":
        dst.mkdir(parents=True, exist_ok=True)
    elif edit["kind"] == "remove":
        if dst.is_dir():
            shutil.rmtree(dst)
        else:
            dst.unlink()
    else:
        raise ValueError(edit["kind"])
sys.exit(spec[phase + "_exit"])
"#;

#[derive(Deserialize)]
struct Case {
    schema: u32,
    id: String,
    live: bool,
    initial: Vec<Entry>,
    input: Run,
    #[serde(rename = "final")]
    final_tree: Vec<Entry>,
    expected: Observation,
}

#[derive(Deserialize)]
struct Entry {
    path: String,
    content: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct Run {
    post_copy: Vec<Edit>,
    baseline: Vec<Edit>,
    mutant: Vec<Edit>,
    baseline_exit: u64,
    mutant_exit: u64,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Edit {
    Write { path: String, content: String },
    Directory { path: String },
    Remove { path: String },
}

impl Edit {
    fn path(&self) -> &str {
        match self {
            Self::Write { path, .. } | Self::Directory { path } | Self::Remove { path } => path,
        }
    }
}

#[derive(Deserialize)]
struct Observation {
    baseline_exit: Option<u64>,
    mutant_status: Option<String>,
    changed: Option<String>,
    exit_code: i32,
}

fn write_file(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn materialize(root: &Path, entries: &[Entry]) {
    for entry in entries {
        let path = root.join(&entry.path);
        if let Some(content) = &entry.content {
            write_file(&path, content);
        } else {
            fs::create_dir_all(path).unwrap();
        }
    }
}

fn assert_final_tree(root: &Path, case: &Case) {
    let touched: BTreeSet<_> = case
        .initial
        .iter()
        .map(|row| row.path.as_str())
        .chain(case.input.post_copy.iter().map(Edit::path))
        .chain(case.input.baseline.iter().map(Edit::path))
        .chain(case.input.mutant.iter().map(Edit::path))
        .collect();
    for path in touched {
        let actual = root.join(path);
        match case.final_tree.iter().find(|row| row.path == path) {
            None => assert!(!actual.exists(), "{}: {path}", case.id),
            Some(entry) => {
                if let Some(content) = &entry.content {
                    assert_eq!(
                        fs::read(actual).unwrap(),
                        content.as_bytes(),
                        "{}: {path}",
                        case.id
                    );
                } else {
                    assert!(actual.is_dir(), "{}: {path}", case.id);
                }
            }
        }
    }
}

fn verify_manifest_comparison(case: &Case, root: &Path) {
    let options = CopyOptions {
        literal_exclusions: vec![
            LiteralExclusion::File("ignored.txt".into()),
            LiteralExclusion::Tree("hidden-fixture".into()),
        ],
        ..CopyOptions::default()
    };
    let plan =
        WorkspacePlan::preflight(Utf8Path::from_path(root).unwrap(), EffectId(1), 1, options)
            .expect(&case.id);
    for edit in &case.input.post_copy {
        let path = root.join(edit.path());
        match edit {
            Edit::Write { content, .. } => write_file(&path, content),
            Edit::Directory { .. } => fs::create_dir_all(path).unwrap(),
            Edit::Remove { .. } if path.is_dir() => fs::remove_dir_all(path).unwrap(),
            Edit::Remove { .. } => fs::remove_file(path).unwrap(),
        }
    }
    let observed = match plan.verify_originals() {
        Ok(()) => None,
        Err(WorkspaceError::OriginalChanged { path }) => Some(path.into_string()),
        Err(error) => panic!("{}: {error}", case.id),
    };
    assert_eq!(observed, case.expected.changed, "{}", case.id);
}

fn events(bytes: &[u8]) -> Vec<Value> {
    bytes
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

fn assert_events(output: &std::process::Output, case: &Case) {
    let expect = &case.expected;
    assert_eq!(
        output.status.code(),
        Some(expect.exit_code),
        "{}: {}",
        case.id,
        String::from_utf8_lossy(&output.stderr)
    );
    let observed = events(&output.stdout);
    let baseline: Vec<_> = observed
        .iter()
        .filter(|event| event["kind"] == "baseline_finished")
        .map(|event| event["termination"]["Exit"].as_u64().expect(&case.id))
        .collect();
    assert_eq!(
        baseline,
        expect.baseline_exit.into_iter().collect::<Vec<_>>(),
        "{}",
        case.id
    );
    let mutants: Vec<_> = observed
        .iter()
        .filter(|event| event["kind"] == "mutant_finished")
        .map(|event| event["status"].as_str().expect(&case.id))
        .collect();
    assert_eq!(
        mutants,
        expect
            .mutant_status
            .as_deref()
            .into_iter()
            .collect::<Vec<_>>(),
        "{}",
        case.id
    );
    let summary = observed
        .iter()
        .find(|event| event["kind"] == "run_finished")
        .expect(&case.id);
    assert_eq!(summary["exit_code"], expect.exit_code, "{}", case.id);
    assert_eq!(summary["complete"], expect.exit_code == 0, "{}", case.id);
    let diagnostics = events(&output.stderr);
    let changed: Vec<_> = diagnostics
        .iter()
        .filter(|event| event["code"] == "workspace.original.changed")
        .map(|event| event["message"].as_str().unwrap())
        .collect();
    let message = expect
        .changed
        .as_ref()
        .map(|path| format!("original changed: {path}"));
    assert_eq!(
        changed,
        message.iter().map(String::as_str).collect::<Vec<_>>(),
        "{}",
        case.id
    );
}

async fn verify_live(case: &Case, project: &Project) {
    assert_eq!(case.input.post_copy, [], "{}", case.id);
    let stage = tempfile::tempdir().unwrap();
    let mut args = plan_args(
        project,
        [
            "--jobs",
            "1",
            "--operators",
            "binary_add_sub",
            "--exclude",
            "ignored.txt",
            "--exclude",
            "hidden-fixture",
        ],
        &stage.path().join("unused-marker"),
    );
    insert_test_min_free_space(&mut args);
    *args.last_mut().unwrap() = EDIT_ORIGINAL.into();
    args.extend([
        project.path.as_os_str().to_owned(),
        serde_json::to_string(&case.input).unwrap().into(),
    ]);
    let ParsedCommand::Plan(plan) = parse_from(args).unwrap() else {
        panic!("{}: expected plan arguments", case.id);
    };
    let (config, discovery) = plan.into_configs().unwrap();
    let output = create(config, discovery).await.expect(&case.id);
    let [candidate] = output.manifest.candidates.as_slice() else {
        panic!("{}: expected one binary addition candidate", case.id);
    };
    let artifact = stage.path().join("plan.json");
    write_json(&artifact, &serde_json::to_value(&output.manifest).unwrap());
    let run = preview_cli(&artifact, &["--candidate", &candidate.id], stage.path()).await;
    assert_events(&run, case);
}

#[tokio::test]
async fn original_workspace_integrity_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/workspace-integrity.jsonl"
    ));
    let mut ids = BTreeSet::new();
    let mut live = 0;
    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        let project = Project::new();
        materialize(&project.path, &case.initial);
        if case.live {
            verify_live(&case, &project).await;
            live += 1;
        } else {
            verify_manifest_comparison(&case, &project.path);
        }
        assert_final_tree(&project.path, &case);
    }
    assert_eq!(ids.len(), 58);
    assert_eq!(live, 13);
}
