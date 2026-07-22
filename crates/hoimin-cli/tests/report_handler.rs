use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use hoimin_cli::report::ReportHandler;
use hoimin_core::{
    BaselineFinished, ByteSpan, Diagnostic, EffectFailure, EffectId, EmitOutput, MutantFinished,
    MutantStarted, MutationCandidate, MutationStatus, MutationSummary, OutputEvent, OutputFormat,
    OutputSpoolRef, ProcessTermination, REPORT_SCHEMA_VERSION, ResourceMode, RunStarted,
    RunSummary,
};

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<WriterState>>);

#[derive(Default)]
struct WriterState {
    bytes: Vec<u8>,
    flushes: usize,
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().bytes.extend_from_slice(buf);
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
        OutputEvent::RunStarted(RunStarted::minimal("pre-baseline", 1)),
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
        }),
    );
    serde_json::from_str(stdout.text().trim()).unwrap()
}

fn documented_events() -> Vec<OutputEvent> {
    let mut events = vec![
        OutputEvent::RunStarted(RunStarted::minimal("documented-run", 1)),
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
            elapsed_ms: 2,
            resource_mode: if mutant_sequence % 2 == 0 {
                ResourceMode::Hard
            } else {
                ResourceMode::BestEffort
            },
            output: termination.map(|_| output_ref()),
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
        } else if let Some(pointer) = reference.strip_prefix("run-event.schema.json#") {
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

    if let Some(expected) = schema.get("const") {
        if instance != expected {
            return Err(format!("{path}: expected const {expected}, got {instance}"));
        }
    }
    if let Some(values) = schema.get("enum").and_then(serde_json::Value::as_array) {
        if !values.contains(instance) {
            return Err(format!("{path}: {instance} is not in enum"));
        }
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

    if let Some(minimum) = schema.get("minimum").and_then(serde_json::Value::as_f64) {
        if instance.as_f64().is_some_and(|value| value < minimum) {
            return Err(format!("{path}: number is below {minimum}"));
        }
    }
    if let Some(maximum) = schema.get("maximum").and_then(serde_json::Value::as_f64) {
        if instance.as_f64().is_some_and(|value| value > maximum) {
            return Err(format!("{path}: number is above {maximum}"));
        }
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
            event: OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
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
            serde_json::to_string(&OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)))
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
            event: OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
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
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
    );
    let failed = handler
        .handle(EmitOutput {
            id: EffectId(100),
            event: OutputEvent::RunStarted(RunStarted::minimal("run-1", 2)),
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
            event: OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
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
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
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
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
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
    assert!(stdout.text().contains("mutant finished: m0 killed"));
    assert!(stdout.text().contains("run finished: exit 0"));
    assert!(!stdout.text().contains("diagnostic text"));
    assert!(stderr.text().contains("warning x: diagnostic text"));
    assert_eq!(stdout.flushes(), 5);
    assert_eq!(stderr.flushes(), 1);
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
    let mut started = RunStarted::minimal("run-1", 1);
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
fn human_format_omits_fingerprint_provenance_without_patterns() {
    let stdout = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Human,
        stdout.clone(),
        io::sink(),
        std::env::temp_dir(),
    )
    .unwrap();
    let mut started = RunStarted::minimal("run-1", 1);
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
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
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
    })
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
        elapsed_ms: 2,
        resource_mode: ResourceMode::Hard,
        output: Some(output_ref()),
    })
}

fn output_ref() -> OutputSpoolRef {
    OutputSpoolRef {
        token: "output".to_owned(),
        retained: 4,
        observed: 9,
    }
}
