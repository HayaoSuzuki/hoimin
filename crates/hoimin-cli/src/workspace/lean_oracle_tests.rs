use std::collections::{BTreeMap, HashSet};
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    ApplyMutation, BudgetLedger, ByteSpan, EffectId, MutationCandidate, Preflight, ResetWorker,
    RunBudgets, WorkspaceCopyGrant, reserve_workspace_copy,
};
use serde::Deserialize;

use super::{CopyOptions, WorkspaceHandler, WorkspaceTask, WorkspaceTaskCompletion};

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
    include_str!("../../../../formal/HoiminOracle/corpus/workspace-lifecycle.jsonl")
}

fn internal_cases() -> Result<Vec<OracleCase>, String> {
    let mut cases = Vec::new();
    let mut ids = HashSet::new();
    for (index, line) in corpus_text().lines().enumerate() {
        let case: OracleCase =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", index + 1))?;
        if case.schema != 1 {
            return Err(format!("unsupported schema: {}", case.schema));
        }
        if case.schedule.len() != case.expected.len() {
            return Err(format!("{} has an invalid schedule length", case.id));
        }
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate case id: {}", case.id));
        }
        if case.mode == "internal-fixture" {
            cases.push(case);
        }
    }
    if cases.len() != 2 {
        return Err(format!("expected 2 internal cases, found {}", cases.len()));
    }
    Ok(cases)
}

struct Fixture {
    _project: tempfile::TempDir,
    handler: WorkspaceHandler,
    grant: Option<WorkspaceCopyGrant>,
    generations: BTreeMap<Utf8PathBuf, String>,
    tasks: BTreeMap<String, WorkspaceTask>,
    completions: BTreeMap<String, WorkspaceTaskCompletion>,
    next_id: u64,
    released: bool,
}

