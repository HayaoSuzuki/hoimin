use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use hoimin_cli::report::ReportHandler;
use hoimin_core::{
    BaselineFinished, ByteSpan, CommandArg, Diagnostic, EffectFailure, EffectId, EmitOutput,
    MutantFinished, MutantStarted, MutationCandidate, MutationStatus, MutationSummary, OutputEvent,
    OutputFormat, OutputSpoolRef, ProcessOutputState, ProcessTermination, REPORT_SCHEMA_VERSION,
    ReportSequence, ResourceMode, RunStarted, RunSummary, SessionDiagnostic, VerificationSelection,
    VerificationSelectionMode, VerificationSelectionPolicy, VerificationSelectionScope,
};
use serde::{Deserialize, Serialize};

const TEST_MIN_FREE_SPACE: &str = "1B";

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<WriterState>>);

#[derive(Default)]
struct WriterState {
    bytes: Vec<u8>,
    flushes: usize,
    writes: usize,
}

#[test]
fn original_schema_v3_report_fixture_matches_the_published_schema() {
    let root = repo_root();
    let event_schema = read_schema(&root.join("docs/json-schema/run-event.schema.json"));
    let result_schema = read_schema(&root.join("docs/json-schema/run-result.schema.json"));
    let report =
        read_schema(&root.join("crates/hoimin-cli/tests/golden/reports/schema-v4-original.json"));

    assert_schema_valid(&result_schema, &report, &event_schema);
    assert_report_optionals(&report, false);
}

#[test]
fn disk_stop_secondary_evidence_matches_the_published_event_schema() {
    let root = repo_root();
    let event_schema = read_schema(&root.join("docs/json-schema/run-event.schema.json"));
    let mut disk = default_disk_summary();
    disk.stop = Some(hoimin_core::DiskStopReport {
        code: hoimin_core::FILESYSTEM_RESERVE_REACHED.to_owned(),
        owned_bytes: Some(17),
        available_bytes: Some(5),
        message: None,
        secondary: vec![
            hoimin_core::DiskSecondary::Observation {
                reason: hoimin_core::DiskStopReason::WorkspaceSizeExceeded,
                value: hoimin_core::DiskObservation {
                    owned_bytes: 17,
                    available_bytes: 5,
                    measured_in: std::time::Duration::from_millis(3),
                },
            },
            hoimin_core::DiskSecondary::Error {
                code: "process.reap.failed".to_owned(),
                message: "fixture secondary error".to_owned(),
            },
        ],
    });
    let event = serde_json::to_value(OutputEvent::RunFinished(RunSummary {
        schema_version: REPORT_SCHEMA_VERSION,
        sequence: 1,
        run_id: "secondary-schema".to_owned(),
        counts: MutationSummary::default(),
        complete: false,
        exit_code: 2,
        disk,
        verification_selection: None,
    }))
    .unwrap();

    assert_schema_valid(&event_schema, &event, &event_schema);
}

#[test]
fn disk_filesystem_schema_requires_a_delta_for_two_endpoints() {
    let root = repo_root();
    let event_schema = read_schema(&root.join("docs/json-schema/run-event.schema.json"));
    let filesystem_schema = &event_schema["$defs"]["diskFilesystem"];
    let incomplete = serde_json::json!({
        "key": "workspace",
        "start_available_bytes": 1024,
        "minimum_available_bytes": 512,
        "end_available_bytes": 768,
        "available_bytes_change": null
    });

    assert_schema_invalid(filesystem_schema, &incomplete, &event_schema);
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct GoldenReportDocument {
    schema_version: u32,
    run: OutputEvent,
    baseline: OutputEvent,
    mutants: Vec<OutputEvent>,
    summary: OutputEvent,
}

#[test]
fn current_report_golden_matches_typed_semantic_regeneration() {
    let root = repo_root();
    let event_schema = read_schema(&root.join("docs/json-schema/run-event.schema.json"));
    let result_schema = read_schema(&root.join("docs/json-schema/run-result.schema.json"));
    let path = root.join("crates/hoimin-cli/tests/golden/reports/schema-v4-current.json");
    let checked_bytes = std::fs::read(&path).unwrap();
    let checked: GoldenReportDocument = serde_json::from_slice(&checked_bytes).unwrap();
    let checked_value: serde_json::Value = serde_json::from_slice(&checked_bytes).unwrap();
    assert_schema_valid(&result_schema, &checked_value, &event_schema);
    assert_canonical_normalized_config(&checked_value["run"]);
    let regenerated: GoldenReportDocument =
        serde_json::from_slice(&render_json_report(&all_optional_report_events(true))).unwrap();

    assert_eq!(checked, regenerated);
    assert_eq!(checked.schema_version, REPORT_SCHEMA_VERSION);
    assert_report_optionals(&serde_json::to_value(&checked).unwrap(), true);
}

#[test]
fn report_event_goldens_are_typed_complete_sequences() {
    let repo = repo_root();
    let event_schema = read_schema(&repo.join("docs/json-schema/run-event.schema.json"));
    let root = repo.join("crates/hoimin-cli/tests/golden/events");
    for (name, current) in [
        ("schema-v4-original.jsonl", false),
        ("schema-v4-current.jsonl", true),
    ] {
        let text = std::fs::read_to_string(root.join(name)).unwrap();
        let checked = text
            .lines()
            .map(|line| {
                let event = serde_json::from_str::<OutputEvent>(line).unwrap();
                let value: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_schema_valid(&event_schema, &value, &event_schema);
                event
            })
            .collect::<Vec<_>>();
        let mut sequence = ReportSequence::new();
        for event in &checked {
            sequence.observe(event).unwrap();
        }
        assert_eq!(checked, all_optional_report_events(current));
        assert_event_optionals(&checked, current);
    }
}

#[test]
fn current_event_golden_matches_typed_semantic_regeneration() {
    let root = repo_root();
    let event_schema = read_schema(&root.join("docs/json-schema/run-event.schema.json"));
    let path = root.join("crates/hoimin-cli/tests/golden/events/schema-v4-current.jsonl");
    let text = std::fs::read_to_string(path).unwrap();
    let first: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_canonical_normalized_config(&first);
    let checked = text
        .lines()
        .map(|line| {
            let event = serde_json::from_str::<OutputEvent>(line).unwrap();
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_schema_valid(&event_schema, &value, &event_schema);
            event
        })
        .collect::<Vec<_>>();

    assert_eq!(checked, all_optional_report_events(true));
}

fn assert_canonical_normalized_config(event: &serde_json::Value) {
    assert_eq!(
        event["normalized_config"]["limits"]["mutant_timeout"],
        "auto"
    );
    for argument in event["normalized_config"]["test_argv"].as_array().unwrap() {
        assert!(argument.get("unix").is_some());
    }
}

#[test]
fn raw_goldens_reject_unknown_fields_before_typed_comparison() {
    let root = repo_root();
    let event_schema = read_schema(&root.join("docs/json-schema/run-event.schema.json"));
    let result_schema = read_schema(&root.join("docs/json-schema/run-result.schema.json"));
    let mut report =
        read_schema(&root.join("crates/hoimin-cli/tests/golden/reports/schema-v4-current.json"));
    report["unexpected"] = serde_json::json!(true);
    assert_schema_invalid(&result_schema, &report, &event_schema);

    let line = std::fs::read_to_string(
        root.join("crates/hoimin-cli/tests/golden/events/schema-v4-current.jsonl"),
    )
    .unwrap();
    let mut event: serde_json::Value = serde_json::from_str(line.lines().next().unwrap()).unwrap();
    event["unexpected"] = serde_json::json!(true);
    assert_schema_invalid(&event_schema, &event, &event_schema);
}

#[test]
fn raw_goldens_reject_duplicate_known_fields_before_value_parsing() {
    let root = repo_root();
    let report_path = root.join("crates/hoimin-cli/tests/golden/reports/schema-v4-current.json");
    let report = std::fs::read_to_string(report_path).unwrap();
    let duplicate_report = report.replacen(
        "\"schema_version\": 4,",
        "\"schema_version\": 4,\n  \"schema_version\": 4,",
        1,
    );
    assert!(serde_json::from_str::<GoldenReportDocument>(&duplicate_report).is_err());

    let event_path = root.join("crates/hoimin-cli/tests/golden/events/schema-v4-current.jsonl");
    let events = std::fs::read_to_string(event_path).unwrap();
    let first = events.lines().next().unwrap();
    let duplicate_event = first.replacen(
        "\"kind\":\"run_started\",",
        "\"kind\":\"run_started\",\"kind\":\"run_started\",",
        1,
    );
    assert!(serde_json::from_str::<OutputEvent>(&duplicate_event).is_err());
}

#[allow(
    clippy::too_many_lines,
    reason = "The complete compatibility constructor keeps every serialized optional visible together."
)]
fn all_optional_report_events(current: bool) -> Vec<OutputEvent> {
    all_optional_report_events_at(current, REPORT_SCHEMA_VERSION)
}

