use std::collections::{BTreeMap, HashSet};
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::workspace::{CopyOptions, WorkspaceHandler};
use hoimin_core::{
    ApplyMutation, BudgetLedger, ByteSpan, EffectFailed, EffectId, MutationCandidate, Preflight,
    ResetWorker, RunBudgets, WorkspaceCopyGrant, reserve_workspace_copy,
};
use serde::Deserialize;

const EVENTS: &[&str] = &[
    "preflight",
    "create:w0:g0",
    "create:w0:g1",
    "sync_apply:w0",
    "sync_reset:w0",
    "prepare:t0:apply:w0",
    "prepare:t0:reset:w0",
    "execute:t0:active",
    "accept:t0",
    "cleanup:success",
];
const MODES: &[&str] = &[
    "strict",
    "internal-fixture",
    "model-only",
    "infrastructure-error",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Slot {
    worker: String,
    generation: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct OracleStep {
    event: String,
    verdict: String,
    error_code: Option<String>,
    active: Vec<Slot>,
    pending: Vec<Slot>,
    task_owned: Vec<Slot>,
    released: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    schedule: Vec<String>,
    expected: Vec<OracleStep>,
}

fn corpus_text() -> &'static str {
    include_str!("../../../formal/HoiminOracle/corpus/workspace-lifecycle.jsonl")
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut cases = Vec::new();
    let mut ids = HashSet::new();
    for (index, line) in input.lines().enumerate() {
        let case: OracleCase =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", index + 1))?;
        if case.schema != 1 {
            return Err(format!("unsupported schema: {}", case.schema));
        }
        if !MODES.contains(&case.mode.as_str()) {
            return Err(format!("unknown mode: {}", case.mode));
        }
        if case.schedule.is_empty() || case.schedule.len() != case.expected.len() {
            return Err(format!("{} has an invalid schedule length", case.id));
        }
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate case id: {}", case.id));
        }
        for (event, expected) in case.schedule.iter().zip(&case.expected) {
            if !EVENTS.contains(&event.as_str()) {
                return Err(format!("unknown event: {event}"));
            }
            if expected.event != *event {
                return Err(format!(
                    "{} observation event does not match schedule",
                    case.id
                ));
            }
            if !["accepted", "rejected"].contains(&expected.verdict.as_str()) {
                return Err(format!("unknown verdict: {}", expected.verdict));
            }
            for slot in expected
                .active
                .iter()
                .chain(&expected.pending)
                .chain(&expected.task_owned)
            {
                if !["w0", "w1"].contains(&slot.worker.as_str()) {
                    return Err(format!("unknown worker: {}", slot.worker));
                }
                if !["g0", "g1"].contains(&slot.generation.as_str()) {
                    return Err(format!("unknown generation: {}", slot.generation));
                }
            }
        }
        cases.push(case);
    }
    if cases.len() != 6 {
        return Err(format!("expected 6 cases, found {}", cases.len()));
    }
    Ok(cases)
}

struct StrictFixture {
    _project: tempfile::TempDir,
    handler: WorkspaceHandler,
    grant: Option<WorkspaceCopyGrant>,
    generations: BTreeMap<Utf8PathBuf, String>,
    next_id: u64,
    released: bool,
}

impl StrictFixture {
    fn new() -> Result<Self, String> {
        let project = tempfile::tempdir().map_err(|error| error.to_string())?;
        let package = project.path().join("pkg");
        fs::create_dir(&package).map_err(|error| error.to_string())?;
        fs::write(package.join("a.py"), b"original\n").map_err(|error| error.to_string())?;
        let root = Utf8Path::from_path(project.path())
            .ok_or_else(|| "temporary project path is not UTF-8".to_owned())?
            .to_owned();
        Ok(Self {
            handler: WorkspaceHandler::new(root, Vec::new(), 1, CopyOptions::default()),
            _project: project,
            grant: None,
            generations: BTreeMap::new(),
            next_id: 1,
            released: false,
        })
    }

    fn id(&mut self) -> EffectId {
        let id = EffectId(self.next_id);
        self.next_id += 1;
        id
    }

    fn failure_code(error: &EffectFailed) -> String {
        error.failure.code().to_owned()
    }

