use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/source-order.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    mode: String,
    kind: String,
    old: Vec<String>,
    current: Vec<String>,
    initial_status: String,
    status: String,
    reuse: bool,
    termination_exit: Option<u32>,
    executed: u32,
    exit: i32,
    complete: bool,
    baseline_exit: u32,
    other_status: String,
}

fn cases() -> Vec<Case> {
    let cases: Vec<Case> = CORPUS
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 8);
    let mut coordinates = BTreeSet::new();
    for case in &cases {
        assert_eq!(case.mode, "strict");
        assert!(["source", "imports"].contains(&case.kind.as_str()));
        for roots in [&case.old, &case.current] {
            let set: BTreeSet<&str> = roots.iter().map(String::as_str).collect();
            assert_eq!(roots.len(), 2);
            assert_eq!(set, BTreeSet::from(["a", "b"]));
        }
        assert!(coordinates.insert((&case.kind, &case.old, &case.current)));
    }
    cases
}

struct Fixture {
    directory: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        for name in ["a", "b"] {
            fs::create_dir_all(root.join(name)).unwrap();
            fs::write(root.join(name).join("calc.py"), "value = 1 + 2\n").unwrap();
        }
        Self { directory, root }
    }

    fn run(&self, case: &Case, order: &[String], resume: bool, database: &str) -> (Value, Value) {
        let python = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            });
        let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command.args(["run", "--root"]).arg(&self.root);
        let default_order = vec!["a".to_owned(), "b".to_owned()];
        let sources = if case.kind == "source" {
            order
        } else {
            &default_order
        };
        for source in sources {
            command.args(["--source", source]);
        }
        if case.kind == "imports" {
            for source in order {
                command.args(["--import-root", source]);
            }
        }
        let metrics = self.directory.path().join("metrics.json");
        command
            .args([
                "--operators",
                "binary_add_sub",
                "--jobs",
                "1",
                "--max-mutants",
                "1",
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--session",
            ])
            .arg(self.directory.path().join(database))
            .arg("--metrics")
            .arg(&metrics);
        if resume {
            command.arg("--resume");
        }
        command
            .arg("--")
            .arg(python)
            .args(["-c", "import calc; assert calc.value == 3"])
            .env_remove("PYTHONPATH");
        let output = command.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(case.exit),
            "{case:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            report["baseline"]["termination"]["Exit"],
            case.baseline_exit
        );
        assert_eq!(report["summary"]["complete"], case.complete);
        assert_eq!(report["summary"]["exit_code"], case.exit);
        assert_eq!(report["mutants"].as_array().unwrap().len(), 2);
        (
            report,
            serde_json::from_slice(&fs::read(metrics).unwrap()).unwrap(),
        )
    }
}

fn mutant<'a>(report: &'a Value, path: &str) -> &'a Value {
    report["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|mutant| mutant["candidate"]["path"] == path)
        .unwrap()
}

#[test]
fn source_order_public_resume_matches_lean_and_fresh_execution() {
    for case in cases() {
        let fixture = Fixture::new();
        let (initial, initial_metrics) = fixture.run(&case, &case.old, false, "session.sqlite");
        assert_eq!(mutant(&initial, "a/calc.py")["status"], case.initial_status);
        assert_eq!(initial_metrics["executed"], 1);
        let (resumed, metrics) = fixture.run(&case, &case.current, true, "session.sqlite");
        assert_eq!(metrics["executed"], case.executed, "{case:?}");
        assert_eq!(
            initial["run"]["run_id"] == resumed["run"]["run_id"],
            case.reuse,
            "{case:?}"
        );
        let a = mutant(&resumed, "a/calc.py");
        let b = mutant(&resumed, "b/calc.py");
        assert_eq!(a["status"], case.status, "{case:?}");
        assert_eq!(b["status"], case.other_status);
        assert_eq!(
            a["termination"],
            case.termination_exit
                .map_or(Value::Null, |exit| serde_json::json!({"Exit": exit}))
        );
        assert!(b["termination"].is_null());
        assert_eq!(
            mutant(&initial, "a/calc.py")["candidate"]["id"],
            a["candidate"]["id"]
        );
        if !case.reuse {
            let (fresh, fresh_metrics) = fixture.run(&case, &case.current, false, "fresh.sqlite");
            assert_eq!(fresh_metrics["executed"], 1);
            assert_eq!(mutant(&fresh, "a/calc.py")["status"], a["status"]);
            assert_eq!(
                mutant(&fresh, "a/calc.py")["candidate"]["id"],
                a["candidate"]["id"]
            );
        }
    }
}
