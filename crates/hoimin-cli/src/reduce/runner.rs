//! Owns copy/reset, cancellation, disk observation and child-tree shutdown.
use super::{artifacts::Artifacts, input::Input};
use crate::process::{ProcessCancellation, ProcessHandler, ProcessRequest};
use crate::workspace::{
    CopyOptions, DiskMeter, DiskMonitor, ManagedChild, ManagedRootCoordinator, ManagedRunRoot,
    OwnerKind, SystemAvailableSpace, WorkspaceHandler,
};
use camino::Utf8PathBuf;
use hoimin_core::{
    BaselineContext, BudgetLedger, DiskPolicy, EffectId, MutantContext, MutationCandidate,
    Preflight, ProcessFinished, ProcessLimits, ProcessTermination, ResetWorker, RunBudgets,
    RunConfig, RunProcess, reserve_workspace_copy,
};
use serde::Serialize;
use std::{collections::BTreeMap, ffi::OsString, sync::Arc, time::Duration};
use tokio::time::Instant;

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

// Normal cleanup can spend the unconsumed run budget. A stopped search gets
// only the shutdown grace, never extending the original deadline plus grace.
fn finalization_deadline(run_deadline: Instant, stopped: bool, now: Instant) -> Instant {
    let deadline = run_deadline
        .checked_add(SHUTDOWN_GRACE)
        .unwrap_or(run_deadline);
    if stopped {
        deadline.min(now + SHUTDOWN_GRACE)
    } else {
        deadline
    }
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct Observation {
    pub index: u64,
    pub timeout_ns: u128,
    pub effective_timeout_ns: u128,
    pub variant: u64,
    pub mutant: Option<String>,
    pub elapsed_ns: u128,
    pub termination: ProcessTermination,
    pub output: String,
    pub retained: u64,
    pub observed: u64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct Stop {
    pub reason: String,
    pub exit_code: i32,
}
impl Stop {
    #[allow(
        clippy::needless_pass_by_value,
        reason = "accepts owned errors directly as a map_err conversion"
    )]
    pub fn infrastructure(reason: impl ToString) -> Self {
        Self {
            reason: reason.to_string(),
            exit_code: 3,
        }
    }
    pub fn budget() -> Self {
        Self {
            reason: "total_timeout".into(),
            exit_code: 4,
        }
    }
}
pub(super) struct Runner {
    workspace: WorkspaceHandler,
    spool: Option<ManagedChild>,
    process: ProcessHandler,
    root: Arc<ManagedRunRoot>,
    monitor: DiskMonitor,
    inherited: BTreeMap<OsString, OsString>,
    config: RunConfig,
    output: Utf8PathBuf,
    pub artifacts: Artifacts,
    shutdown_deadline: Option<Instant>,
    root_closed: bool,
    monitor_stopped: bool,
    pub observations: Vec<Observation>,
    pub run_id: String,
    pub snapshot_hash: String,
    pub start: Instant,
    pub deadline: Instant,
    interrupts: crate::interrupt::InterruptMonitor,
    interrupt_seen: bool,
}
impl Runner {
    #[allow(
        clippy::too_many_lines,
        reason = "setup ownership and rollback remain visible together"
    )]
    pub async fn new(
        input: &Input,
        artifacts: Artifacts,
        start: Instant,
        deadline: Instant,
    ) -> Result<Self, Stop> {
        let config = input.config.clone();
        let output = Utf8PathBuf::from_path_buf(input.output.clone())
            .map_err(|_| Stop::infrastructure("output is not UTF-8"))?;
        let inherited = std::env::vars_os().collect();
        let (mut workspace, root, process, spool) = tokio::task::spawn_blocking(move || {
            let parent = Utf8PathBuf::from_path_buf(std::env::temp_dir())
                .map_err(|_| Stop::infrastructure("temporary path is not UTF-8"))?;
            let coordinator =
                ManagedRootCoordinator::open(&parent).map_err(Stop::infrastructure)?;
            let root = Arc::new(
                ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution)
                    .map_err(Stop::infrastructure)?,
            );
            let build = || {
                let mut workspace = WorkspaceHandler::new(
                    config.selection.root.clone(),
                    config.selection.sources.clone(),
                    1,
                    CopyOptions {
                        includes: config.selection.includes.clone(),
                        excludes: config.selection.excludes.clone(),
                        literal_exclusions: Vec::new(),
                    },
                )
                .with_import_roots(config.import_roots.clone())
                .with_managed_root(root.clone())
                .with_max_copy_bytes(config.limits.max_copy_size.get())
                .with_max_owned_bytes(config.limits.max_workspace_size.get());
                let completed = workspace
                    .handle_preflight(Preflight { id: EffectId(1) })
                    .map_err(|e| Stop::infrastructure(format!("{e:?}")))?;
                let mut ledger = BudgetLedger::new(RunBudgets {
                    memory: config.limits.max_memory.get(),
                    copy: config.limits.max_copy_size.get(),
                    processes: config.limits.max_processes.get() as u64,
                });
                let grant = reserve_workspace_copy(&mut ledger, &completed)
                    .map_err(Stop::infrastructure)?;
                workspace
                    .handle_create_worker(
                        grant
                            .create_worker(EffectId(2), 0)
                            .map_err(Stop::infrastructure)?,
                    )
                    .map_err(|e| Stop::infrastructure(format!("{e:?}")))?;
                let backend =
                    crate::shell::resource_backend(&config).map_err(Stop::infrastructure)?;
                let spool = root
                    .create_child("reduce-output-")
                    .map_err(Stop::infrastructure)?;
                // The parent root owns the output directory through final drain.
                let process = ProcessHandler::new(backend, spool.path().to_owned());
                Ok::<_, Stop>((workspace, process, spool))
            };
            match build() {
                Ok((workspace, process, spool)) => Ok((workspace, root, process, spool)),
                Err(e) => {
                    let _ = root.mark_cleanup_ready();
                    root.cleanup(Duration::from_secs(2));
                    Err(e)
                }
            }
        })
        .await
        .map_err(Stop::infrastructure)??;
        let manifest = workspace.worker(0).expect("created worker").manifest();
        let mut snapshot = blake3::Hasher::new();
        for entry in manifest.entries() {
            let name = entry.path.as_str().as_bytes();
            snapshot.update(&(name.len() as u64).to_le_bytes());
            snapshot.update(name);
            snapshot.update(&entry.size.to_le_bytes());
            snapshot.update(entry.blake3.as_bytes());
        }
        for directory in manifest.directories() {
            let name = directory.as_str().as_bytes();
            snapshot.update(&(name.len() as u64).to_le_bytes());
            snapshot.update(name);
        }
        let snapshot_hash = snapshot.finalize().to_hex().to_string();
        let setup = (|| {
            let meter = DiskMeter::new(
                vec![
                    root.disk_capability().map_err(Stop::infrastructure)?,
                    artifacts
                        .capability(output.clone())
                        .map_err(Stop::infrastructure)?,
                ],
                SystemAvailableSpace,
            );
            let monitor = DiskMonitor::start_unmeasured_with_meter(
                meter,
                DiskPolicy {
                    max_owned_bytes: input.config.limits.max_workspace_size,
                    min_free_bytes: input.config.limits.min_free_space,
                },
                vec![root.clone()],
                hoimin_core::DISK_SAMPLE_INTERVAL,
            )
            .map_err(Stop::infrastructure)?;
            Ok::<_, Stop>(monitor)
        })();
        let monitor = match setup {
            Ok(monitor) => monitor,
            Err(error) => {
                let _ = process.close();
                let _ = workspace.close();
                let _ = spool.cleanup();
                drop(spool);
                let _ = root.mark_cleanup_ready();
                root.cleanup(Duration::from_secs(2));
                return Err(error);
            }
        };
        let runner = Self {
            workspace,
            root,
            process,
            spool: Some(spool),
            monitor,
            inherited,
            config: input.config.clone(),
            output,
            artifacts,
            shutdown_deadline: None,
            root_closed: false,
            monitor_stopped: false,
            observations: Vec::new(),
            run_id: uuid::Uuid::new_v4().to_string(),
            snapshot_hash,
            start,
            deadline,
            interrupts: crate::interrupt::InterruptMonitor::spawn(),
            interrupt_seen: false,
        };
        Ok(runner)
    }
    pub fn normal_timeout(&self) -> Duration {
        self.config.limits.baseline_timeout.get()
    }
    pub fn tick(&self) -> u128 {
        self.start.elapsed().as_nanos()
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    pub fn check_deadline(&self) -> Result<(), Stop> {
        if Instant::now() >= self.deadline {
            Err(Stop::budget())
        } else {
            Ok(())
        }
    }
    #[allow(
        clippy::too_many_lines,
        reason = "cancellation, output preservation and process admission form one lifecycle"
    )]
    pub async fn execute(
        &mut self,
        variant: u64,
        files: &[(Utf8PathBuf, Vec<u8>)],
        mutant: Option<&MutationCandidate>,
        timeout: Duration,
    ) -> Result<Observation, Stop> {
        self.check_deadline()?;
        if let Some(failure) = self.monitor.sample_now().await {
            return Err(Stop {
                reason: format!("disk: {failure:?}"),
                exit_code: 4,
            });
        }
        self.workspace
            .handle_reset_worker(ResetWorker {
                id: EffectId(3),
                worker: 0,
            })
            .map_err(|e| Stop::infrastructure(format!("{e:?}")))?;
        let worker = self
            .workspace
            .worker_mut(0)
            .ok_or_else(|| Stop::infrastructure("worker unavailable after reset"))?;
        for (path, bytes) in files {
            worker.write(path, bytes).map_err(Stop::infrastructure)?;
        }
        if let Some(candidate) = mutant {
            worker
                .apply_mutation(candidate)
                .map_err(Stop::infrastructure)?;
        }
        let mut environment = self
            .workspace
            .command_environment(0, &self.inherited)
            .map_err(Stop::infrastructure)?;
        self.check_deadline()?;
        let index = self.observations.len() as u64;
        let cancellation = ProcessCancellation::new();
        crate::shell::set_execution_metadata(
            &mut environment,
            &self.run_id,
            mutant.map(|m| m.id.as_str()),
        );
        let limits = ProcessLimits {
            timeout: timeout.min(self.remaining()),
            max_output_bytes: self.config.limits.max_output.get(),
            max_memory_bytes: self.config.limits.max_memory.get(),
            max_processes: u32::try_from(self.config.limits.max_processes.get())
                .map_err(Stop::infrastructure)?,
        };
        let request = match mutant {
            Some(candidate) => self.request(
                index,
                MutantContext {
                    worker: 0,
                    run_id: self.run_id.clone(),
                    mutant_id: candidate.id.clone(),
                },
                environment,
                limits,
                cancellation.clone(),
            ),
            None => self.request(
                index,
                BaselineContext {
                    worker: 0,
                    run_id: self.run_id.clone(),
                },
                environment,
                limits,
                cancellation.clone(),
            ),
        };
        let mut disk = self.monitor.stop_receiver();
        if disk.borrow().is_some() {
            return Err(Stop {
                reason: format!("disk: {:?}", disk.borrow().as_ref()),
                exit_code: 4,
            });
        }
        let process = self.process.clone();
        let execution = process.run(request);
        tokio::pin!(execution);
        let (result, interrupted) = tokio::select! {
            biased;
            ()=tokio::time::sleep_until(self.deadline) => (Err(Stop::budget()),true),
            signal=self.interrupts.first() => {
                self.interrupt_seen = true;
                (Err(Stop {reason:signal.err().unwrap_or_else(||"cancelled".into()),exit_code:130}),true)
            },
            _=disk.changed() => (Err(Stop {reason:format!("disk: {:?}",disk.borrow().as_ref()),exit_code:4}),true),
            result=&mut execution => (result.map_err(|e|Stop::infrastructure(format!("{e:?}"))),false),
        };
        if interrupted {
            cancellation.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(2), &mut execution).await;
        }
        let finished: ProcessFinished = result?;
        let path = self
            .process
            .spool_path(&finished.output)
            .map_err(Stop::infrastructure)?;
        let output = format!("execution-{index}.bin");
        self.admit(finished.output.retained).await?;
        self.artifacts
            .copy(&output, path.as_std_path(), finished.output.retained)
            .map_err(Stop::infrastructure)?;
        self.sample().await?;
        std::fs::remove_file(path).map_err(Stop::infrastructure)?;
        let observation = Observation {
            index,
            timeout_ns: timeout.as_nanos(),
            effective_timeout_ns: limits.timeout.as_nanos(),
            variant,
            mutant: mutant.map(|m| m.id.clone()),
            elapsed_ns: finished.elapsed.as_nanos(),
            termination: finished.termination,
            output,
            retained: finished.output.retained,
            observed: finished.output.observed,
        };
        self.observations.push(observation.clone());
        self.check_deadline()?;
        if !finished.output_state.is_complete() {
            return Err(Stop::infrastructure("incomplete process output"));
        }
        if !matches!(observation.termination, ProcessTermination::Exit(_)) {
            return Err(Stop {
                reason: format!(
                    "process resource/termination: {:?}",
                    observation.termination
                ),
                exit_code: 4,
            });
        }
        if mutant.is_none() && observation.elapsed_ns == 0 {
            return Err(Stop::infrastructure(
                "measurement_error: observed zero duration",
            ));
        }
        Ok(observation)
    }
    fn request<C>(
        &self,
        index: u64,
        context: C,
        environment: crate::workspace::CommandEnvironment,
        limits: ProcessLimits,
        cancellation: ProcessCancellation,
    ) -> ProcessRequest
    where
        ProcessRequest: From<RunProcess<C>>,
    {
        ProcessRequest::from(RunProcess {
            id: EffectId(index + 10),
            context,
            argv: self.config.test_argv.clone(),
            cwd: environment.cwd.clone(),
            limits,
        })
        .with_environment(environment)
        .with_cancellation(cancellation)
    }
    /// Rechecks the complete copied manifest before discarding its authority.
    pub fn verify_originals(&self) -> Result<(), Stop> {
        self.workspace
            .handle_verify_originals(hoimin_core::VerifyOriginals {
                id: EffectId(4),
                checkpoint: hoimin_core::IntegrityCheckpoint::Periodic,
            })
            .map(|_| ())
            .map_err(|e| Stop::infrastructure(format!("{e:?}")))
    }
    fn measure(&self) -> Result<crate::workspace::MeterReading, Stop> {
        let mut roots = vec![
            self.artifacts
                .capability(self.output.clone())
                .map_err(Stop::infrastructure)?,
        ];
        if !self.root_closed {
            roots.push(self.root.disk_capability().map_err(Stop::infrastructure)?);
        }
        DiskMeter::new(roots, SystemAvailableSpace)
            .measure()
            .map_err(Stop::infrastructure)
    }
    pub async fn sample(&self) -> Result<(), Stop> {
        if !self.monitor_stopped
            && let Some(failure) = self.monitor.sample_now().await
        {
            return Err(Stop {
                reason: format!("disk: {failure:?}"),
                exit_code: 4,
            });
        }
        let observed = self.measure()?;
        if observed.owned_bytes > self.config.limits.max_workspace_size.get()
            || observed
                .available_by_filesystem
                .values()
                .any(|n| *n < self.config.limits.min_free_space.get())
        {
            return Err(Stop {
                reason: "disk policy exceeded".into(),
                exit_code: 4,
            });
        }
        Ok(())
    }
    async fn admit(&self, additional: u64) -> Result<(), Stop> {
        self.sample().await?;
        let observed = self.measure()?;
        let fits = observed
            .owned_bytes
            .checked_add(additional)
            .is_some_and(|n| n <= self.config.limits.max_workspace_size.get());
        let free = observed
            .available_by_filesystem
            .values()
            .all(|n| n.saturating_sub(additional) >= self.config.limits.min_free_space.get());
        if !fits || !free {
            return Err(Stop {
                reason: "disk: artifact admission exceeds resource policy".into(),
                exit_code: 4,
            });
        }
        Ok(())
    }
    /// Publication remains capability-relative and disk-admitted through its final
    /// write. Failed admission cannot create an applicable diff.
    pub async fn publish(&self, name: &str, bytes: &[u8]) -> Result<(), Stop> {
        self.admit(bytes.len() as u64).await?;
        self.artifacts
            .write(name, bytes)
            .map_err(Stop::infrastructure)?;
        if let Err(error) = self.sample().await {
            let _ = self.artifacts.remove(name);
            return Err(error);
        }
        Ok(())
    }
    pub fn environment_hash(&self) -> String {
        let mut hash = blake3::Hasher::new();
        for (name, value) in &self.inherited {
            for text in [name, value] {
                let bytes = text.as_encoded_bytes();
                hash.update(&(bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
            }
        }
        hash.finalize().to_hex().to_string()
    }
    // Poll between bounded shutdown waits/slices. A consumed first signal must
    // never be polled again; the monitor still owns second-signal escalation.
    async fn finalization_interrupt(&mut self) -> Option<Stop> {
        if self.interrupt_seen {
            return None;
        }
        tokio::select! {
            biased;
            signal = self.interrupts.first() => {
                self.interrupt_seen = true;
                let cutoff = finalization_deadline(self.deadline, true, Instant::now());
                self.shutdown_deadline = Some(self.shutdown_deadline.map_or(cutoff, |d| d.min(cutoff)));
                Some(Stop { reason: signal.err().unwrap_or_else(|| "cancelled".into()), exit_code: 130 })
            }
            () = std::future::ready(()) => None,
        }
    }
    /// Normal completion retains the total budget; an existing stop caps shutdown
    /// to its grace. The established deadline is shared with final cleanup.
    /// Stops dispatch and drains before releasing workspace/output children. The
    /// monitor observes execution until close; final artifact writes then use
    /// synchronous admission against the retained output-directory capability.
    pub async fn finish_execution(&mut self, stop: Option<&Stop>) -> Result<(), Stop> {
        self.shutdown_deadline.get_or_insert_with(|| {
            finalization_deadline(self.deadline, stop.is_some(), Instant::now())
        });
        let mut interrupted = self.finalization_interrupt().await;
        let deadline = self
            .shutdown_deadline
            .expect("established shutdown deadline");
        let drained = self
            .process
            .drain_for_shutdown(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(SHUTDOWN_GRACE),
            )
            .await;
        interrupted = interrupted.or(self.finalization_interrupt().await);
        if !drained.all_reaped
            || !drained.output_drains_joined
            || !drained.secondary_errors.is_empty()
        {
            self.workspace.retain_workers_for_janitor();
            self.root
                .abandon_for_janitor("reduce shutdown did not establish quiescence".into());
            return Err(Stop::infrastructure(format!("shutdown: {drained:?}")));
        }
        self.workspace.close().map_err(Stop::infrastructure)?;
        if let Some(spool) = self.spool.take() {
            spool.cleanup().map_err(Stop::infrastructure)?;
        }
        interrupted = interrupted.or(self.finalization_interrupt().await);
        interrupted.map_or(Ok(()), Err)
    }
    pub async fn close(&mut self) -> Result<(), Stop> {
        let mut interrupted = self.finalization_interrupt().await;
        let deadline = self
            .shutdown_deadline
            .unwrap_or_else(|| finalization_deadline(self.deadline, true, Instant::now()));
        let joined = self
            .monitor
            .stop_and_join(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(SHUTDOWN_GRACE),
            )
            .await;
        self.monitor_stopped = true;
        interrupted = interrupted.or(self.finalization_interrupt().await);
        if !joined {
            return Err(Stop::infrastructure("disk monitor did not stop"));
        }
        self.root
            .mark_cleanup_ready()
            .map_err(Stop::infrastructure)?;
        let mut progress = (0, 0);
        let record = loop {
            interrupted = interrupted.or(self.finalization_interrupt().await);
            let deadline = self.shutdown_deadline.unwrap_or(deadline);
            let record = self.root.cleanup(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(SHUTDOWN_GRACE),
            );
            // Partial slices keep their cursor in the root. Continue only after
            // actual progress or typed coordinator contention, within one budget.
            let observed = (
                record.evidence.examined_entries,
                record.evidence.removed_entries,
            );
            let progressed = matches!(
                record.disposition,
                crate::workspace::CleanupDisposition::Deferred(None)
            ) && (observed.0 > progress.0 || observed.1 > progress.1);
            progress = observed;
            tokio::task::yield_now().await;
            interrupted = interrupted.or(self.finalization_interrupt().await);
            let deadline = self.shutdown_deadline.unwrap_or(deadline);
            if (!record.retryable_contention() && !progressed) || Instant::now() >= deadline {
                break record;
            }
            tokio::time::sleep(
                Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
            )
            .await;
        };
        if !matches!(
            record.disposition,
            crate::workspace::CleanupDisposition::Clean
        ) {
            return Err(interrupted.map_or_else(
                || Stop::infrastructure(format!("cleanup: {record:?}")),
                |mut stop| {
                    stop.reason = format!("{}; cleanup: {record:?}", stop.reason);
                    stop
                },
            ));
        }
        self.root_closed = true;
        interrupted.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