#[allow(
    clippy::too_many_lines,
    reason = "Complete typed golden sequence spanning all event kinds"
)]
fn all_optional_report_events_at(current: bool, version: u32) -> Vec<OutputEvent> {
    let mut config = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--root",
        ".",
        "--source",
        "src",
        "--changed",
        "--diff-base",
        "golden-base",
        "--metrics",
        "metrics.json",
        "--session",
        "session.sqlite3",
        "--operators",
        "binary_add_sub",
        "--",
        "python",
        "-m",
        "pytest",
    ])
    .unwrap();
    config.root = ".".into();
    config.selection.root = ".".into();
    config.test_argv = ["python", "-m", "pytest"]
        .into_iter()
        .map(|argument| CommandArg::Unix(argument.as_bytes().to_vec()))
        .collect();
    config.output.metrics = Some("metrics.json".into());
    config.session.as_mut().unwrap().path = "session.sqlite3".into();
    let verification_selection = current.then(documented_verification_selection);
    let run_id_text = format!(
        "schema-v{version}-{}",
        if current { "current" } else { "original" }
    );
    let run_id = run_id_text.as_str();
    config.resume = current && version >= 4;
    let mut started = RunStarted::minimal(run_id, 1, test_resource_control());
    started.normalized_config = Some(config);
    started.resume = (current && version >= 4).then_some(hoimin_core::ResumeOutcome::Fresh {
        reason: hoimin_core::ResumeFreshReason::NoPriorRun,
    });
    "golden-os".clone_into(&mut started.versions.os);
    "golden-hoimin".clone_into(&mut started.versions.hoimin);
    "golden-control".clone_into(&mut started.resource_control.mechanism);
    started
        .verification_selection
        .clone_from(&verification_selection);
    let candidate = MutationCandidate {
        id: "golden-mutant".to_owned(),
        sequence: 0,
        path: "src/example.py".into(),
        span: ByteSpan {
            start: 4,
            length: 1,
        },
        original: "+".to_owned(),
        replacement: "-".to_owned(),
        operator: "binary_add_sub".to_owned(),
        line: 2,
        column: 17,
        symbol: Some("calculate".to_owned()),
        file_hash: "golden-file-hash".to_owned(),
    };
    vec![
        OutputEvent::RunStarted(started),
        OutputEvent::BaselineFinished(BaselineFinished {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence: 2,
            run_id: run_id.to_owned(),
            termination: ProcessTermination::Exit(0),
            elapsed_ms: 11,
            resource_mode: ResourceMode::Hard,
            output: OutputSpoolRef {
                token: "golden-baseline-output".to_owned(),
                retained: 21,
                observed: 34,
            },
        }),
        OutputEvent::MutantStarted(MutantStarted::new(run_id, 3, "golden-mutant", 0)),
        OutputEvent::MutantFinished(MutantFinished {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence: 4,
            run_id: run_id.to_owned(),
            candidate,
            status: MutationStatus::Killed,
            termination: Some(ProcessTermination::Exit(7)),
            output_state: ProcessOutputState::Complete,
            elapsed_ms: 13,
            resource_mode: ResourceMode::Hard,
            output: Some(OutputSpoolRef {
                token: "golden-mutant-output".to_owned(),
                retained: 55,
                observed: 89,
            }),
            diagnostics: Vec::new(),
        }),
        OutputEvent::Diagnostic(Diagnostic::new(
            run_id,
            5,
            "warning",
            "golden.warning",
            "golden diagnostic",
        )),
        OutputEvent::RunFinished(RunSummary {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence: 6,
            run_id: run_id.to_owned(),
            counts: MutationSummary {
                killed: 1,
                score: Some(1.0),
                ..MutationSummary::default()
            },
            complete: true,
            exit_code: 0,
            disk: default_disk_summary(),
            verification_selection,
        }),
    ]
    .into_iter()
    .map(|event| {
        let mut value = serde_json::to_value(event).unwrap();
        value["schema_version"] = serde_json::json!(version);
        serde_json::from_value(value).unwrap()
    })
    .collect()
}

fn render_json_report(events: &[OutputEvent]) -> Vec<u8> {
    let spool = tempfile::tempdir().unwrap();
    let stdout = SharedWriter::default();
    let mut handler =
        ReportHandler::new(OutputFormat::Json, stdout.clone(), io::sink(), spool.path()).unwrap();
    for (index, event) in events.iter().cloned().enumerate() {
        emit(
            &mut handler,
            u64::try_from(index).expect("fixture event count fits u64"),
            event,
        );
    }
    stdout.0.lock().unwrap().bytes.clone()
}

fn assert_report_optionals(document: &serde_json::Value, current: bool) {
    for pointer in [
        "/run/normalized_config",
        "/run/normalized_config/selection/diff_base",
        "/run/normalized_config/output/metrics",
        "/run/normalized_config/session",
        "/mutants/0/candidate/symbol",
        "/mutants/0/termination",
        "/mutants/0/output",
        "/summary/counts/score",
    ] {
        assert!(
            document
                .pointer(pointer)
                .is_some_and(|value| !value.is_null()),
            "missing populated report optional {pointer}"
        );
    }
    assert_eq!(
        document
            .pointer("/run/verification_selection")
            .is_some_and(|value| !value.is_null()),
        current
    );
    assert_eq!(
        document
            .pointer("/summary/verification_selection")
            .is_some_and(|value| !value.is_null()),
        current
    );
}

fn assert_event_optionals(events: &[OutputEvent], current: bool) {
    let OutputEvent::RunStarted(started) = &events[0] else {
        panic!("golden sequence must begin with run_started");
    };
    let config = started.normalized_config.as_ref().unwrap();
    assert_eq!(config.selection.diff_base.as_deref(), Some("golden-base"));
    assert_eq!(
        config
            .output
            .metrics
            .as_deref()
            .map(camino::Utf8Path::as_str),
        Some("metrics.json")
    );
    assert_eq!(
        config.session.as_ref().map(|session| session.path.as_str()),
        Some("session.sqlite3")
    );
    assert_eq!(started.verification_selection.is_some(), current);
    let OutputEvent::MutantFinished(finished) = &events[3] else {
        panic!("fourth golden event must finish the mutant");
    };
    assert_eq!(finished.candidate.symbol.as_deref(), Some("calculate"));
    assert_eq!(finished.termination, Some(ProcessTermination::Exit(7)));
    assert_eq!(
        finished.output.as_ref().unwrap().token,
        "golden-mutant-output"
    );
    let OutputEvent::RunFinished(summary) = &events[5] else {
        panic!("golden sequence must end with run_finished");
    };
    assert_eq!(summary.counts.score, Some(1.0));
    assert_eq!(summary.verification_selection.is_some(), current);
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self.0.lock().unwrap();
        state.writes += 1;
        state.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flushes += 1;
        Ok(())
    }
}

