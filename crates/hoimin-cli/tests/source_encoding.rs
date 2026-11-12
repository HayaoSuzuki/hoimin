#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use collecting_writer::CollectingWriter;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use hoimin_cli::plan::PlanManifest;

fn python() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if cfg!(windows) {
        root.join(".venv/Scripts/python.exe")
    } else {
        root.join(".venv/bin/python")
    }
}

fn args(root: &Path, log: &Path) -> Vec<OsString> {
    vec!["hoimin".into(), "plan".into(), "--root".into(), root.into(), "--file".into(), "calc.py".into(), "--allow-best-effort-memory".into(), "--min-free-space".into(), "1B".into(), "--operators".into(), "boolean_literal,collection_list_tuple".into(), "--".into(), python().into(), "-c".into(), format!("import calc, json; from pathlib import Path; p = Path({:?}); p.open('a').write(json.dumps(list(Path('calc.py').read_bytes())) + '\\n'); assert calc.café == 'olé'; assert calc.items[0] == 'café'", log.to_string_lossy()).into()]
}

#[rstest::rstest]
#[tokio::test]
async fn latin1_plan_run_verify_preserve_raw_spans_ids_and_worker_bytes(
    #[values(false, true)] verify: bool,
) {
    let source = b"# coding: latin-1\nbefore = True; caf\xe9 = 'ol\xe9'; after = False\nitems = ['caf\xe9']\n";
    assert_latin1_flow(source, 2, verify).await;
}

#[rstest::rstest]
#[case::lf(b"\n".as_slice())]
#[case::crlf(b"\r\n")]
#[case::cr(b"\r")]
#[tokio::test]
async fn dense_latin1_plan_run_verify_preserve_all_python_line_endings(
    #[case] ending: &[u8],
    #[values(false, true)] verify: bool,
) {
    let mut source = b"# coding: latin-1".to_vec();
    source.extend_from_slice(ending);
    source.extend_from_slice(b"# ");
    source.extend(std::iter::repeat_n(0xe9, 4096));
    source.extend_from_slice(ending);
    source.extend_from_slice(b"before = True; caf\xe9 = 'ol\xe9'; after = False");
    source.extend_from_slice(ending);
    source.extend_from_slice(b"items = ['caf\xe9']");
    source.extend_from_slice(ending);
    assert_latin1_flow(&source, 3, verify).await;
}

