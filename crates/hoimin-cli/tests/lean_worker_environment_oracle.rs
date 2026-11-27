use std::collections::{BTreeMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::workspace::{
    CommandEnvironment, WorkspaceError, build_command_environment,
    build_command_environment_with_import_roots,
};
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/worker-environment.jsonl");
const PYTHONPATH: &str = "PYTHONPATH";
const PROBE_MODULE: &str = "hoimin_worker_environment_oracle.py";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Platform {
    Unix,
    Windows,
}

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    platform: Platform,
    directories: Vec<Vec<String>>,
    links: Vec<(Vec<String>, Vec<String>)>,
    input: Input,
    expected: Option<Expected>,
    modules: Vec<Vec<String>>,
    winner: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Input {
    original: Vec<String>,
    worker: Vec<String>,
    imports: Vec<Vec<String>>,
    sources: Vec<PathInput>,
    inherited: Vec<Variable>,
}

#[derive(Deserialize)]
struct PathInput {
    absolute: bool,
    parts: Vec<String>,
}

impl PathInput {
    fn render(&self, base: &Path) -> PathBuf {
        let path = relative_path(&self.parts);
        if self.absolute { base.join(path) } else { path }
    }
}

#[derive(Deserialize)]
struct Variable {
    name: String,
    paths: Vec<PathInput>,
}

#[derive(Deserialize)]
struct Expected {
    cwd: Vec<String>,
    paths: Vec<Vec<String>>,
    retained: Vec<String>,
}

fn relative_path(parts: &[String]) -> PathBuf {
    parts.iter().collect()
}

fn utf8_path(path: PathBuf) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(path).expect("fixture path is UTF-8")
}

fn set_up(case: &Case, base: &Path) {
    for directory in &case.directories {
        fs::create_dir_all(base.join(relative_path(directory))).unwrap();
    }
    #[cfg(unix)]
    for (name, destination) in &case.links {
        std::os::unix::fs::symlink(
            base.join(relative_path(destination)),
            base.join(relative_path(name)),
        )
        .unwrap();
    }
    #[cfg(not(unix))]
    assert!(
        case.links.is_empty(),
        "Unix fixture links on other platform"
    );
    for directory in &case.modules {
        fs::write(
            base.join(relative_path(directory)).join(PROBE_MODULE),
            b"value = 1\n",
        )
        .unwrap();
    }
}

fn inherited(input: &Input, base: &Path) -> BTreeMap<OsString, OsString> {
    input
        .inherited
        .iter()
        .map(|item| {
            let value =
                std::env::join_paths(item.paths.iter().map(|path| path.render(base))).unwrap();
            (OsString::from(&item.name), value)
        })
        .collect()
}

fn check_environment(case: &Case, base: &Path) -> Option<CommandEnvironment> {
    let input = &case.input;
    let original = utf8_path(base.join(relative_path(&input.original)));
    let worker = utf8_path(base.join(relative_path(&input.worker)));
    let inherited = inherited(input, base);
    let sources = input
        .sources
        .iter()
        .map(|source| utf8_path(source.render(base)))
        .collect::<Vec<_>>();
    let imports = input
        .imports
        .iter()
        .map(|path| utf8_path(relative_path(path)))
        .collect::<Vec<_>>();
    let result = build_command_environment_with_import_roots(
        &original, &worker, &imports, &sources, &inherited,
    );
    if imports.is_empty() {
        assert_eq!(
            result,
            build_command_environment(&original, &worker, &sources, &inherited),
            "{} legacy entrypoint",
            case.id
        );
    }
    let Some(expected) = &case.expected else {
        assert!(
            matches!(result, Err(WorkspaceError::PythonPath(_))),
            "{}: {result:?}",
            case.id
        );
        return None;
    };
    let environment = result.unwrap_or_else(|error| panic!("{}: {error}", case.id));
    let expected_paths = expected
        .paths
        .iter()
        .map(|path| base.join(relative_path(path)))
        .collect::<Vec<_>>();
    let encoded = &environment.env[OsStr::new(PYTHONPATH)];
    assert_eq!(
        std::env::split_paths(encoded).collect::<Vec<_>>(),
        expected_paths,
        "{} search order",
        case.id
    );
    assert_eq!(
        environment.cwd,
        utf8_path(base.join(relative_path(&expected.cwd))),
        "{} cwd",
        case.id
    );
    assert_eq!(
        environment.env.len(),
        expected.retained.len() + 1,
        "{} variable count",
        case.id
    );
    for name in &expected.retained {
        assert_eq!(
            environment.env.get(OsStr::new(name)),
            inherited.get(OsStr::new(name)),
            "{} variable {name}",
            case.id
        );
    }
    Some(environment)
}

async fn check_python_import(environment: &CommandEnvironment, expected: &Utf8Path, case: &str) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let python = repo.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut command = tokio::process::Command::new(python);
    command
        .current_dir(&environment.cwd)
        .env_clear()
        .envs(&environment.env)
        .args([
            "-B",
            "-P",
            "-S",
            "-c",
            "import hoimin_worker_environment_oracle as m; print(m.__file__)",
        ])
        .kill_on_drop(true);
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", root);
    }
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .expect("worker import fixture timed out")
        .expect("worker import fixture started");
    assert!(
        output.status.success(),
        "{case}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout).unwrap();
    let resolved = fs::canonicalize(actual.trim_end()).unwrap();
    assert_eq!(
        resolved,
        fs::canonicalize(expected.join(PROBE_MODULE)).unwrap(),
        "{case} imported module"
    );
}

#[tokio::test]
async fn worker_command_environment_matches_lean_oracle() {
    let platform = if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Unix
    };
    let mut ids = HashSet::new();
    let mut checked = 0;
    let mut probes = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "strict");
        assert!(ids.insert(case.id.clone()));
        if case.platform != platform {
            continue;
        }
        let directory = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(directory.path()).unwrap();
        set_up(&case, &base);
        let environment = check_environment(&case, &base);
        if !case.modules.is_empty() {
            let expected = utf8_path(base.join(relative_path(case.winner.as_ref().unwrap())));
            check_python_import(&environment.unwrap(), &expected, &case.id).await;
            probes += 1;
        }
        checked += 1;
    }
    assert_eq!(ids.len(), 146);
    assert_eq!(checked, if platform == Platform::Unix { 76 } else { 70 });
    assert_eq!(probes, 4);
}