impl SharedWriter {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().bytes.clone()).unwrap()
    }

    fn flushes(&self) -> usize {
        self.0.lock().unwrap().flushes
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "The documentation contract is intentionally kept in one test so each README command is exercised against the same fixture."
)]
#[tokio::test]
async fn documentation_contract() {
    let root = repo_root();
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    assert!(
        readme.contains(
            "hoimin run --root . --source src --metrics metrics.json -- python -m pytest -q"
        ),
        "README must contain the documented --metrics metrics.json example",
    );
    assert!(
        readme.contains("opt-in") && readme.contains("separate from the run JSON"),
        "README must identify metrics as opt-in and separate from run JSON",
    );
    assert!(
        readme.contains("warns without changing the mutation result"),
        "README must document metrics write-failure behavior",
    );
    assert!(
        readme.contains(
            "`complete` is `false` when any mutant is inconclusive or the run fails or is interrupted"
        ),
        "README must define the machine-readable complete field",
    );
    let commands = fenced_run_commands(&readme);
    assert!(
        !commands.is_empty(),
        "README must contain fenced hoimin run commands"
    );
    assert!(commands.iter().any(|words| words.first().unwrap() == "uvx"));
    assert!(
        commands
            .iter()
            .any(|words| words.first().unwrap() == "pipx")
    );

    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(fixture.path().join("src")).unwrap();
    std::fs::create_dir_all(fixture.path().join("tests")).unwrap();
    std::fs::create_dir_all(fixture.path().join("tests/fixtures")).unwrap();
    std::fs::write(
        fixture.path().join("src/calc.py"),
        "def add(left, right) -> int:\n    return left + right\n",
    )
    .unwrap();
    std::fs::write(fixture.path().join("src/__init__.py"), "").unwrap();
    std::fs::write(fixture.path().join(".gitignore"), "tests/fixtures/\n").unwrap();
    std::fs::write(
        fixture.path().join("tests/fixtures/settings.toml"),
        "[fixture]\nvalue = 1\n",
    )
    .unwrap();
    let python = repository_python();
    let mut documented_outputs = Vec::new();
    let mut documented_type_operator = false;
    for command in commands {
        let argv = normalize_documented_command(&command, fixture.path(), &python);
        let config = hoimin_cli::cli::parse_config_from(argv.clone())
            .unwrap_or_else(|error| panic!("invalid documented command {command:?}: {error}"));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = hoimin_cli::run_with_io(argv, &mut stdout, &mut stderr).await;
        assert!(
            matches!(exit, 0 | 1),
            "documented command returned an infrastructure error: {exit}; command: {command:?}\nstderr: {}",
            String::from_utf8_lossy(&stderr)
        );
        match config.output.format {
            OutputFormat::Json => {
                let _: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
            }
            OutputFormat::Jsonl => {
                assert!(
                    stdout
                        .split(|byte| *byte == b'\n')
                        .filter(|line| !line.is_empty())
                        .all(|line| serde_json::from_slice::<serde_json::Value>(line).is_ok())
                );
            }
            OutputFormat::Human => assert!(!stdout.is_empty()),
        }
        documented_outputs.push((config.output.format, stdout, stderr));
    }

    let event_schema = read_schema(&root.join("docs/json-schema/run-event.schema.json"));
    let result_schema = read_schema(&root.join("docs/json-schema/run-result.schema.json"));
    assert_eq!(
        event_schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(
        result_schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    for (format, stdout, stderr) in documented_outputs {
        match format {
            OutputFormat::Json => {
                let document = serde_json::from_slice(&stdout).unwrap();
                assert_schema_valid(&result_schema, &document, &event_schema);
                documented_type_operator |= document["mutants"].as_array().is_some_and(|mutants| {
                    mutants.iter().any(|mutant| {
                        mutant["candidate"]["operator"]
                            .as_str()
                            .is_some_and(|operator| operator.starts_with("type_"))
                    })
                });
            }
            OutputFormat::Jsonl => {
                for line in stdout
                    .split(|byte| *byte == b'\n')
                    .chain(stderr.split(|byte| *byte == b'\n'))
                    .filter(|line| !line.is_empty())
                {
                    let event = serde_json::from_slice(line).unwrap();
                    assert_schema_valid(&event_schema, &event, &event_schema);
                }
            }
            OutputFormat::Human => {}
        }
    }

    assert!(
        documented_type_operator,
        "a documented JSON command must validate a type-operator candidate"
    );
    let (document, jsonl_events) = actual_documented_reports();
    assert_schema_valid(&result_schema, &document, &event_schema);
    for event in &jsonl_events {
        assert_schema_valid(&event_schema, event, &event_schema);
    }
    assert_eq!(
        document["run"]["verification_selection"]["policy"],
        "file_round_robin_v1"
    );
    assert_eq!(
        document["summary"]["verification_selection"]["policy"],
        "file_round_robin_v1"
    );
    for kind in ["run_started", "run_finished"] {
        let event = jsonl_events
            .iter()
            .find(|event| event["kind"] == kind)
            .unwrap_or_else(|| panic!("missing {kind} fixture"));
        assert_eq!(
            event["verification_selection"]["policy"],
            "file_round_robin_v1"
        );
    }

    let mut missing_result_policy = document.clone();
    missing_result_policy["run"]["verification_selection"]
        .as_object_mut()
        .unwrap()
        .remove("policy");
    assert_schema_invalid(&result_schema, &missing_result_policy, &event_schema);

    let mut unknown_event_policy = jsonl_events
        .iter()
        .find(|event| event["kind"] == "run_started")
        .unwrap()
        .clone();
    unknown_event_policy["verification_selection"]["policy"] = serde_json::json!("weighted");
    assert_schema_invalid(&event_schema, &unknown_event_policy, &event_schema);

    let incomplete = actual_pre_baseline_report();
    assert!(incomplete["baseline"].is_null());
    assert_schema_valid(&result_schema, &incomplete, &event_schema);

    let mut invalid = document.clone();
    invalid["schema_version"] = serde_json::json!(REPORT_SCHEMA_VERSION + 1);
    assert_schema_invalid(&result_schema, &invalid, &event_schema);
    invalid = document.clone();
    invalid["unexpected"] = serde_json::json!(true);
    assert_schema_invalid(&result_schema, &invalid, &event_schema);

    let mut invalid_event = jsonl_events
        .iter()
        .find(|event| event["kind"] == "mutant_finished")
        .unwrap()
        .clone();
    invalid_event["status"] = serde_json::json!("unknown");
    assert_schema_invalid(&event_schema, &invalid_event, &event_schema);
    invalid_event["status"] = serde_json::json!("killed");
    invalid_event["resource_mode"] = serde_json::json!("soft");
    assert_schema_invalid(&event_schema, &invalid_event, &event_schema);

    let mut invalid_event = jsonl_events
        .iter()
        .find(|event| event["kind"] == "run_started")
        .unwrap()
        .clone();
    invalid_event["versions"]["python"] = serde_json::json!("3.14.0");
    assert_schema_invalid(&event_schema, &invalid_event, &event_schema);
    invalid_event["versions"]
        .as_object_mut()
        .unwrap()
        .remove("python");
    invalid_event["versions"]["unexpected"] = serde_json::json!("1.0.0");
    assert_schema_invalid(&event_schema, &invalid_event, &event_schema);
}

fn fenced_run_commands(markdown: &str) -> Vec<Vec<String>> {
    let mut fenced = false;
    markdown
        .lines()
        .filter_map(|line| {
            if line.trim_start().as_bytes().starts_with(&[96; 3]) {
                fenced = !fenced;
                return None;
            }
            if !fenced {
                return None;
            }
            let line = line.trim().strip_prefix("$ ").unwrap_or(line.trim());
            let words = line
                .split_ascii_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let run = words.windows(2).any(|pair| pair == ["hoimin", "run"]);
            run.then_some(words)
        })
        .collect()
}

fn normalize_documented_command(command: &[String], root: &Path, python: &Path) -> Vec<String> {
    let launcher = command
        .windows(2)
        .position(|pair| pair == ["hoimin", "run"])
        .expect("documented launch prefix");
    let mut argv = command[launcher..].to_vec();
    let root = root.to_str().expect("UTF-8 fixture");
    let python = python.to_str().expect("UTF-8 Python");
    for index in 0..argv.len() {
        if index > 0 && argv[index - 1] == "--root" {
            root.clone_into(&mut argv[index]);
        } else if index > 0 && argv[index - 1] == "--metrics" {
            argv[index] = Path::new(root)
                .join("metrics.json")
                .to_string_lossy()
                .into_owned();
        } else if argv[index] == "python" {
            python.clone_into(&mut argv[index]);
        }
    }
    let separator = argv
        .iter()
        .position(|argument| argument == "--")
        .expect("documented direct argv separator");
    argv.splice(
        separator..separator,
        [
            "--max-mutants".to_owned(),
            "1".to_owned(),
            "--max-candidates".to_owned(),
            "32".to_owned(),
            "--total-timeout".to_owned(),
            "30s".to_owned(),
            "--min-free-space".to_owned(),
            TEST_MIN_FREE_SPACE.to_owned(),
            "--allow-best-effort-memory".to_owned(),
        ],
    );
    let separator = argv.iter().position(|argument| argument == "--").unwrap();
    argv.truncate(separator + 1);
    argv.extend([
        python.to_owned(),
        "-c".to_owned(),
        "from src.calc import add; assert add(2, 1) == 3".to_owned(),
    ]);
    argv
}

fn repository_python() -> PathBuf {
    let executable = if cfg!(windows) {
        repo_root().join(".venv/Scripts/python.exe")
    } else {
        repo_root().join(".venv/bin/python")
    };
    assert!(
        executable.is_file(),
        "repository virtualenv Python interpreter is missing: {}",
        executable.display()
    );
    executable
}

fn read_schema(path: &Path) -> serde_json::Value {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("read schema {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse schema {}: {error}", path.display()))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn actual_documented_reports() -> (serde_json::Value, Vec<serde_json::Value>) {
    let event_records = documented_events();
    let spool = tempfile::tempdir().unwrap();

    let json_stdout = SharedWriter::default();
    let mut json = ReportHandler::new(
        OutputFormat::Json,
        json_stdout.clone(),
        io::sink(),
        spool.path(),
    )
    .unwrap();
    for event in event_records.iter().cloned() {
        emit(&mut json, event.sequence(), event);
    }
    let document = serde_json::from_str(json_stdout.text().trim()).unwrap();

    let json_lines_stdout = SharedWriter::default();
    let jsonl_stderr = SharedWriter::default();
    let mut jsonl = ReportHandler::new(
        OutputFormat::Jsonl,
        json_lines_stdout.clone(),
        jsonl_stderr.clone(),
        spool.path(),
    )
    .unwrap();
    for event in event_records {
        emit(&mut jsonl, event.sequence(), event);
    }
    let events = json_lines_stdout
        .text()
        .lines()
        .chain(jsonl_stderr.text().lines())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (document, events)
}

fn actual_pre_baseline_report() -> serde_json::Value {
    let stdout = SharedWriter::default();
    let spool = tempfile::tempdir().unwrap();
    let mut handler =
        ReportHandler::new(OutputFormat::Json, stdout.clone(), io::sink(), spool.path()).unwrap();
    emit(
        &mut handler,
        1,
        OutputEvent::RunStarted(RunStarted::minimal(
            "pre-baseline",
            1,
            test_resource_control(),
        )),
    );
    emit(
        &mut handler,
        2,
        OutputEvent::RunFinished(RunSummary {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence: 2,
            run_id: "pre-baseline".to_owned(),
            counts: MutationSummary::default(),
            complete: false,
            exit_code: 4,
            disk: default_disk_summary(),
            verification_selection: None,
        }),
    );
    serde_json::from_str(stdout.text().trim()).unwrap()
}

fn documented_verification_selection() -> VerificationSelection {
    VerificationSelection {
        mode: VerificationSelectionMode::Top,
        policy: VerificationSelectionPolicy::FileRoundRobinV1,
        requested: 3,
        selected: 3,
        scope: VerificationSelectionScope::RetainedCandidates,
        plan_truncated: false,
    }
}

fn documented_events() -> Vec<OutputEvent> {
    let mut run_started = RunStarted::minimal("documented-run", 1, test_resource_control());
    run_started.verification_selection = Some(documented_verification_selection());
    let mut events = vec![
        OutputEvent::RunStarted(run_started),
        OutputEvent::BaselineFinished(BaselineFinished {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence: 2,
            run_id: "documented-run".to_owned(),
            termination: ProcessTermination::Exit(0),
            elapsed_ms: 10,
            resource_mode: ResourceMode::Hard,
            output: output_ref(),
        }),
    ];
    let statuses = [
        MutationStatus::Killed,
        MutationStatus::Survived,
        MutationStatus::Timeout,
        MutationStatus::OutOfMemory,
        MutationStatus::ProcessLimit,
        MutationStatus::Error,
        MutationStatus::NotRun,
    ];
    let mut summary = MutationSummary::default();
    let mut event_sequence = 3;
    for (mutant_sequence, status) in statuses.into_iter().enumerate() {
        let mutant_sequence = u64::try_from(mutant_sequence).expect("fixture sequence fits u64");
        let mutant_id = format!("m{mutant_sequence}");
        events.push(OutputEvent::MutantStarted(MutantStarted::new(
            "documented-run",
            event_sequence,
            &mutant_id,
            mutant_sequence,
        )));
        event_sequence += 1;
        summary.record(status);
        let termination = match status {
            MutationStatus::Killed => Some(ProcessTermination::Exit(1)),
            MutationStatus::Survived => Some(ProcessTermination::Exit(0)),
            MutationStatus::Timeout => Some(ProcessTermination::Timeout),
            MutationStatus::OutOfMemory => Some(ProcessTermination::OutOfMemory),
            MutationStatus::ProcessLimit => Some(ProcessTermination::ProcessLimit),
            MutationStatus::NotRun => Some(ProcessTermination::Cancelled),
            MutationStatus::Error => None,
        };
        events.push(OutputEvent::MutantFinished(MutantFinished {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence: event_sequence,
            run_id: "documented-run".to_owned(),
            candidate: MutationCandidate {
                id: mutant_id,
                sequence: mutant_sequence,
                path: "src/example.py".into(),
                span: ByteSpan {
                    start: mutant_sequence,
                    length: 1,
                },
                original: "+".to_owned(),
                replacement: "-".to_owned(),
                operator: "binary_add_sub".to_owned(),
                line: 1,
                column: u32::try_from(mutant_sequence).expect("fixture sequence fits u32"),
                symbol: None,
                file_hash: "hash".to_owned(),
            },
            status,
            termination,
            output_state: ProcessOutputState::Complete,
            elapsed_ms: 2,
            resource_mode: if mutant_sequence % 2 == 0 {
                ResourceMode::Hard
            } else {
                ResourceMode::BestEffort
            },
            output: termination.map(|_| output_ref()),
            diagnostics: Vec::new(),
        }));
        event_sequence += 1;
    }
    events.push(OutputEvent::Diagnostic(Diagnostic::new(
        "documented-run",
        event_sequence,
        "warning",
        "resource.best_effort",
        "fixture diagnostic",
    )));
    event_sequence += 1;
    events.push(OutputEvent::RunFinished(RunSummary {
        schema_version: REPORT_SCHEMA_VERSION,
        sequence: event_sequence,
        run_id: "documented-run".to_owned(),
        counts: summary,
        complete: false,
        exit_code: 2,
        disk: default_disk_summary(),
        verification_selection: Some(documented_verification_selection()),
    }));
    events
}

fn assert_schema_valid(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
    event_schema: &serde_json::Value,
) {
    if let Err(error) = validate_schema(schema, instance, schema, event_schema, "$") {
        panic!("schema validation failed: {error}\ninstance: {instance}");
    }
}

fn assert_schema_invalid(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
    event_schema: &serde_json::Value,
) {
    assert!(
        validate_schema(schema, instance, schema, event_schema, "$").is_err(),
        "invalid instance unexpectedly matched schema: {instance}"
    );
}

fn event_schema_pointer(reference: &str) -> Option<&str> {
    reference
        .strip_prefix("run-event.schema.json#")
        .or_else(|| reference.strip_prefix("run-event-v3.schema.json#"))
}

fn validate_schema(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
    active_root: &serde_json::Value,
    event_root: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(serde_json::Value::as_str) {
        let (root, pointer) = if let Some(pointer) = reference.strip_prefix('#') {
            (active_root, pointer)
        } else if let Some(pointer) = event_schema_pointer(reference) {
            (event_root, pointer)
        } else {
            return Err(format!("{path}: unsupported schema reference {reference}"));
        };
        let target = root
            .pointer(pointer)
            .ok_or_else(|| format!("{path}: unresolved schema reference {reference}"))?;
        return validate_schema(target, instance, root, event_root, path);
    }

    if let Some(branches) = schema.get("oneOf").and_then(serde_json::Value::as_array) {
        let matches = branches
            .iter()
            .filter(|branch| {
                validate_schema(branch, instance, active_root, event_root, path).is_ok()
            })
            .count();
        if matches != 1 {
            return Err(format!(
                "{path}: expected exactly one oneOf match, got {matches}"
            ));
        }
    }

    if let Some(expected) = schema.get("const")
        && instance != expected
    {
        return Err(format!("{path}: expected const {expected}, got {instance}"));
    }
    if let Some(values) = schema.get("enum").and_then(serde_json::Value::as_array)
        && !values.contains(instance)
    {
        return Err(format!("{path}: {instance} is not in enum"));
    }
    if let Some(expected) = schema.get("type") {
        let matches = match expected {
            serde_json::Value::String(kind) => instance_has_type(instance, kind),
            serde_json::Value::Array(kinds) => kinds
                .iter()
                .filter_map(serde_json::Value::as_str)
                .any(|kind| instance_has_type(instance, kind)),
            _ => false,
        };
        if !matches {
            return Err(format!("{path}: {instance} does not have type {expected}"));
        }
    }

    if let Some(minimum) = schema.get("minimum").and_then(serde_json::Value::as_f64)
        && instance.as_f64().is_some_and(|value| value < minimum)
    {
        return Err(format!("{path}: number is below {minimum}"));
    }
    if let Some(maximum) = schema.get("maximum").and_then(serde_json::Value::as_f64)
        && instance.as_f64().is_some_and(|value| value > maximum)
    {
        return Err(format!("{path}: number is above {maximum}"));
    }

    if let Some(object) = instance.as_object() {
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object);
        if let Some(required) = schema.get("required").and_then(serde_json::Value::as_array) {
            for name in required.iter().filter_map(serde_json::Value::as_str) {
                if !object.contains_key(name) {
                    return Err(format!("{path}: missing required property {name}"));
                }
            }
        }
        if let Some(properties) = properties {
            for (name, value) in object {
                if let Some(property_schema) = properties.get(name) {
                    validate_schema(
                        property_schema,
                        value,
                        active_root,
                        event_root,
                        &format!("{path}.{name}"),
                    )?;
                } else if schema.get("additionalProperties")
                    == Some(&serde_json::Value::Bool(false))
                {
                    return Err(format!("{path}: unexpected property {name}"));
                }
            }
        }
    }

    if let (Some(items), Some(values)) = (schema.get("items"), instance.as_array()) {
        for (index, value) in values.iter().enumerate() {
            validate_schema(
                items,
                value,
                active_root,
                event_root,
                &format!("{path}[{index}]"),
            )?;
        }
    }
    Ok(())
}

fn instance_has_type(instance: &serde_json::Value, kind: &str) -> bool {
    match kind {
        "null" => instance.is_null(),
        "boolean" => instance.is_boolean(),
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "number" => instance.is_number(),
        "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
        "string" => instance.is_string(),
        _ => false,
    }
}

#[test]
fn jsonl_flushes_each_event_and_keeps_diagnostics_on_stderr() {
    let stdout = SharedWriter::default();
    let stderr = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Jsonl,
        stdout.clone(),
        stderr.clone(),
        std::env::temp_dir(),
    )
    .unwrap();

    let emitted = handler
        .handle(EmitOutput {
            id: EffectId(41),
            event: OutputEvent::RunStarted(RunStarted::minimal(
                "run-1",
                1,
                test_resource_control(),
            )),
        })
        .unwrap();
    assert_eq!(emitted.id, EffectId(41));

    handler
        .handle(EmitOutput {
            id: EffectId(42),
            event: OutputEvent::Diagnostic(Diagnostic::new(
                "run-1",
                2,
                "warning",
                "resource.best_effort",
                "hard memory control unavailable",
            )),
        })
        .unwrap();

    assert_eq!(stdout.flushes(), 1);
    assert_eq!(stderr.flushes(), 1);
    let line: serde_json::Value = serde_json::from_str(stdout.text().trim()).unwrap();
    assert_eq!(line["schema_version"], REPORT_SCHEMA_VERSION);
    assert_eq!(line["kind"], "run_started");
    assert_eq!(line["sequence"], 1);
    assert!(!stdout.text().contains("diagnostic"));
    assert!(stderr.text().contains("\"kind\":\"diagnostic\""));
    assert_eq!(
        stdout.text(),
        format!(
            "{}\n",
            serde_json::to_string(&OutputEvent::RunStarted(RunStarted::minimal(
                "run-1",
                1,
                test_resource_control()
            )))
            .unwrap()
        )
    );
}

