use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/nullable-gate.jsonl");

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Mode {
    Strict,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    id: String,
    mode: Mode,
    source: String,
    operator: String,
    pairs: Vec<(String, String)>,
}

fn cases() -> Vec<Case> {
    let cases: Vec<Case> = CORPUS
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut ids = BTreeSet::new();
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert!(!case.id.is_empty() && ids.insert(&case.id), "{}", case.id);
        assert!(!case.source.is_empty());
        assert!(case.operator.starts_with("type_"));
        assert_eq!(
            hoimin_core::MutationOperatorSelection::parse_selector(&case.operator)
                .expect("corpus operator must be a public selector")
                .len(),
            1,
            "corpus rows select one operator"
        );
    }
    assert!(!cases.is_empty());
    cases
}

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
                "../../.venv/Scripts/python.exe"
            } else {
                "../../.venv/bin/python"
            })
        },
        PathBuf::from,
    )
}

async fn output(mut command: tokio::process::Command) -> std::process::Output {
    command.kill_on_drop(true);
    let result = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("infrastructure error: child deadline")
        .expect("infrastructure error: child launch");
    assert!(
        result.status.success(),
        "infrastructure error: status={}\nstdout={}\nstderr={}",
        result.status,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    result
}

async fn plan(case: &Case) -> serde_json::Value {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("subject.py"), &case.source).unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["plan", "--root"])
        .arg(project.path())
        .args([
            "--file",
            "subject.py",
            "--operators",
            &case.operator,
            "--allow-best-effort-memory",
            "--min-free-space",
            "1B",
            "--",
        ])
        .arg(python())
        .args(["-c", "pass"]);
    let result = output(command).await;
    let plan: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(plan["truncated"], false, "{}", case.id);
    assert!(
        plan["diagnostics"].as_array().unwrap().is_empty(),
        "{}: {plan}",
        case.id
    );
    plan
}

async fn check_python(case: &Case, mutants: &[String]) {
    let probe = concat!(
        "import annotationlib,json,sys,types\n",
        "assert sys.version_info >= (3,14), sys.version\n",
        "module=types.ModuleType('subject')\n",
        "exec(compile(sys.argv[1], 'subject.py', 'exec'), module.__dict__)\n",
        "annotationlib.get_annotations(module)['x']\n",
        "for source in json.loads(sys.argv[2]): compile(source, 'subject.py', 'exec')\n",
    );
    let mut command = tokio::process::Command::new(python());
    command.args([
        "-B",
        "-c",
        probe,
        &case.source,
        &serde_json::to_string(mutants).unwrap(),
    ]);
    output(command).await;
}

#[test]
fn nullable_gate_parser_requires_every_row_to_be_strict() {
    let rows = cases();
    assert_eq!(rows.len(), 65);
    assert!(rows.iter().all(|case| case.mode == Mode::Strict));
    for mode in ["strcit", "report-only"] {
        let mut invalid: serde_json::Value =
            serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
        invalid["mode"] = serde_json::json!(mode);
        assert!(serde_json::from_value::<Case>(invalid).is_err());
    }
}

#[tokio::test]
async fn nullable_gate_public_plan_matches_strict_lean_rows() {
    let mut matches = 0;
    for case in cases() {
        let plan = plan(&case).await;
        let mut observed = Vec::new();
        let mut mutants = Vec::new();
        for candidate in plan["candidates"].as_array().unwrap() {
            assert_eq!(candidate["operator"], case.operator);
            let original = candidate["original"].as_str().unwrap();
            let replacement = candidate["replacement"].as_str().unwrap();
            observed.push((original.to_owned(), replacement.to_owned()));
            let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
            let length = usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
            assert_eq!(&case.source[start..start + length], original, "{}", case.id);
            mutants.push(format!(
                "{}{}{}",
                &case.source[..start],
                replacement,
                &case.source[start + length..]
            ));
        }
        check_python(&case, &mutants).await;
        observed.sort();
        let mut expected = case.pairs.clone();
        expected.sort();
        assert_eq!(
            observed, expected,
            "semantic mismatch: {}\n{}",
            case.id, case.source
        );
        matches += 1;
    }
    eprintln!("strict matches={matches}; infrastructure errors=0");
}