impl Fixture {
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
            tasks: BTreeMap::new(),
            completions: BTreeMap::new(),
            next_id: 1,
            released: false,
        })
    }

    fn id(&mut self) -> EffectId {
        let id = EffectId(self.next_id);
        self.next_id += 1;
        id
    }

    fn preflight(&mut self) -> Result<(), String> {
        let id = self.id();
        let completed = self
            .handler
            .handle_preflight(Preflight { id })
            .map_err(|error| error.failure.code().to_owned())?;
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
            .ok_or_else(|| "create requested before preflight".to_owned())?
            .create_worker(id, 0)
            .map_err(|error| error.to_string())?;
        self.handler
            .handle_create_worker(request)
            .map_err(|error| error.failure.code().to_owned())?;
        let root = self.handler.worker(0).unwrap().root().to_owned();
        self.generations.insert(root, generation.to_owned());
        Ok(())
    }

    fn candidate(&self) -> Result<MutationCandidate, String> {
        let worker = self
            .handler
            .worker(0)
            .ok_or_else(|| "candidate requested without active worker".to_owned())?;
        let hash = worker
            .manifest()
            .entry(Utf8Path::new("pkg/a.py"))
            .ok_or_else(|| "fixture file missing from manifest".to_owned())?
            .blake3
            .to_hex()
            .to_string();
        Ok(MutationCandidate {
            id: "lean-workspace-internal".into(),
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

    fn prepare(&mut self, event: &str) -> Result<(), String> {
        let parts = event.split(':').collect::<Vec<_>>();
        let task_id = parts
            .get(1)
            .ok_or_else(|| format!("missing task ID in {event}"))?
            .to_string();
        let operation = *parts
            .get(2)
            .ok_or_else(|| format!("missing operation in {event}"))?;
        let id = self.id();
        let task = match operation {
            "apply" => {
                let candidate = self.candidate()?;
                self.handler.prepare_apply_task(ApplyMutation {
                    id,
                    worker: 0,
                    candidate,
                })
            }
            "reset" => self
                .handler
                .prepare_reset_task(ResetWorker { id, worker: 0 }),
            "create" => {
                let request = self
                    .grant
                    .ok_or_else(|| "create task requested before preflight".to_owned())?
                    .create_worker(id, 0)
                    .map_err(|error| error.to_string())?;
                self.handler.prepare_create_task(request)
            }
            _ => return Err(format!("unknown prepare operation: {operation}")),
        }
        .map_err(|error| error.failure.code().to_owned())?;
        self.tasks.insert(task_id, task);
        Ok(())
    }

    fn execute(&mut self, event: &str) -> Result<(), String> {
        let parts = event.split(':').collect::<Vec<_>>();
        let task_id = parts
            .get(1)
            .ok_or_else(|| format!("missing task ID in {event}"))?
            .to_string();
        let expected_location = *parts
            .get(2)
            .ok_or_else(|| format!("missing execution location in {event}"))?;
        let task = self
            .tasks
            .remove(&task_id)
            .ok_or_else(|| format!("missing prepared task {task_id}"))?;
        let completion = task.execute();
        let actual_location = if completion.active.is_some() {
            "active"
        } else if completion.pending.is_some() {
            "pending"
        } else {
            "absent"
        };
        if actual_location != expected_location {
            return Err(format!(
                "{task_id} completed at {actual_location}, expected {expected_location}"
            ));
        }
        self.completions.insert(task_id, completion);
        Ok(())
    }

    fn accept(&mut self, task_id: &str) -> Result<(), String> {
        let completion = self
            .completions
            .remove(task_id)
            .ok_or_else(|| format!("missing completion {task_id}"))?;
        self.handler
            .accept_task_completion(completion)
            .map(|_| ())
            .map_err(|error| error.failure.code().to_owned())
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let id = self.id();
        let request = self
            .grant
            .ok_or_else(|| "cleanup requested before preflight".to_owned())?
            .cleanup(id);
        self.handler
            .handle_cleanup(request)
            .map_err(|error| error.failure.code().to_owned())?;
        self.released = true;
        Ok(())
    }

    fn slot_for_root(&self, root: &Utf8Path) -> Result<Slot, String> {
        let generation = self
            .generations
            .get(root)
            .ok_or_else(|| format!("unmapped worker root: {root}"))?;
        Ok(Slot {
            worker: "w0".to_owned(),
            generation: generation.clone(),
        })
    }

    fn active(&self) -> Result<Vec<Slot>, String> {
        self.handler
            .workers
            .values()
            .map(|workspace| self.slot_for_root(workspace.root()))
            .collect()
    }

    fn pending(&self) -> Result<Vec<Slot>, String> {
        self.handler
            .pending_cleanup
            .values()
            .map(|workspace| self.slot_for_root(workspace.root()))
            .collect()
    }

    fn task_owned(&self) -> Result<Vec<Slot>, String> {
        let task_roots = self.tasks.values().filter_map(|task| match task {
            WorkspaceTask::Create { pending, .. } => {
                pending.as_ref().map(|workspace| workspace.root())
            }
            WorkspaceTask::Apply { workspace, .. } | WorkspaceTask::Reset { workspace, .. } => {
                Some(workspace.root())
            }
            WorkspaceTask::Verify { .. } => None,
        });
        let completion_roots = self.completions.values().filter_map(|completion| {
            completion
                .active
                .as_ref()
                .map(|(_, workspace)| workspace.root())
                .or_else(|| {
                    completion
                        .pending
                        .as_ref()
                        .map(|(_, workspace)| workspace.root())
                })
        });
        task_roots
            .chain(completion_roots)
            .map(|root| self.slot_for_root(root))
            .collect()
    }

    fn observe(&self, event: &str, error_code: Option<String>) -> Result<OracleStep, String> {
        Ok(OracleStep {
            event: event.to_owned(),
            verdict: if error_code.is_some() {
                "rejected".to_owned()
            } else {
                "accepted".to_owned()
            },
            error_code,
            active: self.active()?,
            pending: self.pending()?,
            task_owned: self.task_owned()?,
            released: self.released,
        })
    }
}

fn replay(case: &OracleCase) -> Result<Vec<OracleStep>, String> {
    let mut fixture = Fixture::new()?;
    let mut observations = Vec::new();
    for event in &case.schedule {
        let result = match event.as_str() {
            "preflight" => fixture.preflight(),
            value if value.starts_with("create:w0:") => {
                fixture.create(value.rsplit(':').next().unwrap())
            }
            value if value.starts_with("prepare:") => fixture.prepare(value),
            value if value.starts_with("execute:") => fixture.execute(value),
            value if value.starts_with("accept:") => {
                fixture.accept(value.split(':').nth(1).unwrap())
            }
            "cleanup:success" => fixture.cleanup(),
            value => return Err(format!("unknown internal event: {value}")),
        };
        observations.push(fixture.observe(event, result.err())?);
    }
    Ok(observations)
}

#[test]
fn lean_workspace_internal_oracle_exposes_task_ownership() {
    let selected = std::env::var("HOIMIN_WORKSPACE_ORACLE_CASE").ok();
    let mut matched = false;
    for case in internal_cases()
        .expect("valid internal workspace cases")
        .into_iter()
        .filter(|case| selected.as_ref().is_none_or(|id| id == &case.id))
    {
        matched = true;
        let actual = replay(&case)
            .unwrap_or_else(|error| panic!("{} infrastructure-error: {error}", case.id));
        assert_eq!(actual, case.expected, "{} internal mismatch", case.id);
    }
    assert!(
        selected.is_none() || matched,
        "selected internal workspace oracle case was not found"
    );
}