#[test]
fn jsonl_uses_all_exact_event_kinds_and_flushes_each_record() {
    let stdout = SharedWriter::default();
    let stderr = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Jsonl,
        stdout.clone(),
        stderr.clone(),
        std::env::temp_dir(),
    )
    .unwrap();

    for (offset, event) in events().into_iter().enumerate() {
        handler
            .handle(EmitOutput {
                id: EffectId(100 + offset as u64),
                event,
            })
            .unwrap();
    }

    let stdout_kinds = stdout
        .text()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["kind"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        stdout_kinds,
        [
            "run_started",
            "baseline_finished",
            "mutant_started",
            "mutant_finished",
            "run_finished",
        ]
    );
    assert_eq!(stdout.flushes(), 5);
    assert_eq!(stderr.flushes(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(stderr.text().trim()).unwrap()["kind"],
        "diagnostic"
    );
}

#[test]
fn json_streams_mutants_from_disk_into_one_document() {
    let stdout = SharedWriter::default();
    let stderr = SharedWriter::default();
    let spool = tempfile::tempdir().unwrap();
    let mut handler = ReportHandler::new(
        OutputFormat::Json,
        stdout.clone(),
        stderr.clone(),
        spool.path(),
    )
    .unwrap();
    let event_records = events();
    for event in event_records.iter().cloned() {
        handler
            .handle(EmitOutput {
                id: EffectId(event.sequence()),
                event,
            })
            .unwrap();
    }

    let document: serde_json::Value = serde_json::from_str(stdout.text().trim()).unwrap();
    assert_eq!(document["schema_version"], REPORT_SCHEMA_VERSION);
    assert_eq!(document["run"]["kind"], "run_started");
    assert_eq!(document["baseline"]["kind"], "baseline_finished");
    assert_eq!(document["mutants"].as_array().unwrap().len(), 1);
    assert_eq!(document["mutants"][0]["kind"], "mutant_finished");
    assert_eq!(document["summary"]["kind"], "run_finished");
    assert!(!stdout.text().contains("diagnostic"));
    assert!(stderr.text().contains("\"kind\":\"diagnostic\""));
    assert_eq!(stdout.flushes(), 1);
    assert_eq!(
        stdout.text(),
        format!(
            "{{\"schema_version\":{REPORT_SCHEMA_VERSION},\"run\":{},\"baseline\":{},\"mutants\":[{}],\"summary\":{}}}\n",
            serde_json::to_string(&event_records[0]).unwrap(),
            serde_json::to_string(&event_records[1]).unwrap(),
            serde_json::to_string(&event_records[3]).unwrap(),
            serde_json::to_string(&event_records[5]).unwrap(),
        )
    );
}

