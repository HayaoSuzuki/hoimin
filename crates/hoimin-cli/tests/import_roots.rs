use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

// No installer or network: a regular package exposed by a path-only editable .pth.
struct Project {
    temp: TempDir,
    root: PathBuf,
    python: PathBuf,
    log: PathBuf,
    source: &'static str,
}

impl Project {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir_all(root.join("src/pkg")).unwrap();
        fs::write(root.join("src/pkg/__init__.py"), "").unwrap();
        let source = "def add(a, b):\n    return a + b\n";
        fs::write(root.join("src/pkg/calc.py"), source).unwrap();
        fs::write(
            root.join("src/other.py"),
            "def other(a, b):\n    return a + b\n",
        )
        .unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = if cfg!(windows) {
            "Scripts/python.exe"
        } else {
            "bin/python"
        };
        let venv = temp.path().join("venv");
        let result = Command::new(repo.join(".venv").join(executable))
            .args(["-m", "venv", "--without-pip"])
            .arg(&venv)
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        let python = venv.join(executable);
        let result = Command::new(&python)
            .args([
                "-c",
                "import sysconfig; print(sysconfig.get_path('purelib'))",
            ])
            .output()
            .unwrap();
        assert!(result.status.success());
        let site = PathBuf::from(String::from_utf8(result.stdout).unwrap().trim());
        fs::write(
            site.join("fixture.pth"),
            format!("{}\n", root.join("src").display()),
        )
        .unwrap();
        let log = temp.path().join("imports.jsonl");
        fs::write(root.join("check.py"), format!("import json\nfrom pathlib import Path\nfrom pkg import calc\nvalue = calc.add(2, 3)\nwith Path({}).open('a') as f:\n    f.write(json.dumps(dict(file=calc.__file__, value=value)) + '\\n')\nassert value == 5\n", serde_json::to_string(&log).unwrap())).unwrap();
        Self {
            temp,
            root,
            python,
            log,
            source,
        }
    }

    fn command() -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command.env_remove("PYTHONPATH");
        command
    }

    fn execute(&self, mode: &str, selector: &[&str], options: &[&str], pythonpath: bool) -> Output {
        let mut command = Self::command();
        command
            .arg(mode)
            .arg("--root")
            .arg(&self.root)
            .args(selector)
            .args(options)
            .args([
                "--operators",
                "binary_add_sub",
                "--min-free-space",
                "1B",
                "--allow-best-effort-memory",
            ]);
        if mode == "run" {
            command.args(["--format", "json"]);
        }
        if pythonpath {
            command.env("PYTHONPATH", self.root.join("src"));
        }
        command
            .arg("--")
            .arg(&self.python)
            .arg("check.py")
            .output()
            .unwrap()
    }

    fn verify(path: &Path) -> Output {
        Self::command()
            .arg("verify")
            .arg(path)
            .args(["--top", "1", "--format", "json"])
            .output()
            .unwrap()
    }

    fn assert_killed(&self, output: &Output) -> Value {
        assert!(output.status.success(), "{output:?}");
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["baseline"]["termination"], json!({"Exit": 0}));
        let mutants = report["mutants"].as_array().unwrap();
        assert_eq!(mutants.len(), 1);
        assert_eq!(mutants[0]["candidate"]["path"], "src/pkg/calc.py");
        assert_eq!(mutants[0]["candidate"]["line"], 2);
        assert_eq!(mutants[0]["status"], "killed");
        let rows: Vec<Value> = fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["value"], 5);
        assert_eq!(rows[1]["value"], -1);
        assert_eq!(rows[0]["file"], rows[1]["file"]);
        for row in rows {
            let path = Path::new(row["file"].as_str().unwrap());
            assert!(path.ends_with("src/pkg/calc.py"));
            assert!(!path.starts_with(&self.root));
            assert!(
                path.to_string_lossy().contains("worker"),
                "{}",
                path.display()
            );
        }
        assert_eq!(
            fs::read_to_string(self.root.join("src/pkg/calc.py")).unwrap(),
            self.source
        );
        assert_eq!(
            fs::read_to_string(self.root.join("src/other.py")).unwrap(),
            "def other(a, b):\n    return a + b\n"
        );
        report
    }
}