    fn preflight(&mut self) -> Result<(), String> {
        let id = self.id();
        let completed = self
            .handler
            .handle_preflight(Preflight { id })
            .map_err(|error| Self::failure_code(&error))?;
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes,
            processes: 1,
        });
        self.grant = Some(
            reserve_workspace_copy(&mut ledger, &completed).map_err(|error| error.to_string())?,
        );
        self.released = false;
        Ok(())
    }

    fn create(&mut self, generation: &str) -> Result<(), String> {
        let id = self.id();
        let request = self
            .grant
            .ok_or_else(|| "create requested before preflight grant".to_owned())?
            .create_worker(id, 0)
            .map_err(|error| error.to_string())?;
        self.handler
            .handle_create_worker(request)
            .map_err(|error| Self::failure_code(&error))?;
        let root = self
            .handler
            .worker(0)
            .ok_or_else(|| "successful create returned no active worker".to_owned())?
            .root()
            .to_owned();
        self.generations.insert(root, generation.to_owned());
        Ok(())
    }

    fn candidate(&self) -> Result<MutationCandidate, String> {
        let worker = self
            .handler
            .worker(0)
            .ok_or_else(|| "apply requested without active worker".to_owned())?;
        let hash = worker
            .manifest()
            .entry(Utf8Path::new("pkg/a.py"))
            .ok_or_else(|| "fixture candidate is absent from manifest".to_owned())?
            .blake3
            .to_hex()
            .to_string();
        Ok(MutationCandidate {
            id: "lean-workspace-oracle".into(),
            sequence: 0,
            path: "pkg/a.py".into(),
            span: ByteSpan {
                start: 0,
                length: 8,
            },
            original: "original".into(),
            replacement: "mutated!".into(),
            operator: "audit".into(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: hash,
        })
    }

    fn apply(&mut self) -> Result<(), String> {
        let candidate = self.candidate()?;
        let request = ApplyMutation {
            id: self.id(),
            worker: 0,
            candidate: candidate.clone(),
        };
        self.handler
            .handle_apply_mutation(request, &candidate)
            .map(|_| ())
            .map_err(|error| Self::failure_code(&error))
    }

    fn reset(&mut self) -> Result<(), String> {
        let id = self.id();
        self.handler
            .handle_reset_worker(ResetWorker { id, worker: 0 })
            .map(|_| ())
            .map_err(|error| Self::failure_code(&error))
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let id = self.id();
        let request = self
            .grant
            .ok_or_else(|| "cleanup requested before preflight grant".to_owned())?
            .cleanup(id);
        self.handler
            .handle_cleanup(request)
            .map_err(|error| Self::failure_code(&error))?;
        self.released = true;
        Ok(())
    }

    fn active(&self) -> Result<Vec<Slot>, String> {
        let Some(worker) = self.handler.worker(0) else {
            return Ok(Vec::new());
        };
        let generation = self
            .generations
            .get(worker.root())
            .ok_or_else(|| format!("unmapped worker root: {}", worker.root()))?;
        Ok(vec![Slot {
            worker: "w0".to_owned(),
            generation: generation.clone(),
        }])
    }

    fn observe(&self, event: &str, error_code: Option<String>) -> Result<OracleStep, String> {
        if self.handler.pending_cleanup_count() != 0 {
            return Err("strict fixture cannot inspect a pending generation".to_owned());
        }
        Ok(OracleStep {
            event: event.to_owned(),
            verdict: if error_code.is_some() {
                "rejected".to_owned()
            } else {
                "accepted".to_owned()
            },
            error_code,
            active: self.active()?,
            pending: Vec::new(),
            task_owned: Vec::new(),
            released: self.released,
        })
    }
}

fn replay_strict(case: &OracleCase) -> Result<Vec<OracleStep>, String> {
    let mut fixture = StrictFixture::new()?;
    let mut observations = Vec::new();
    for event in &case.schedule {
        let result = match event.as_str() {
            "preflight" => fixture.preflight(),
            value if value.starts_with("create:w0:") => {
                fixture.create(value.rsplit(':').next().unwrap())
            }
            "sync_apply:w0" => fixture.apply(),
            "sync_reset:w0" => fixture.reset(),
            "cleanup:success" => fixture.cleanup(),
            value => return Err(format!("unknown strict event: {value}")),
        };
        observations.push(fixture.observe(event, result.err())?);
    }
    Ok(observations)
}

#[test]
fn lean_workspace_strict_oracle_matches_public_handler() {
    let cases = parse_corpus(corpus_text()).expect("valid Lean workspace corpus");
    let selected = std::env::var("HOIMIN_WORKSPACE_ORACLE_CASE").ok();
    let mut matched = false;
    for case in cases
        .iter()
        .filter(|case| case.mode == "strict" && selected.as_ref().is_none_or(|id| id == &case.id))
    {
        matched = true;
        let actual = replay_strict(case)
            .unwrap_or_else(|error| panic!("{} infrastructure-error: {error}", case.id));
        assert_eq!(actual, case.expected, "{} strict mismatch", case.id);
    }
    assert!(
        selected.is_none() || matched,
        "selected strict workspace oracle case was not found"
    );
}

#[test]
fn lean_workspace_corpus_rejects_unknown_fields_and_duplicate_ids() {
    let first = corpus_text().lines().next().unwrap();
    let unknown = first.replacen("\"schema\":1", "\"schema\":1,\"extra\":true", 1);
    assert!(
        parse_corpus(&unknown)
            .unwrap_err()
            .contains("unknown field")
    );
    let duplicate = format!("{first}\n{first}\n");
    assert!(
        parse_corpus(&duplicate)
            .unwrap_err()
            .contains("duplicate case id")
    );
}