#[test]
fn report_errors_are_typed_and_echo_the_effect_id() {
    let mut handler = ReportHandler::new(
        OutputFormat::Jsonl,
        FailingWriter,
        io::sink(),
        std::env::temp_dir(),
    )
    .unwrap();
    let failed = handler
        .handle(EmitOutput {
            id: EffectId(99),
            event: OutputEvent::RunStarted(RunStarted::minimal(
                "run-1",
                1,
                test_resource_control(),
            )),
        })
        .unwrap_err();
    assert_eq!(failed.id, EffectId(99));
    assert!(matches!(failed.failure, EffectFailure::ReportIo { .. }));

    let spool = tempfile::tempdir().unwrap();
    let mut handler =
        ReportHandler::new(OutputFormat::Json, io::sink(), io::sink(), spool.path()).unwrap();
    emit(
        &mut handler,
        1,
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
    );
    let failed = handler
        .handle(EmitOutput {
            id: EffectId(100),
            event: OutputEvent::RunStarted(RunStarted::minimal(
                "run-1",
                2,
                test_resource_control(),
            )),
        })
        .unwrap_err();
    assert_eq!(failed.id, EffectId(100));
    assert!(matches!(failed.failure, EffectFailure::ReportState { .. }));

    let mut handler = ReportHandler::new(
        OutputFormat::Human,
        FailingWriter,
        io::sink(),
        std::env::temp_dir(),
    )
    .unwrap();
    let failed = handler
        .handle(EmitOutput {
            id: EffectId(101),
            event: OutputEvent::RunStarted(RunStarted::minimal(
                "run-1",
                1,
                test_resource_control(),
            )),
        })
        .unwrap_err();
    assert_eq!(failed.id, EffectId(101));
    assert!(matches!(failed.failure, EffectFailure::ReportIo { .. }));
}