#[test]
fn file_and_line_import_copied_regular_package_from_explicit_root() {
    for selector in [
        ["--file", "src/pkg/calc.py"],
        ["--line", "src/pkg/calc.py:2"],
    ] {
        let mut project = Project::new();
        if selector[0] == "--line" {
            project.source =
                "def add(a, b):\n    return a + b\n\ndef unused(a, b):\n    return a + b\n";
            fs::write(project.root.join("src/pkg/calc.py"), project.source).unwrap();
        }
        let output = project.execute("run", &selector, &["--import-root", "src"], false);
        let report = project.assert_killed(&output);
        assert_eq!(
            report["run"]["normalized_config"]["import_roots"],
            json!(["src"])
        );
        assert_eq!(
            report["run"]["normalized_config"]["selection"]["sources"],
            json!([])
        );
    }
}

#[test]
fn inherited_pythonpath_workaround_still_imports_worker_package() {
    let project = Project::new();
    project.assert_killed(&project.execute("run", &["--file", "src/pkg/calc.py"], &[], true));
}

#[test]
fn serialized_plan_verify_inherits_import_roots_and_preserves_plan() {
    let project = Project::new();
    let output = project.execute(
        "plan",
        &["--file", "src/pkg/calc.py"],
        &["--import-root", "./src", "--import-root", "src/"],
        false,
    );
    assert!(output.status.success(), "{output:?}");
    assert!(!project.log.exists());
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["schema_version"], 4);
    assert_eq!(plan["ranking_rule_version"], 4);
    assert_eq!(plan["normalized_config"]["import_roots"], json!(["src"]));
    assert_eq!(plan["candidates"].as_array().unwrap().len(), 1);
    let path = project.temp.path().join("plan.json");
    fs::write(&path, &output.stdout).unwrap();
    let report = project.assert_killed(&Project::verify(&path));
    assert_eq!(
        report["mutants"][0]["candidate"]["id"],
        plan["candidates"][0]["id"]
    );
    assert_eq!(fs::read(path).unwrap(), output.stdout);
}

#[test]
fn unavailable_copied_import_roots_fail_before_baseline() {
    for (root, options) in [
        ("missing", vec![]),
        ("imports", vec!["--exclude", "imports"]),
        ("check.py", vec![]),
    ] {
        let project = Project::new();
        fs::create_dir(project.root.join("imports")).unwrap();
        fs::write(project.root.join("imports/helper.py"), "value = 1\n").unwrap();
        let mut flags = vec!["--import-root", root];
        flags.extend(options);
        let output = project.execute("run", &["--file", "src/pkg/calc.py"], &flags, false);
        assert!(!output.status.success(), "{output:?}");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            combined.contains("unavailable in the copied worker"),
            "{combined}"
        );
        assert!(!project.log.exists(), "baseline command ran");
    }
}

#[test]
fn old_or_forged_plans_fail_before_baseline() {
    let project = Project::new();
    let output = project.execute(
        "plan",
        &["--file", "src/pkg/calc.py"],
        &["--import-root", "src"],
        false,
    );
    assert!(output.status.success(), "{output:?}");
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    let path = project.temp.path().join("invalid-plan.json");
    let mut old = plan.clone();
    old["schema_version"] = json!(3);
    old["normalized_config"]
        .as_object_mut()
        .unwrap()
        .remove("import_roots");
    fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    let rejected = Project::verify(&path);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("regenerate the plan"));
    for roots in [
        json!(["../outside"]),
        json!(["/absolute"]),
        json!(["src", "src"]),
        json!(["./src"]),
    ] {
        let mut forged = plan.clone();
        forged["normalized_config"]["import_roots"] = roots;
        fs::write(&path, serde_json::to_vec(&forged).unwrap()).unwrap();
        let rejected = Project::verify(&path);
        assert!(!rejected.status.success(), "{rejected:?}");
        assert!(!project.log.exists());
    }
}

#[test]
fn empty_selected_import_root_survives_children_only_exclusion() {
    let project = Project::new();
    fs::create_dir(project.root.join("imports")).unwrap();
    fs::write(project.root.join("imports/helper.py"), "value = 1\n").unwrap();
    let output = project.execute(
        "run",
        &["--file", "src/pkg/calc.py"],
        &["--import-root", "imports", "--exclude", "imports/**"],
        false,
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 0, "{output:?}");
    assert!(project.log.exists());
}