async fn assert_latin1_flow(source: &[u8], first_candidate_line: u32, verify: bool) {
    let project = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("calc.py"), source).unwrap();
    let log = output.path().join("executions.jsonl");
    let plan_args = args(project.path(), &log);
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    // pins: issue #480
    let status =
        hoimin_cli::run_with_writers(plan_args.clone(), stdout.clone(), stderr.clone()).await;

    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    let manifest: PlanManifest = jsonl_report::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(manifest.candidates.len(), 3);
    assert!(!log.exists());
    let planned_ids = manifest
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected_tokens = [
        (b"True".as_slice(), b"False".as_slice()),
        (b"False", b"True"),
        (b"['caf\xe9']", b"('caf\xe9',)"),
    ];
    let mut expected_files = BTreeSet::from([source.to_vec()]);
    for (original, replacement) in expected_tokens {
        let start = source
            .windows(original.len())
            .position(|bytes| bytes == original)
            .unwrap();
        let candidate = manifest
            .candidates
            .iter()
            .find(|candidate| candidate.span.start == start as u64)
            .unwrap();
        assert_eq!(candidate.span.length, original.len() as u64);
        let line_start = source[..start]
            .iter()
            .rposition(|byte| matches!(byte, b'\n' | b'\r'))
            .map_or(0, |position| position + 1);
        assert_eq!(candidate.column as usize, start - line_start);
        assert_eq!(
            candidate.line,
            first_candidate_line + u32::from(original[0] == b'[')
        );
        assert_eq!(candidate.file_hash, blake3::hash(source).to_hex().as_str());
        let mut bytes = source[..start].to_vec();
        bytes.extend_from_slice(replacement);
        bytes.extend_from_slice(&source[start + original.len()..]);
        expected_files.insert(bytes);
    }
    let plan_path = output.path().join("plan.json");
    std::fs::write(&plan_path, stdout.snapshot()).unwrap();
    let mut run_args = plan_args;
    run_args[1] = "run".into();
    let verify_args = vec![
        "hoimin".into(),
        "verify".into(),
        plan_path.into_os_string(),
        "--top".into(),
        "3".into(),
    ];
    let command = [run_args, verify_args]
        .into_iter()
        .nth(usize::from(verify))
        .unwrap();
    stdout.clear();
    stderr.clear();
    let status = hoimin_cli::run_with_writers(command, stdout.clone(), stderr.clone()).await;

    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    let report: serde_json::Value = jsonl_report::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(
        report["baseline"]["termination"],
        serde_json::json!({"Exit": 0})
    );
    let mutants = report["mutants"].as_array().unwrap();
    assert_eq!(
        mutants
            .iter()
            .map(|mutant| mutant["candidate"]["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        planned_ids
    );
    assert_worker_files(&log, &expected_files);
    assert_eq!(
        std::fs::read(project.path().join("calc.py")).unwrap(),
        source
    );
    std::fs::remove_file(&log).unwrap();
}

fn assert_worker_files(log: &Path, expected: &BTreeSet<Vec<u8>>) {
    let observed = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Vec<u8>>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(observed.len(), 4);
    assert_eq!(&observed.into_iter().collect::<BTreeSet<_>>(), expected);
}

#[rstest::rstest]
#[case::cp1252(b"# coding: cp1252\nvalue = True\n".as_slice(), "cp1252")]
#[case::unknown(b"# coding: nonexistent-codec\nvalue = True\n", "nonexistent-codec")]
#[case::ascii(b"# coding: ascii\nname = '\xe9'; value = True\n", "ascii")]
#[case::utf8(b"# coding: UTF_8\nname = '\xe9'; value = True\n", "UTF_8")]
#[case::bom_latin1(b"\xef\xbb\xbf# coding: latin-1\nvalue = True\n", "latin-1")]
#[case::implicit_utf8(b"name = '\xe9'; value = True\n", "utf-8 (default)")]
#[tokio::test]
async fn encoding_diagnostics_include_path_and_declaration(
    #[case] source: &[u8],
    #[case] name: &str,
    #[values("plan", "run")] command: &str,
) {
    let project = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("calc.py"), source).unwrap();
    let marker = output.path().join("baseline.jsonl");
    let mut command_args = args(project.path(), &marker);
    command_args[1] = command.into();
    *command_args.last_mut().unwrap() = format!(
        "from pathlib import Path; Path({:?}).write_text('baseline')",
        marker.to_string_lossy()
    )
    .into();
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let status = hoimin_cli::run_with_writers(command_args, stdout.clone(), stderr.clone()).await;

    assert_eq!(
        status,
        2,
        "{command}: {}",
        String::from_utf8_lossy(&stderr.snapshot())
    );
    let error = format!(
        "{}{}",
        String::from_utf8_lossy(&stdout.snapshot()),
        String::from_utf8_lossy(&stderr.snapshot())
    );
    assert!(error.contains("calc.py"), "{error}");
    assert!(error.contains(name), "{error}");
    assert_eq!(
        marker.exists(),
        command == "run",
        "run retains its baseline-before-analysis lifecycle"
    );
    assert_eq!(
        std::fs::read(project.path().join("calc.py")).unwrap(),
        source
    );
}

#[rstest::rstest]
#[case::shebang(b"#!/usr/bin/python\r\n# coding=latin-1\r\nname = '\xe9'; value = True\r\n".as_slice())]
#[case::ascii(b"\n# coding: ascii\nvalue = True\n")]
#[case::bom_utf8(b"\xef\xbb\xbf# coding: UTF_8\nvalue = True\n")]
#[case::implicit_unicode("name = '日本語'; value = True\n".as_bytes())]
#[case::string(b"text = 'coding: cp1252'; value = True\n")]
#[case::trailing_comment(b"value = True # coding: cp1252\n")]
#[case::third_line(b"# not a cookie\n# ordinary comment\n# coding: cp1252\nvalue = True\n")]
#[tokio::test]
async fn accepted_cookie_placements_preserve_original_plan_hashes(#[case] source: &[u8]) {
    let project = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("calc.py"), source).unwrap();
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let status = hoimin_cli::run_with_writers(
        args(project.path(), &output.path().join("baseline")),
        stdout.clone(),
        stderr.clone(),
    )
    .await;

    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    let compiled = std::process::Command::new(python())
    .args(["-c", "import sys; from pathlib import Path; compile(Path(sys.argv[1]).read_bytes(), sys.argv[1], 'exec')"])
    .arg(project.path().join("calc.py")).output().unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let manifest: PlanManifest = jsonl_report::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(manifest.candidates.len(), 1);
    assert_eq!(
        manifest.candidates[0].file_hash,
        blake3::hash(source).to_hex().as_str()
    );
    let start = usize::try_from(manifest.candidates[0].span.start).unwrap();
    assert_eq!(&source[start..start + 4], b"True");
}