#[test]
fn partial_stdout_failure_poisons_json_report() {
    let spool = tempfile::tempdir().unwrap();
    let mut handler = ReportHandler::new(
        OutputFormat::Json,
        FailAfter::new(24),
        io::sink(),
        spool.path(),
    )
    .unwrap();
    emit(
        &mut handler,
        1,
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
    );
    let failed = handler
        .handle(EmitOutput {
            id: EffectId(2),
            event: run_summary(2),
        })
        .unwrap_err();
    assert_eq!(failed.id, EffectId(2));
    assert!(matches!(failed.failure, EffectFailure::ReportIo { .. }));

    let retry = handler
        .handle(EmitOutput {
            id: EffectId(3),
            event: run_summary(3),
        })
        .unwrap_err();
    assert_eq!(retry.id, EffectId(3));
    assert!(matches!(retry.failure, EffectFailure::ReportState { .. }));
}

#[test]
fn partial_mutant_spool_failure_poisons_json_report() {
    let mut handler = ReportHandler::with_mutant_spool(
        OutputFormat::Json,
        io::sink(),
        io::sink(),
        FailAfter::new(12),
    );
    emit(
        &mut handler,
        1,
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
    );
    let failed = handler
        .handle(EmitOutput {
            id: EffectId(2),
            event: mutant_finished(2, 0),
        })
        .unwrap_err();
    assert_eq!(failed.id, EffectId(2));
    assert!(matches!(failed.failure, EffectFailure::ReportIo { .. }));

    let retry = handler
        .handle(EmitOutput {
            id: EffectId(3),
            event: mutant_finished(3, 0),
        })
        .unwrap_err();
    assert_eq!(retry.id, EffectId(3));
    assert!(matches!(retry.failure, EffectFailure::ReportState { .. }));
}

#[test]
fn json_spool_writes_one_complete_record_per_mutant() {
    let calls = Arc::new(AtomicUsize::new(0));
    let stdout = SharedWriter::default();
    let mut handler = ReportHandler::with_mutant_spool(
        OutputFormat::Json,
        stdout.clone(),
        io::sink(),
        CountingSpool::new(usize::MAX, Arc::clone(&calls)),
    );
    emit(
        &mut handler,
        1,
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
    );
    for index in 0..100 {
        emit(&mut handler, index + 2, mutant_finished(index + 2, index));
    }
    emit(&mut handler, 102, run_summary(102));

    assert_eq!(calls.load(Ordering::Relaxed), 100);
    let document: serde_json::Value = serde_json::from_str(stdout.text().trim()).unwrap();
    assert_eq!(document["mutants"].as_array().unwrap().len(), 100);
}

#[test]
fn json_spool_retries_short_writes_until_the_complete_record_is_stored() {
    let calls = Arc::new(AtomicUsize::new(0));
    let stdout = SharedWriter::default();
    let mut handler = ReportHandler::with_mutant_spool(
        OutputFormat::Json,
        stdout.clone(),
        io::sink(),
        CountingSpool::new(7, Arc::clone(&calls)),
    );
    emit(
        &mut handler,
        1,
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
    );
    emit(&mut handler, 2, mutant_finished(2, 0));
    emit(&mut handler, 3, run_summary(3));

    assert!(calls.load(Ordering::Relaxed) > 1);
    let document: serde_json::Value = serde_json::from_str(stdout.text().trim()).unwrap();
    assert_eq!(document["mutants"][0]["kind"], "mutant_finished");
}

#[test]
fn zero_progress_mutant_spool_write_fails_before_ack_and_poisons_report() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut handler = ReportHandler::with_mutant_spool(
        OutputFormat::Json,
        io::sink(),
        io::sink(),
        CountingSpool::new(0, Arc::clone(&calls)),
    );
    let failed = handler
        .handle(EmitOutput {
            id: EffectId(1),
            event: mutant_finished(1, 0),
        })
        .unwrap_err();

    assert_eq!(failed.id, EffectId(1));
    assert!(matches!(
        failed.failure,
        EffectFailure::ReportIo { ref operation, .. } if operation == "write mutant record"
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let retry = handler
        .handle(EmitOutput {
            id: EffectId(2),
            event: mutant_finished(2, 0),
        })
        .unwrap_err();
    assert!(matches!(retry.failure, EffectFailure::ReportState { .. }));
}

#[test]
fn human_format_writes_progress_to_stdout_and_diagnostics_to_stderr() {
    let stdout = SharedWriter::default();
    let stderr = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Human,
        stdout.clone(),
        stderr.clone(),
        std::env::temp_dir(),
    )
    .unwrap();
    for event in events() {
        handler
            .handle(EmitOutput {
                id: EffectId(event.sequence()),
                event,
            })
            .unwrap();
    }
    assert!(stdout.text().contains("run started: run-1"));
    assert!(stdout.text().contains("baseline finished: exit (0)"));
    assert!(
        stdout
            .text()
            .contains("mutant finished: src/example.py:1:0 binary \"+\" -> \"-\" killed")
    );
    assert!(stdout.text().contains("run summary:"));
    assert!(stdout.text().contains("killed: 1"));
    assert!(stdout.text().contains("survived: 0"));
    assert!(stdout.text().contains("timeout: 0"));
    assert!(stdout.text().contains("out_of_memory: 0"));
    assert!(stdout.text().contains("process_limit: 0"));
    assert!(stdout.text().contains("error: 0"));
    assert!(stdout.text().contains("not_run: 0"));
    assert!(stdout.text().contains("score: 1.00"));
    assert!(stdout.text().contains("complete: true"));
    assert!(stdout.text().contains("exit: 0"));
    assert!(stdout.text().contains("disk max owned bytes: 8589934592"));
    assert!(
        stdout
            .text()
            .contains("disk minimum free bytes: 10737418240")
    );
    assert!(!stdout.text().contains("diagnostic text"));
    assert!(stderr.text().contains("warning x: diagnostic text"));
    assert_eq!(stdout.flushes(), 5);
    assert_eq!(stderr.flushes(), 1);
}

#[test]
fn human_format_writes_mutant_output_timeout_diagnostic_to_stderr() {
    let stdout = SharedWriter::default();
    let stderr = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Human,
        stdout.clone(),
        stderr.clone(),
        std::env::temp_dir(),
    )
    .unwrap();
    let mut event = mutant_finished(1, 0);
    let OutputEvent::MutantFinished(finished) = &mut event else {
        unreachable!()
    };
    finished.status = MutationStatus::Error;
    finished.termination = Some(ProcessTermination::Timeout);
    finished.output_state = ProcessOutputState::CloseTimedOut;
    finished.diagnostics.push(SessionDiagnostic {
        mutant_id: finished.candidate.id.clone(),
        level: "error".to_owned(),
        code: "process.output.close.timeout".to_owned(),
        message: "captured output may be incomplete".to_owned(),
    });

    handler
        .handle(EmitOutput {
            id: EffectId(1),
            event,
        })
        .unwrap();

    assert!(stdout.text().contains("error"));
    assert!(
        stderr
            .text()
            .contains("error process.output.close.timeout: captured output may be incomplete")
    );
}

#[test]
fn human_format_renders_none_score_for_runs_without_decidable_mutants() {
    let stdout = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Human,
        stdout.clone(),
        io::sink(),
        std::env::temp_dir(),
    )
    .unwrap();

    handler
        .handle(EmitOutput {
            id: EffectId(1),
            event: OutputEvent::RunFinished(RunSummary {
                schema_version: REPORT_SCHEMA_VERSION,
                sequence: 1,
                run_id: "run-1".to_owned(),
                counts: MutationSummary::default(),
                complete: false,
                exit_code: 4,
                disk: default_disk_summary(),
                verification_selection: None,
            }),
        })
        .unwrap();

    assert!(stdout.text().contains("score: none"));
    assert!(stdout.text().contains("complete: false"));
    assert!(stdout.text().contains("exit: 4"));
}

#[test]
fn human_format_includes_profile_and_fingerprint_provenance_for_normalized_runs() {
    let stdout = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Human,
        stdout.clone(),
        io::sink(),
        std::env::temp_dir(),
    )
    .unwrap();
    let mut started = RunStarted::minimal("run-1", 1, test_resource_control());
    started.normalized_config = Some({
        let mut config = hoimin_cli::cli::parse_config_from([
            "hoimin",
            "run",
            "--file",
            "x.py",
            "--profile",
            "focused",
            "--fingerprint-include",
            "pyproject.toml",
            "--fingerprint-file",
            "pyproject.toml",
            "--",
            "check",
        ])
        .unwrap();
        config.fingerprint_inputs = vec![hoimin_core::FingerprintInputFile {
            path: "pyproject.toml".into(),
            hash: "sha256:fixture".into(),
        }];
        config
    });

    handler
        .handle(EmitOutput {
            id: EffectId(1),
            event: OutputEvent::RunStarted(started),
        })
        .unwrap();

    assert!(
        stdout
            .text()
            .contains("run started: run-1 (profile: focused)")
    );
    assert!(
        stdout
            .text()
            .contains("fingerprint includes: [pyproject.toml]")
    );
    assert!(
        stdout
            .text()
            .contains("fingerprint files: [pyproject.toml]")
    );
    assert!(
        stdout
            .text()
            .contains("fingerprint inputs: [pyproject.toml=")
    );
}

#[test]
fn human_format_includes_verification_selection_policy() {
    let stdout = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Human,
        stdout.clone(),
        io::sink(),
        std::env::temp_dir(),
    )
    .unwrap();
    let mut started = RunStarted::minimal("run-1", 1, test_resource_control());
    started.normalized_config = Some(
        hoimin_cli::cli::parse_config_from(["hoimin", "run", "--file", "x.py", "--", "check"])
            .unwrap(),
    );
    started.verification_selection = Some(VerificationSelection {
        mode: VerificationSelectionMode::Top,
        policy: VerificationSelectionPolicy::FileRoundRobinV1,
        requested: 3,
        selected: 3,
        scope: VerificationSelectionScope::RetainedCandidates,
        plan_truncated: false,
    });

    handler
        .handle(EmitOutput {
            id: EffectId(1),
            event: OutputEvent::RunStarted(started),
        })
        .unwrap();

    assert!(stdout.text().contains(
        "verification selection: mode=top policy=file_round_robin_v1 requested=3 selected=3 scope=retained_candidates plan_truncated=false"
    ));
}

#[test]
fn human_format_omits_fingerprint_provenance_without_patterns() {
    let stdout = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Human,
        stdout.clone(),
        io::sink(),
        std::env::temp_dir(),
    )
    .unwrap();
    let mut started = RunStarted::minimal("run-1", 1, test_resource_control());
    started.normalized_config = Some(
        hoimin_cli::cli::parse_config_from([
            "hoimin",
            "run",
            "--file",
            "x.py",
            "--profile",
            "focused",
            "--",
            "check",
        ])
        .unwrap(),
    );

    handler
        .handle(EmitOutput {
            id: EffectId(1),
            event: OutputEvent::RunStarted(started),
        })
        .unwrap();

    assert!(
        stdout
            .text()
            .contains("run started: run-1 (profile: focused)")
    );
    assert!(!stdout.text().contains("fingerprint includes:"));
    assert!(!stdout.text().contains("fingerprint inputs:"));
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("fixture write failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("fixture flush failure"))
    }
}

struct FailAfter {
    inner: Cursor<Vec<u8>>,
    remaining: usize,
}

struct CountingSpool {
    inner: Cursor<Vec<u8>>,
    max_chunk: usize,
    calls: Arc<AtomicUsize>,
}

impl CountingSpool {
    fn new(max_chunk: usize, calls: Arc<AtomicUsize>) -> Self {
        Self {
            inner: Cursor::new(Vec::new()),
            max_chunk,
            calls,
        }
    }
}