#[rstest::rstest]
#[tokio::test]
async fn latin1_symbol_selection_uses_shared_decoding_in_plan_run_and_verify(
    #[values(false, true)] verify: bool,
) {
    let project = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let source = b"# coding: latin-1\ndef caf\xe9():\n    return True\n";
    std::fs::write(project.path().join("calc.py"), source).unwrap();
    let mut command_args = args(project.path(), &output.path().join("unused"));
    let separator = command_args
        .iter()
        .position(|argument| argument == "--")
        .unwrap();
    command_args.splice(
        separator..separator,
        ["--source", ".", "--symbol", "calc:café"].map(OsString::from),
    );
    *command_args.last_mut().unwrap() = "import calc; assert calc.café()".into();
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let status =
        hoimin_cli::run_with_writers(command_args.clone(), stdout.clone(), stderr.clone()).await;

    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    let manifest: PlanManifest = jsonl_report::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(manifest.candidates.len(), 1);
    assert_eq!(manifest.candidates[0].symbol.as_deref(), Some("café"));
    let plan_path = output.path().join("plan.json");
    std::fs::write(&plan_path, stdout.snapshot()).unwrap();
    command_args[1] = "run".into();
    let verify_args = vec![
        "hoimin".into(),
        "verify".into(),
        plan_path.into_os_string(),
        "--top".into(),
        "1".into(),
    ];
    let command = [command_args, verify_args]
        .into_iter()
        .nth(usize::from(verify))
        .unwrap();
    stdout.clear();
    stderr.clear();
    let status = hoimin_cli::run_with_writers(command, stdout.clone(), stderr.clone()).await;

    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    let report: serde_json::Value = jsonl_report::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(
        report["mutants"][0]["candidate"]["id"],
        manifest.candidates[0].id
    );
    assert_eq!(
        std::fs::read(project.path().join("calc.py")).unwrap(),
        source
    );
}

#[rstest::rstest]
#[case::first_line(b"# coding: latin-1\nvalue = '\xe9'\n".as_slice(), true)]
#[case::second_line(b"# first\n# coding: latin-1\nvalue = '\xe9'\n", true)]
#[case::blank_first(b"\n# coding: latin-1\nvalue = '\xe9'\n", true)]
#[case::code_first(b"value = 1\n# coding: latin-1\ntext = '\xe9'\n", false)]
#[case::string(b"text = 'coding: latin-1'\nvalue = '\xe9'\n", false)]
#[case::trailing(b"value = 1 # coding: latin-1\ntext = '\xe9'\n", false)]
#[case::third_line(b"# first\n# second\n# coding: latin-1\ntext = '\xe9'\n", false)]
#[case::explanation(b"# explanation coding: latin-1\ntext = '\xe9'\n", true)]
#[case::bom_utf8(b"\xef\xbb\xbf# coding: UTF_8\nvalue = True\n", true)]
#[case::bom_utf8_alias(b"\xef\xbb\xbf# coding: utf8\nvalue = True\n", false)]
#[case::bom_latin1(b"\xef\xbb\xbf# coding: latin-1\nvalue = True\n", false)]
#[case::ascii_non_ascii(b"# coding: ascii\ntext = '\xe9'\n", false)]
#[test]
fn supported_codec_cookie_boundaries_match_cpython_compilation(
    #[case] source: &[u8],
    #[case] accepted: bool,
) {
    let project = tempfile::tempdir().unwrap();
    let path = project.path().join("calc.py");
    std::fs::write(&path, source).unwrap();
    let compiled = std::process::Command::new(python())
    .args(["-c", "import sys; from pathlib import Path; compile(Path(sys.argv[1]).read_bytes(), sys.argv[1], 'exec')"])
    .arg(&path).output().unwrap();
    assert_eq!(
        compiled.status.success(),
        accepted,
        "{source:?}: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert_eq!(
        hoimin_core::decode_python_source(source).is_ok(),
        accepted,
        "{source:?}"
    );
}