impl Write for CountingSpool {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let length = bytes.len().min(self.max_chunk);
        self.inner.write(&bytes[..length])
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for CountingSpool {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
}

impl Seek for CountingSpool {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}

impl FailAfter {
    fn new(remaining: usize) -> Self {
        Self {
            inner: Cursor::new(Vec::new()),
            remaining,
        }
    }
}

impl Write for FailAfter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other("injected partial write failure"));
        }
        let written = self.remaining.min(buf.len());
        self.inner.write_all(&buf[..written])?;
        self.remaining -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for FailAfter {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Seek for FailAfter {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}

fn emit(handler: &mut ReportHandler<impl Write, impl Write>, id: u64, event: OutputEvent) {
    handler
        .handle(EmitOutput {
            id: EffectId(id),
            event,
        })
        .unwrap();
}

fn events() -> Vec<OutputEvent> {
    vec![
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
        OutputEvent::BaselineFinished(BaselineFinished {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence: 2,
            run_id: "run-1".to_owned(),
            termination: ProcessTermination::Exit(0),
            elapsed_ms: 10,
            resource_mode: ResourceMode::Hard,
            output: output_ref(),
        }),
        OutputEvent::MutantStarted(MutantStarted::new("run-1", 3, "m1", 0)),
        mutant_finished(4, 0),
        OutputEvent::Diagnostic(Diagnostic::new(
            "run-1",
            5,
            "warning",
            "x",
            "diagnostic text",
        )),
        run_summary(6),
    ]
}

fn run_summary(sequence: u64) -> OutputEvent {
    OutputEvent::RunFinished(RunSummary {
        schema_version: REPORT_SCHEMA_VERSION,
        sequence,
        run_id: "run-1".to_owned(),
        counts: MutationSummary {
            killed: 1,
            score: Some(1.0),
            ..MutationSummary::default()
        },
        complete: true,
        exit_code: 0,
        disk: default_disk_summary(),
        verification_selection: None,
    })
}

fn default_disk_summary() -> hoimin_core::DiskRunSummary {
    hoimin_core::DiskRunSummary::unmeasured(8 * 1024 * 1024 * 1024, 10 * 1024 * 1024 * 1024)
}

fn mutant_finished(event_sequence: u64, mutant_sequence: u64) -> OutputEvent {
    OutputEvent::MutantFinished(MutantFinished {
        schema_version: REPORT_SCHEMA_VERSION,
        sequence: event_sequence,
        run_id: "run-1".to_owned(),
        candidate: MutationCandidate {
            id: format!("m{mutant_sequence}"),
            sequence: mutant_sequence,
            path: "src/example.py".into(),
            span: ByteSpan {
                start: mutant_sequence,
                length: 1,
            },
            original: "+".to_owned(),
            replacement: "-".to_owned(),
            operator: "binary".to_owned(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: "hash".to_owned(),
        },
        status: MutationStatus::Killed,
        termination: Some(ProcessTermination::Exit(1)),
        output_state: ProcessOutputState::Complete,
        elapsed_ms: 2,
        resource_mode: ResourceMode::Hard,
        output: Some(output_ref()),
        diagnostics: Vec::new(),
    })
}

fn output_ref() -> OutputSpoolRef {
    OutputSpoolRef {
        token: "output".to_owned(),
        retained: 4,
        observed: 9,
    }
}

fn test_resource_control() -> hoimin_core::ResourceControl {
    hoimin_core::ResourceControl {
        mode: hoimin_core::ResourceMode::Hard,
        mechanism: "test_supplied_hard".into(),
    }
}

#[test]
fn json_diagnostics_coalesce_writes_without_changing_bytes_or_event_flushes() {
    let spool = tempfile::tempdir().unwrap();
    for format in [OutputFormat::Json, OutputFormat::Jsonl] {
        for unit in ["x", "x\n", "\"\\\n\t\r\u{0000}é"] {
            for size in [16 * 1024, 1024 * 1024] {
                let message = unit.repeat(size / unit.len());
                let stderr = SharedWriter::default();
                let stdout = SharedWriter::default();
                let mut handler =
                    ReportHandler::new(format, stdout.clone(), stderr.clone(), spool.path())
                        .unwrap();
                let event = OutputEvent::Diagnostic(Diagnostic::new(
                    "run",
                    1,
                    "error",
                    "baseline.output",
                    message.clone(),
                ));
                let expected = serde_json::to_string(&event).unwrap() + "\n";
                let ack = handler
                    .handle(EmitOutput {
                        id: EffectId(42),
                        event,
                    })
                    .unwrap();
                assert_eq!(ack.id, EffectId(42));
                let state = stderr.0.lock().unwrap();
                assert_eq!(state.bytes, expected.as_bytes());
                assert_eq!(state.flushes, 1);
                let value: serde_json::Value = serde_json::from_slice(&state.bytes).unwrap();
                assert_eq!(value["message"], message);
                let bound = expected.len().div_ceil(4096) + 8;
                eprintln!(
                    "diagnostic format={format:?} payload={} encoded={} writes={} bound={bound}",
                    message.len(),
                    expected.len(),
                    state.writes
                );
                assert!(
                    state.writes <= bound,
                    "unbuffered diagnostic writes={} bound={bound}",
                    state.writes
                );
                drop(state);
                assert!(stdout.text().is_empty());
                assert_eq!(stdout.flushes(), 0);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum DiagnosticFault {
    PartialError,
    Zero,
    Interrupted,
    Flush,
}

#[derive(Clone)]
struct DiagnosticFaultWriter {
    mode: DiagnosticFault,
    state: Arc<Mutex<WriterState>>,
}

impl Write for DiagnosticFaultWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.state.lock().unwrap();
        state.writes += 1;
        match (self.mode, state.writes) {
            (DiagnosticFault::PartialError, 1) => {
                let n = bytes.len().min(4);
                state.bytes.extend_from_slice(&bytes[..n]);
                return Ok(n);
            }
            (DiagnosticFault::PartialError, 2) => {
                return Err(io::Error::other("transient diagnostic error"));
            }
            (DiagnosticFault::Zero, _) => return Ok(0),
            (DiagnosticFault::Interrupted, 1) => return Err(io::ErrorKind::Interrupted.into()),
            _ => {}
        }
        state.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.state.lock().unwrap().flushes += 1;
        if matches!(self.mode, DiagnosticFault::Flush) {
            Err(io::Error::other("diagnostic flush error"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn buffered_diagnostic_failures_do_not_acknowledge_or_retry_on_drop() {
    let spool = tempfile::tempdir().unwrap();
    for format in [OutputFormat::Json, OutputFormat::Jsonl] {
        for mode in [
            DiagnosticFault::PartialError,
            DiagnosticFault::Zero,
            DiagnosticFault::Flush,
        ] {
            for message in ["small".to_owned(), "x\n".repeat(16384)] {
                let state = Arc::new(Mutex::new(WriterState::default()));
                let writer = DiagnosticFaultWriter {
                    mode,
                    state: state.clone(),
                };
                let mut handler =
                    ReportHandler::new(format, io::sink(), writer, spool.path()).unwrap();
                let failure = handler
                    .handle(EmitOutput {
                        id: EffectId(77),
                        event: OutputEvent::Diagnostic(Diagnostic::new(
                            "run",
                            1,
                            "error",
                            "baseline.output",
                            message,
                        )),
                    })
                    .unwrap_err();
                assert_eq!(failure.id, EffectId(77));
                assert!(
                    matches!(failure.failure, EffectFailure::ReportIo { ref operation, .. } if operation == "write JSON Lines event")
                );
                drop(handler);
                let state = state.lock().unwrap();
                match mode {
                    DiagnosticFault::PartialError => {
                        assert_eq!(state.writes, 2, "retried after write error: {format:?}");
                        assert!(state.bytes.len() <= 4);
                        assert_eq!(state.flushes, 0);
                    }
                    DiagnosticFault::Zero => {
                        assert_eq!(state.writes, 1);
                        assert!(state.bytes.is_empty());
                    }
                    DiagnosticFault::Flush => assert_eq!(state.flushes, 1),
                    DiagnosticFault::Interrupted => unreachable!(),
                }
            }
        }
    }
}

#[test]
fn buffered_diagnostics_retry_interrupted_writes_and_flush_each_record() {
    let spool = tempfile::tempdir().unwrap();
    for format in [OutputFormat::Json, OutputFormat::Jsonl] {
        let state = Arc::new(Mutex::new(WriterState::default()));
        let writer = DiagnosticFaultWriter {
            mode: DiagnosticFault::Interrupted,
            state: state.clone(),
        };
        let mut handler = ReportHandler::new(format, io::sink(), writer, spool.path()).unwrap();
        let mut expected = String::new();
        for sequence in 1..=3 {
            let event = OutputEvent::Diagnostic(Diagnostic::new(
                "run",
                sequence,
                "error",
                "baseline.output",
                format!("record-{sequence}\n"),
            ));
            expected += &(serde_json::to_string(&event).unwrap() + "\n");
            assert_eq!(
                handler
                    .handle(EmitOutput {
                        id: EffectId(sequence),
                        event
                    })
                    .unwrap()
                    .id,
                EffectId(sequence)
            );
            let observed = state.lock().unwrap();
            assert_eq!(observed.bytes, expected.as_bytes());
            assert_eq!(observed.flushes, usize::try_from(sequence).unwrap());
        }
    }
}

#[test]
fn historical_schema_three_goldens_remain_typed_and_validate_the_archived_contract() {
    let root = repo_root();
    let schema = read_schema(&root.join("docs/json-schema/run-event-v3.schema.json"));
    let result_schema = read_schema(&root.join("docs/json-schema/run-result-v3.schema.json"));
    for era in ["original", "current"] {
        let value = read_schema(&root.join(format!(
            "crates/hoimin-cli/tests/golden/reports/schema-v3-{era}.json"
        )));
        assert_schema_valid(&result_schema, &value, &schema);
    }
    for (name, current) in [
        ("schema-v3-original.jsonl", false),
        ("schema-v3-current.jsonl", true),
    ] {
        let text = std::fs::read_to_string(
            root.join("crates/hoimin-cli/tests/golden/events")
                .join(name),
        )
        .unwrap();
        let events: Vec<OutputEvent> = text
            .lines()
            .map(|line| {
                let value: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_schema_valid(&schema, &value, &schema);
                serde_json::from_str(line).unwrap()
            })
            .collect();
        assert_eq!(events, all_optional_report_events_at(current, 3));
    }
}

#[tokio::test]
async fn public_resume_reports_validate_schema_four_in_json_and_jsonl() {
    let repo = repo_root();
    let event_schema = read_schema(&repo.join("docs/json-schema/run-event.schema.json"));
    let result_schema = read_schema(&repo.join("docs/json-schema/run-result.schema.json"));
    let python = repo.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    for format in ["json", "jsonl"] {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("project");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("calc.py"), "value = 1 + 2\nother = 4 + 5\n").unwrap();
        for resumed in [false, true] {
            let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
            command
                .args(["run", "--root"])
                .arg(&root)
                .args([
                    "--file",
                    "calc.py",
                    "--operators",
                    "binary_add_sub",
                    "--max-mutants",
                    "1",
                    "--allow-best-effort-memory",
                    "--min-free-space",
                    "1B",
                    "--resume",
                    "--format",
                    format,
                    "--session",
                ])
                .arg(fixture.path().join("session.db"))
                .env("TMPDIR", fixture.path())
                .env("TMP", fixture.path())
                .env("TEMP", fixture.path())
                .arg("--")
                .arg(&python)
                .args(["-c", "import calc; assert calc.value == 3"])
                .kill_on_drop(true);
            let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(4),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
            let expected = if resumed {
                serde_json::json!({"status":"resumed"})
            } else {
                serde_json::json!({"status":"fresh","reason":"no_prior_run"})
            };
            if format == "json" {
                let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_schema_valid(&result_schema, &document, &event_schema);
                assert_eq!(document["schema_version"], 4);
                assert_eq!(document["run"]["resume"], expected);
            } else {
                let text = String::from_utf8(output.stdout).unwrap();
                for (index, line) in text.lines().enumerate() {
                    let event: serde_json::Value = serde_json::from_str(line).unwrap();
                    assert_schema_valid(&event_schema, &event, &event_schema);
                    if index == 0 {
                        assert_eq!(event["resume"], expected);
                    }
                }
            }
        }
    }
}
