use std::ffi::OsStr;
use std::fs;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::process::{ProcessCancellation, ProcessHandler, ProcessRequest};
use hoimin_cli::resource::{
    CgroupCapabilities, PortableBackend, ResourceBackend, parse_cgroup_event_counters,
    select_linux_backend,
};
#[cfg(unix)]
use hoimin_core::ProcessOutputState;
use hoimin_core::{
    CommandArg, EffectFailure, EffectId, ProcessLimits, ProcessTermination, ResourceMode,
    RunProcess,
};
use serde::Deserialize;

const OUTPUT_RETENTION_CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/output-retention.jsonl");

#[derive(Deserialize)]
struct PublicOutputAuditCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    capacity: u64,
    chunks: Vec<Vec<u8>>,
    expected_observed: u64,
    expected_retained: u64,
    expected_bytes: Vec<u8>,
}

#[cfg(windows)]
use hoimin_cli::resource::WindowsBackend;
#[cfg(target_os = "linux")]
use hoimin_cli::resource::probe_linux_cgroup_with_launcher;
#[cfg(target_os = "linux")]
use hoimin_core::{RawRunLimits, RunLimits};

#[cfg(unix)]
fn native_arg(value: &OsStr) -> CommandArg {
    use std::os::unix::ffi::OsStrExt;
    CommandArg::Unix(value.as_bytes().to_vec())
}

#[cfg(windows)]
fn native_arg(value: &OsStr) -> CommandArg {
    use std::os::windows::ffi::OsStrExt;
    CommandArg::Windows(value.encode_wide().collect())
}

fn utf8_arg(value: &str) -> CommandArg {
    native_arg(OsStr::new(value))
}

fn python_executable() -> CommandArg {
    if let Some(configured) = std::env::var_os("PYTHON") {
        return native_arg(&configured);
    }
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for candidate in [
        workspace.join(".venv/Scripts/python.exe"),
        workspace.join(".venv/bin/python"),
    ] {
        if candidate.is_file() {
            return native_arg(candidate.as_os_str());
        }
    }
    #[cfg(windows)]
    return utf8_arg("python");
    #[cfg(not(windows))]
    utf8_arg("python3")
}

fn limits(timeout: Duration, max_output_bytes: u64) -> ProcessLimits {
    ProcessLimits {
        timeout,
        max_output_bytes,
        max_memory_bytes: 256 * 1024 * 1024,
        max_processes: 8,
    }
}

fn run_python(id: u64, code: &str, limits: ProcessLimits) -> RunProcess {
    RunProcess {
        id: EffectId(id),
        worker: None,
        run_id: None,
        mutant_id: None,
        argv: vec![python_executable(), utf8_arg("-c"), utf8_arg(code)],
        cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
        limits,
    }
}

fn portable_handler(output_dir: &Utf8Path) -> ProcessHandler {
    ProcessHandler::new(
        ResourceBackend::Portable(PortableBackend::for_tests()),
        output_dir.to_owned(),
    )
}

fn portable_handler_with_termination_failure(output_dir: &Utf8Path) -> ProcessHandler {
    ProcessHandler::new(
        ResourceBackend::Portable(PortableBackend::for_tests_with_termination_failure()),
        output_dir.to_owned(),
    )
}

fn portable_handler_with_classification_failure(output_dir: &Utf8Path) -> ProcessHandler {
    ProcessHandler::new(
        ResourceBackend::Portable(PortableBackend::for_tests_with_classification_failure()),
        output_dir.to_owned(),
    )
}

fn portable_handler_with_classification_and_termination_failure(
    output_dir: &Utf8Path,
) -> ProcessHandler {
    ProcessHandler::new(
        ResourceBackend::Portable(
            PortableBackend::for_tests_with_classification_and_termination_failure(),
        ),
        output_dir.to_owned(),
    )
}

#[cfg(target_os = "linux")]
fn hard_run_limits(max_memory: u64, max_processes: usize) -> RunLimits {
    let raw = RawRunLimits {
        max_memory,
        max_processes,
        ..RawRunLimits::default()
    };
    RunLimits::try_from(&raw).expect("valid hard run limits")
}

mod linux_policy {
    use super::*;

    #[test]
    fn unavailable_cgroup_requires_explicit_best_effort_opt_in() {
        let unavailable =
            CgroupCapabilities::Unavailable("delegated cgroup subtree is not writable".to_owned());

        let error = select_linux_backend(unavailable.clone(), false).unwrap_err();
        assert!(error.to_string().contains("--allow-best-effort-memory"));
        assert!(
            error
                .to_string()
                .contains("delegated cgroup subtree is not writable")
        );

        let backend = select_linux_backend(unavailable, true).unwrap();
        assert_eq!(backend.mode(), ResourceMode::BestEffort);
        assert_eq!(
            backend.diagnostic(),
            Some("delegated cgroup subtree is not writable")
        );
    }

    #[test]
    fn parses_memory_and_process_event_counters_by_name() {
        let counters = parse_cgroup_event_counters(
            b"low 2\nhigh 3\nmax 5\noom 7\noom_kill 11\noom_group_kill 13\n",
            b"max 17\n",
        )
        .unwrap();

        assert_eq!(counters.memory_max, 5);
        assert_eq!(counters.oom, 7);
        assert_eq!(counters.oom_kill, 11);
        assert_eq!(counters.oom_group_kill, 13);
        assert_eq!(counters.pids_max, 17);
    }
}

#[cfg(target_os = "linux")]
mod cgroup_v2 {
    use std::ffi::OsString;
    use std::sync::Arc;

    use hoimin_cli::resource::LinuxBackend;

    use super::*;

    fn hard_backend(max_memory: u64, max_processes: usize) -> Option<LinuxBackend> {
        let capabilities = probe_linux_cgroup_with_launcher(
            &hard_run_limits(max_memory, max_processes),
            OsString::from(env!("CARGO_BIN_EXE_hoimin")),
        );
        match capabilities {
            CgroupCapabilities::Available(backend) => Some(backend),
            CgroupCapabilities::Unavailable(reason) => {
                eprintln!("SKIP: Linux cgroup v2 hard-limit capability unavailable: {reason}");
                None
            }
            CgroupCapabilities::CleanupPending(pending) => {
                panic!(
                    "Linux cgroup probe cleanup remained pending: {}",
                    pending.reason()
                )
            }
        }
    }

    fn hard_handler(
        output_dir: &Utf8Path,
        max_memory: u64,
        max_processes: usize,
    ) -> Option<ProcessHandler> {
        hard_backend(max_memory, max_processes).map(|backend| {
            let handler =
                ProcessHandler::new(ResourceBackend::LinuxHard(backend), output_dir.to_owned());
            assert_eq!(handler.resource_control().mode, ResourceMode::Hard);
            assert_eq!(handler.resource_control().mechanism, "linux_cgroup_v2");
            handler
        })
    }

    fn hard_handler_without_swap(
        output_dir: &Utf8Path,
        max_memory: u64,
        max_processes: usize,
    ) -> Option<ProcessHandler> {
        let backend = hard_backend(max_memory, max_processes)?;
        let swap_max = backend.run_cgroup_path_for_tests().join("memory.swap.max");
        if let Err(error) = fs::write(&swap_max, b"0") {
            backend.close().unwrap();
            eprintln!(
                "SKIP: Linux cgroup v2 swap limiting unavailable: write {} failed: {error}",
                swap_max.display()
            );
            return None;
        }
        let actual = match fs::read_to_string(&swap_max) {
            Ok(actual) => actual,
            Err(error) => {
                backend.close().unwrap();
                eprintln!(
                    "SKIP: Linux cgroup v2 swap limiting unavailable: read {} failed: {error}",
                    swap_max.display()
                );
                return None;
            }
        };
        if actual.trim() != "0" {
            let cleanup = backend.close();
            panic!(
                "Linux cgroup v2 memory.swap.max readback mismatch: requested 0, read {:?}; cleanup: {cleanup:?}",
                actual.trim()
            );
        }

        Some(ProcessHandler::new(
            ResourceBackend::LinuxHard(backend),
            output_dir.to_owned(),
        ))
    }

    #[tokio::test]
    async fn hard_cgroup_classifies_one_root_oom_kill() {
        let output = tempfile::tempdir().unwrap();
        let Some(handler) = hard_handler_without_swap(
            Utf8Path::from_path(output.path()).unwrap(),
            512 * 1024 * 1024,
            16,
        ) else {
            return;
        };
        let mut process_limits = limits(Duration::from_secs(5), 64);
        process_limits.max_memory_bytes = 160 * 1024 * 1024;

        let event = handler
            .handle(run_python(
                210,
                "chunks=[]\nfor _ in range(12):\n chunk=bytearray(16*1024*1024)\n for page in range(0,len(chunk),4096): chunk[page]=1\n chunks.append(chunk)",
                process_limits,
            ))
            .await
            .unwrap();

        assert_eq!(event.resource_mode, ResourceMode::Hard);
        assert_eq!(event.termination, ProcessTermination::OutOfMemory);
        handler.close().unwrap();
    }

    #[test]
    fn decimal_memory_limit_keeps_hard_cgroup_enforcement() {
        let requested = 1_000_000_000;
        let capabilities = probe_linux_cgroup_with_launcher(
            &hard_run_limits(requested, 16),
            OsString::from(env!("CARGO_BIN_EXE_hoimin")),
        );
        let backend = match capabilities {
            CgroupCapabilities::Available(backend) => backend,
            CgroupCapabilities::Unavailable(reason)
                if reason.contains("cgroup limit readback mismatch") =>
            {
                panic!("non-page-aligned memory limit must not disable hard cgroups: {reason}")
            }
            CgroupCapabilities::Unavailable(reason) => {
                eprintln!("SKIP: Linux cgroup v2 hard-limit capability unavailable: {reason}");
                return;
            }
            CgroupCapabilities::CleanupPending(pending) => {
                panic!(
                    "Linux cgroup probe cleanup remained pending: {}",
                    pending.reason()
                )
            }
        };
        // SAFETY: `_SC_PAGESIZE` is a side-effect-free process configuration query.
        let page_size = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).unwrap();
        let expected = requested - requested % page_size;
        let actual = fs::read_to_string(backend.run_cgroup_path_for_tests().join("memory.max"))
            .unwrap()
            .trim()
            .parse::<u64>()
            .unwrap();

        assert_eq!(actual, expected);
        assert!(backend.diagnostics().iter().any(|diagnostic| {
            diagnostic.contains(&requested.to_string())
                && diagnostic.contains(&expected.to_string())
                && diagnostic.contains(&page_size.to_string())
        }));
        backend.close().unwrap();
    }

    #[tokio::test]
    async fn removes_run_cgroup_and_rejects_future_spawn_after_close() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let capabilities = probe_linux_cgroup_with_launcher(
            &hard_run_limits(512 * 1024 * 1024, 16),
            OsString::from(env!("CARGO_BIN_EXE_hoimin")),
        );
        let backend = match capabilities {
            CgroupCapabilities::Available(backend) => backend,
            CgroupCapabilities::Unavailable(reason) => {
                eprintln!("SKIP: Linux cgroup v2 hard-limit capability unavailable: {reason}");
                return;
            }
            CgroupCapabilities::CleanupPending(pending) => {
                panic!(
                    "Linux cgroup probe cleanup remained pending: {}",
                    pending.reason()
                )
            }
        };
        let run_path = backend.run_cgroup_path_for_tests();
        let handler = ProcessHandler::new(ResourceBackend::LinuxHard(backend), output_dir.into());

        handler.close().unwrap();

        assert!(!run_path.exists());
        let failure = handler
            .handle(run_python(
                200,
                "raise SystemExit(0)",
                limits(Duration::from_secs(1), 64),
            ))
            .await
            .unwrap_err();
        assert!(matches!(
            failure.failure,
            EffectFailure::Io { ref code, ref message, .. }
                if code == "process.resource.setup" && message.contains("closed")
        ));
    }

    #[tokio::test]
    async fn hard_cgroup_preserves_non_utf8_argv_without_a_shell() {
        let output = tempfile::tempdir().unwrap();
        let Some(handler) = hard_handler(
            Utf8Path::from_path(output.path()).unwrap(),
            512 * 1024 * 1024,
            16,
        ) else {
            return;
        };
        let raw = vec![b'n', b'o', b'n', b'-', 0xff, b'-', b'u', b't', b'f', b'8'];
        let request = RunProcess {
            id: EffectId(201),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg("import os,sys; sys.stdout.buffer.write(os.fsencode(sys.argv[1]))"),
                CommandArg::Unix(raw.clone()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(5), 64),
        };

        let event = handler.handle(request).await.unwrap();

        assert_eq!(event.resource_mode, ResourceMode::Hard);
        assert_eq!(event.termination, ProcessTermination::Exit(0));
        assert_eq!(
            fs::read(handler.spool_path(&event.output).unwrap()).unwrap(),
            raw
        );
        handler.close().unwrap();
    }

    #[tokio::test]
    async fn timeout_kills_a_setsid_descendant_without_killing_a_sibling_root() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let Some(handler) = hard_handler(output_dir, 512 * 1024 * 1024, 16) else {
            return;
        };
        let handler = Arc::new(handler);
        let pid_file = output_dir.join("setsid-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let timed_out = handler.handle(RunProcess {
            id: EffectId(202),
        worker: None,
        run_id: None,
        mutant_id: None,
        argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg("import os,pathlib,subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'],preexec_fn=os.setsid); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)"),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(1), 64),
        });
        let sibling = handler.handle(run_python(
            203,
            "import sys,time; time.sleep(1.3); sys.stdout.write('alive')",
            limits(Duration::from_secs(5), 64),
        ));

        let (timed_out, sibling) = tokio::join!(timed_out, sibling);
        let child_pid = guard.pid().expect("fixture descendant wrote pid");

        assert_eq!(timed_out.unwrap().termination, ProcessTermination::Timeout);
        let sibling = sibling.unwrap();
        assert_eq!(sibling.termination, ProcessTermination::Exit(0));
        assert_eq!(
            fs::read(handler.spool_path(&sibling.output).unwrap()).unwrap(),
            b"alive"
        );
        assert!(wait_until_process_stops(child_pid).await);
        handler.close().unwrap();
    }

    #[tokio::test]
    async fn abnormal_runtime_root_is_classified_and_cleaned_within_six_seconds() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let Some(handler) = hard_handler(output_dir, 512 * 1024 * 1024, 16) else {
            return;
        };
        let ready_file = output_dir.join("aborting-root.ready");
        let code = "import os,pathlib,signal,sys; ready=pathlib.Path(sys.argv[1]); pending=ready.with_suffix('.pending'); pending.write_text(str(os.getpid())); os.replace(pending,ready); os.kill(os.getpid(),signal.SIGABRT)";
        let started = Instant::now();

        let (result, close) = tokio::time::timeout(Duration::from_secs(6), async {
            let result = handler
                .handle(RunProcess {
                    id: EffectId(208),
                    worker: None,
                    run_id: None,
                    mutant_id: None,
                    argv: vec![
                        python_executable(),
                        utf8_arg("-c"),
                        utf8_arg(code),
                        native_arg(ready_file.as_std_path().as_os_str()),
                    ],
                    cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
                    limits: limits(Duration::from_secs(5), 64),
                })
                .await;
            let close = handler.close();
            (result, close)
        })
        .await
        .expect("abnormal root handling and cgroup cleanup exceeded six seconds");

        assert!(started.elapsed() < Duration::from_secs(6));
        assert!(
            ready_file.exists(),
            "runtime target never published readiness"
        );
        assert_eq!(
            result.unwrap().termination,
            ProcessTermination::Exit(128 + libc::SIGABRT)
        );
        close.unwrap();
    }

    #[tokio::test]
    async fn exited_root_is_observed_before_its_descendant_is_cleaned() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let capabilities = probe_linux_cgroup_with_launcher(
            &hard_run_limits(512 * 1024 * 1024, 16),
            OsString::from(env!("CARGO_BIN_EXE_hoimin")),
        );
        let backend = match capabilities {
            CgroupCapabilities::Available(backend) => backend,
            CgroupCapabilities::Unavailable(reason) => {
                eprintln!("SKIP: Linux cgroup v2 hard-limit capability unavailable: {reason}");
                return;
            }
            CgroupCapabilities::CleanupPending(pending) => {
                panic!(
                    "Linux cgroup probe cleanup remained pending: {}",
                    pending.reason()
                )
            }
        };
        let run_path = backend.run_cgroup_path_for_tests();
        let handler = ProcessHandler::new(ResourceBackend::LinuxHard(backend), output_dir.into());
        let root_pid_file = output_dir.join("exiting-root.pid");
        let descendant_pid_file = output_dir.join("persistent-descendant.pid");
        let release_file = output_dir.join("release-root");
        let root_guard = FixtureChildGuard::new(root_pid_file.clone());
        let descendant_guard = FixtureChildGuard::new(descendant_pid_file.clone());
        let code = "import os,pathlib,subprocess,sys,time; root=pathlib.Path(sys.argv[1]); descendant=pathlib.Path(sys.argv[2]); release=pathlib.Path(sys.argv[3]); child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True); pending=descendant.with_suffix('.pending'); pending.write_text(str(child.pid)); os.replace(pending,descendant); pending=root.with_suffix('.pending'); pending.write_text(str(os.getpid())); os.replace(pending,root);\nwhile not release.exists(): time.sleep(0.005)";
        let started = Instant::now();

        tokio::time::timeout(Duration::from_secs(6), async {
            let handle = handler.handle(RunProcess {
                id: EffectId(209),
                worker: None,
                run_id: None,
                mutant_id: None,
                argv: vec![
                    python_executable(),
                    utf8_arg("-c"),
                    utf8_arg(code),
                    native_arg(root_pid_file.as_std_path().as_os_str()),
                    native_arg(descendant_pid_file.as_std_path().as_os_str()),
                    native_arg(release_file.as_std_path().as_os_str()),
                ],
                cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
                limits: limits(Duration::from_secs(5), 64),
            });
            tokio::pin!(handle);

            let (root_pid, descendant_pid) = loop {
                if let (Some(root_pid), Some(descendant_pid)) =
                    (root_guard.pid(), descendant_guard.pid())
                {
                    break (root_pid, descendant_pid);
                }
                tokio::select! {
                    result = &mut handle => panic!("root completed before publishing fixture PIDs: {result:?}"),
                    () = tokio::time::sleep(Duration::from_millis(5)) => {}
                }
            };
            fs::write(&release_file, b"exit").unwrap();

            assert!(
                wait_until_process_is_zombie(root_pid).await,
                "runtime root did not reach an observed exited state"
            );
            assert!(
                process_exists(descendant_pid),
                "descendant was not alive after the root exit was observed"
            );

            let result = (&mut handle).await;
            assert_eq!(result.unwrap().termination, ProcessTermination::Exit(0));
            assert!(wait_until_process_stops(descendant_pid).await);
            handler.close().unwrap();
            assert!(!process_exists(descendant_pid));
            assert!(!run_path.exists());
        })
        .await
        .expect("root-before-descendant handling and cgroup cleanup exceeded six seconds");
        assert!(started.elapsed() < Duration::from_secs(6));
    }

    #[tokio::test]
    async fn run_cgroup_classifies_concurrent_aggregate_memory_and_process_limits() {
        let memory_output = tempfile::tempdir().unwrap();
        let Some(memory_handler) = hard_handler(
            Utf8Path::from_path(memory_output.path()).unwrap(),
            160 * 1024 * 1024,
            16,
        ) else {
            return;
        };
        let memory_handler = Arc::new(memory_handler);
        let code = "import time; x=bytearray(96*1024*1024); x[::4096]=b'x'*(len(x[::4096])); time.sleep(2)";
        let (first, second) = tokio::join!(
            memory_handler.handle(run_python(204, code, limits(Duration::from_secs(5), 64))),
            memory_handler.handle(run_python(205, code, limits(Duration::from_secs(5), 64)))
        );
        assert!(
            [first.unwrap().termination, second.unwrap().termination]
                .contains(&ProcessTermination::OutOfMemory)
        );
        memory_handler.close().unwrap();

        let process_output = tempfile::tempdir().unwrap();
        let Some(process_handler) = hard_handler(
            Utf8Path::from_path(process_output.path()).unwrap(),
            512 * 1024 * 1024,
            3,
        ) else {
            return;
        };
        let process_handler = Arc::new(process_handler);
        let code = "import subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(2)']); time.sleep(1); child.wait()";
        let (first, second) = tokio::join!(
            process_handler.handle(run_python(206, code, limits(Duration::from_secs(5), 4096))),
            process_handler.handle(run_python(207, code, limits(Duration::from_secs(5), 4096)))
        );
        assert!(
            [first.unwrap().termination, second.unwrap().termination]
                .contains(&ProcessTermination::ProcessLimit)
        );
        process_handler.close().unwrap();
    }
}

struct FixtureChildGuard {
    pid_file: Utf8PathBuf,
}

const REAL_PROCESS_FIXTURE_BUDGET: Duration = Duration::from_secs(6);
const REAL_PROCESS_TIMEOUT: Duration = Duration::from_secs(5);

impl FixtureChildGuard {
    fn new(pid_file: Utf8PathBuf) -> Self {
        Self { pid_file }
    }

    fn pid(&self) -> Option<u32> {
        fs::read_to_string(&self.pid_file)
            .ok()
            .and_then(|value| value.trim().parse().ok())
    }
}

impl Drop for FixtureChildGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid().filter(|pid| process_exists(*pid)) {
            terminate_fixture_process(pid);
        }
    }
}

async fn wait_for_fixture_pid(guard: &FixtureChildGuard, deadline: tokio::time::Instant) -> u32 {
    loop {
        if let Some(pid) = guard.pid() {
            return pid;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "fixture child did not become ready before the test deadline: {}",
            guard.pid_file
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
fn unix_pid(pid: u32) -> Option<i32> {
    i32::try_from(pid).ok()
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    unix_pid(pid).is_some_and(|pid| {
        // SAFETY: signal 0 performs no mutation and accepts a validated Unix pid.
        unsafe { libc::kill(pid, 0) == 0 }
    })
}

#[cfg(unix)]
fn terminate_fixture_process(pid: u32) {
    if let Some(pid) = unix_pid(pid) {
        // SAFETY: this is a best-effort test cleanup for the exact validated fixture pid.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

#[cfg(unix)]
#[test]
fn unix_pid_rejects_values_outside_the_platform_pid_range() {
    assert_eq!(unix_pid(0), Some(0));
    assert_eq!(unix_pid(u32::MAX), None);
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: the handle is checked and closed on every successful open.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code = 0;
        let active =
            GetExitCodeProcess(handle, &raw mut exit_code) != 0 && exit_code == STILL_ACTIVE as u32;
        CloseHandle(handle);
        active
    }
}

#[cfg(windows)]
fn terminate_fixture_process(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    // SAFETY: the handle is checked, used only for fixture termination, and closed.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
    }
}

#[cfg(target_os = "linux")]
async fn wait_until_process_stops(pid: u32) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    wait_until_process_stops_before(pid, deadline).await
}

async fn wait_until_process_stops_before(pid: u32, deadline: tokio::time::Instant) -> bool {
    while process_exists(pid) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    !process_exists(pid)
}

#[cfg(target_os = "linux")]
async fn wait_until_process_is_zombie(pid: u32) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| {
                stat.rsplit_once(") ")
                    .map(|(_, status)| status.starts_with('Z'))
            })
            .unwrap_or(false)
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    false
}

mod portable {
    use super::*;

    #[tokio::test]
    async fn public_process_matches_strict_lean_output_boundaries() {
        let cases = OUTPUT_RETENTION_CORPUS
            .lines()
            .map(|line| serde_json::from_str::<PublicOutputAuditCase>(line).unwrap())
            .filter(|case| case.mode == "strict" && case.scenario == "success")
            .collect::<Vec<_>>();
        assert_eq!(cases.len(), 7);

        for (index, case) in cases.into_iter().enumerate() {
            assert_eq!(case.schema, 1, "{}", case.id);
            let stream = case.chunks.concat();
            let byte_list = stream
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let script = format!(
                "import sys;sys.stdout.buffer.write(bytes([{byte_list}]));sys.stdout.flush()"
            );
            let output = tempfile::tempdir().unwrap();
            let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
            let event = handler
                .handle(run_python(
                    u64::try_from(index + 500).unwrap(),
                    &script,
                    limits(Duration::from_secs(5), case.capacity),
                ))
                .await
                .unwrap();

            assert_eq!(
                event.termination,
                ProcessTermination::Exit(0),
                "{}",
                case.id
            );
            assert_eq!(event.output.observed, case.expected_observed, "{}", case.id);
            assert_eq!(event.output.retained, case.expected_retained, "{}", case.id);
            assert_eq!(
                fs::read(handler.spool_path(&event.output).unwrap()).unwrap(),
                case.expected_bytes,
                "{}",
                case.id
            );
        }
    }

    #[tokio::test]
    async fn output_is_drained_after_retention_cap() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let request = run_python(
            1,
            "import sys; sys.stdout.buffer.write(b'x' * 2000000); sys.stdout.flush()",
            limits(Duration::from_secs(5), 1024),
        );

        let event = handler.handle(request).await.unwrap();

        assert_eq!(event.output.retained, 1024);
        assert_eq!(event.output.observed, 2_000_000);
        assert!(event.output.observed > event.output.retained);
        assert_eq!(
            fs::metadata(handler.spool_path(&event.output).unwrap())
                .unwrap()
                .len(),
            1024
        );
    }

    #[tokio::test]
    async fn truncated_output_retains_the_child_process_tail() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let request = run_python(
            101,
            "import sys; sys.stdout.buffer.write(b'HEAD-' + b'x' * 2000 + b'-TAIL'); sys.stdout.flush()",
            limits(Duration::from_secs(5), 128),
        );

        let event = handler.handle(request).await.unwrap();
        let retained = fs::read(handler.spool_path(&event.output).unwrap()).unwrap();

        assert_eq!(retained.len(), 128);
        assert!(retained.starts_with(b"\n[... hoimin output truncated ...]\n"));
        assert!(retained.ends_with(b"-TAIL"));
        assert_eq!(event.output.retained, 128);
        assert_eq!(event.output.observed, 2010);
    }

    #[tokio::test]
    async fn drains_stdout_and_stderr_under_combined_cap() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let request = run_python(
            2,
            "import sys; sys.stdout.buffer.write(b'o'*800); sys.stdout.flush(); sys.stderr.buffer.write(b'e'*800); sys.stderr.flush()",
            limits(Duration::from_secs(5), 1024),
        );

        let event = handler.handle(request).await.unwrap();

        assert_eq!(event.output.retained, 1024);
        assert_eq!(event.output.observed, 1600);
        assert_eq!(
            fs::read(handler.spool_path(&event.output).unwrap())
                .unwrap()
                .len(),
            1024
        );
    }

    #[tokio::test]
    async fn spools_non_utf8_output() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let request = run_python(
            3,
            "import sys; sys.stdout.buffer.write(bytes([255, 254, 0, 128])); sys.stdout.flush()",
            limits(Duration::from_secs(5), 64),
        );

        let event = handler.handle(request).await.unwrap();

        assert_eq!(event.output.retained, 4);
        assert_eq!(event.output.observed, 4);
        assert_eq!(
            fs::read(handler.spool_path(&event.output).unwrap()).unwrap(),
            [255, 254, 0, 128]
        );
    }

    #[tokio::test]
    async fn returns_zero_and_nonzero_exit() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());

        let zero = handler
            .handle(run_python(
                4,
                "raise SystemExit(0)",
                limits(Duration::from_secs(5), 64),
            ))
            .await
            .unwrap();
        let nonzero = handler
            .handle(run_python(
                5,
                "raise SystemExit(7)",
                limits(Duration::from_secs(5), 64),
            ))
            .await
            .unwrap();

        assert_eq!(zero.id, EffectId(4));
        assert_eq!(zero.termination, ProcessTermination::Exit(0));
        assert_eq!(zero.resource_mode, ResourceMode::BestEffort);
        assert_eq!(nonzero.id, EffectId(5));
        assert_eq!(nonzero.termination, ProcessTermination::Exit(7));
        let drain = handler.drain_for_shutdown(Duration::from_secs(1)).await;
        assert!(drain.all_reaped);
        assert!(drain.output_drains_joined);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn naturally_exited_portable_root_does_not_claim_live_descendants_are_reaped() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("natural-exit-descendant.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler(output_dir);
        let deadline = tokio::time::Instant::now() + REAL_PROCESS_FIXTURE_BUDGET;
        let request = RunProcess {
            id: EffectId(207),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL); pathlib.Path(sys.argv[1]).write_text(str(child.pid))",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(5), 64),
        };

        let (event, descendant_pid) = tokio::join!(
            handler.handle(request),
            wait_for_fixture_pid(&guard, deadline)
        );

        assert_eq!(event.unwrap().termination, ProcessTermination::Exit(0));
        assert!(process_exists(descendant_pid));
        let drain = handler.drain_for_shutdown(Duration::from_secs(1)).await;
        assert!(
            !drain.all_reaped,
            "a live portable process group must keep destructive cleanup deferred"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn preserves_explicit_152_and_sigxcpu_as_native_exits() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let explicit = handler
            .handle(run_python(
                71,
                "raise SystemExit(152)",
                limits(Duration::from_secs(5), 64),
            ))
            .await
            .unwrap();
        let signal = handler
            .handle(run_python(
                72,
                "import os,signal\nsignal.signal(signal.SIGXCPU, signal.SIG_DFL)\nsignal.pthread_sigmask(signal.SIG_UNBLOCK, {signal.SIGXCPU})\nos.kill(os.getpid(), signal.SIGXCPU)",
                limits(Duration::from_secs(5), 64),
            ))
            .await
            .unwrap();

        assert_eq!(explicit.termination, ProcessTermination::Exit(152));
        assert_eq!(
            signal.termination,
            ProcessTermination::Exit(128 + libc::SIGXCPU),
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn escaped_descendant_output_timeout_is_a_mutant_completion() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("escaped-output-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler(output_dir);
        let code = "import pathlib,subprocess,sys; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'],start_new_session=True); pathlib.Path(sys.argv[1]).write_text(str(child.pid))";
        let request = RunProcess {
            id: EffectId(52),
            worker: Some(0),
            run_id: Some("run".to_owned()),
            mutant_id: Some("mutant".to_owned()),
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(code),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(5), 64),
        };

        let event = handler.handle(request).await.unwrap();
        let descendant = guard.pid().expect("escaped descendant wrote its pid");

        assert_eq!(event.termination, ProcessTermination::Exit(0));
        assert_eq!(event.output_state, ProcessOutputState::CloseTimedOut);
        assert!(process_exists(descendant));
        assert_eq!(event.output.retained, 0);
        assert_eq!(event.output.observed, 0);
    }

    #[tokio::test]
    async fn passes_native_arguments_without_a_shell() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let expected = "space & | $() ; literal";
        let request = RunProcess {
            id: EffectId(51),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg("import os,sys; sys.stdout.buffer.write(os.fsencode(sys.argv[1]))"),
                utf8_arg(expected),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(5), 128),
        };

        let event = handler.handle(request).await.unwrap();

        assert_eq!(event.termination, ProcessTermination::Exit(0));
        assert_eq!(
            fs::read(handler.spool_path(&event.output).unwrap()).unwrap(),
            expected.as_bytes()
        );
    }

    #[tokio::test]
    async fn returns_cancelled() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let cancellation = ProcessCancellation::new();
        let request = ProcessRequest::from(run_python(
            6,
            "import time\nwhile True: time.sleep(1)",
            limits(Duration::from_secs(10), 64),
        ))
        .with_cancellation(cancellation.clone());

        let (event, ()) = tokio::join!(handler.run(request), async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancellation.cancel();
        });

        assert_eq!(event.unwrap().termination, ProcessTermination::Cancelled);
    }

    #[tokio::test]
    async fn cancellation_before_run_never_spawns_the_child() {
        let output = tempfile::tempdir().unwrap();
        let marker = output.path().join("spawned");
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let cancellation = ProcessCancellation::new();
        cancellation.cancel();
        let code = format!(
            "from pathlib import Path; Path({:?}).write_text('spawned')",
            marker.to_string_lossy()
        );
        let request =
            ProcessRequest::from(run_python(60, &code, limits(Duration::from_secs(5), 64)))
                .with_cancellation(cancellation);

        let failure = handler.run(request).await.unwrap_err();

        assert_eq!(failure.id, EffectId(60));
        assert_eq!(failure.failure.code(), "process.cancelled.before_spawn");
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn maps_spawn_failure_to_effect_failed() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let request = RunProcess {
            id: EffectId(7),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![utf8_arg("definitely-missing-hoimin-executable")],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(1), 64),
        };

        let failed = handler.handle(request).await.unwrap_err();

        assert_eq!(failed.id, EffectId(7));
        assert!(matches!(
            failed.failure,
            EffectFailure::Io { ref code, ref operation, .. }
                if code == "process.spawn" && operation == "spawn process"
        ));
    }

    #[tokio::test]
    async fn honors_requested_timeout() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let timeout = Duration::from_millis(200);
        let started = Instant::now();

        let event = handler
            .handle(run_python(
                8,
                "import time\ntime.sleep(30)",
                limits(timeout, 64),
            ))
            .await
            .unwrap();

        assert_eq!(event.termination, ProcessTermination::Timeout);
        assert!(started.elapsed() >= timeout);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(event.elapsed >= timeout);
    }

    #[tokio::test]
    async fn timeout_terminates_descendants() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("fixture-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler(output_dir);
        let deadline = tokio::time::Instant::now() + REAL_PROCESS_FIXTURE_BUDGET;
        let request = RunProcess {
            id: EffectId(9),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(REAL_PROCESS_TIMEOUT, 64),
        };

        let (event, child_pid) = tokio::join!(
            handler.handle(request),
            wait_for_fixture_pid(&guard, deadline)
        );
        let event = event.unwrap();

        assert_eq!(event.termination, ProcessTermination::Timeout);
        assert!(wait_until_process_stops_before(child_pid, deadline).await);
        let drain = handler.drain_for_shutdown(Duration::from_secs(1)).await;
        assert!(drain.all_reaped);
        assert!(drain.output_drains_joined);
    }

    #[tokio::test]
    async fn cancellation_terminates_descendants() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("cancelled-fixture-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler(output_dir);
        let cancellation = ProcessCancellation::new();
        let deadline = tokio::time::Instant::now() + REAL_PROCESS_FIXTURE_BUDGET;
        let request = ProcessRequest::from(RunProcess {
            id: EffectId(10),
        worker: None,
        run_id: None,
        mutant_id: None,
        argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys,time; time.sleep(0.2); child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(10), 64),
        })
        .with_cancellation(cancellation.clone());

        let (event, child_pid) = tokio::join!(handler.run(request), async {
            let child_pid = wait_for_fixture_pid(&guard, deadline).await;
            cancellation.cancel();
            child_pid
        });

        assert_eq!(event.unwrap().termination, ProcessTermination::Cancelled);
        assert!(wait_until_process_stops_before(child_pid, deadline).await);
        let drain = handler.drain_for_shutdown(Duration::from_secs(1)).await;
        assert!(drain.all_reaped);
        assert!(drain.output_drains_joined);
    }

    #[tokio::test]
    async fn cancellation_reaps_after_injected_termination_failure() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("termination-failure-fixture-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler_with_termination_failure(output_dir);
        let cancellation = ProcessCancellation::new();
        let deadline = tokio::time::Instant::now() + REAL_PROCESS_FIXTURE_BUDGET;
        let request = ProcessRequest::from(RunProcess {
            id: EffectId(11),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys,time; time.sleep(0.2); child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(10), 64),
        })
        .with_cancellation(cancellation.clone());

        let (event, (cleanup_started, child_pid)) = tokio::join!(handler.run(request), async {
            let child_pid = wait_for_fixture_pid(&guard, deadline).await;
            let cleanup_started = Instant::now();
            cancellation.cancel();
            (cleanup_started, child_pid)
        });
        let cleanup_elapsed = cleanup_started.elapsed();

        let failure = event.expect_err("injected supervisor failure remains observable");
        assert!(matches!(
            failure.failure,
            EffectFailure::Io { ref code, ref message, .. }
                if code == "process.resource.terminate"
                    && message.contains("injected portable termination failure")
        ));
        assert!(wait_until_process_stops_before(child_pid, deadline).await);
        assert!(
            cleanup_elapsed < Duration::from_millis(900),
            "cancellation cleanup took {cleanup_elapsed:?}"
        );
    }

    #[tokio::test]
    async fn timeout_reaps_after_injected_termination_failure() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("timeout-termination-failure-fixture-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler_with_termination_failure(output_dir);
        let timeout = REAL_PROCESS_TIMEOUT;
        let deadline = tokio::time::Instant::now() + REAL_PROCESS_FIXTURE_BUDGET;
        let request = RunProcess {
            id: EffectId(12),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(timeout, 64),
        };
        let started = Instant::now();

        let (event, child_pid) = tokio::join!(
            handler.handle(request),
            wait_for_fixture_pid(&guard, deadline)
        );
        let total_elapsed = started.elapsed();
        let cleanup_elapsed = total_elapsed.saturating_sub(timeout);

        let failure = event.expect_err("injected supervisor failure remains observable");
        assert!(matches!(
            failure.failure,
            EffectFailure::Io { ref code, ref message, .. }
                if code == "process.resource.terminate"
                    && message.contains("injected portable termination failure")
        ));
        assert!(wait_until_process_stops_before(child_pid, deadline).await);
        assert!(
            cleanup_elapsed < Duration::from_millis(900),
            "timeout cleanup took {cleanup_elapsed:?} ({total_elapsed:?} total)"
        );
    }

    #[tokio::test]
    async fn classification_failure_handles_descendants_after_the_root_is_reaped() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("classification-failure-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler_with_classification_failure(output_dir);
        let deadline = tokio::time::Instant::now() + REAL_PROCESS_FIXTURE_BUDGET;
        let request = RunProcess {
            id: EffectId(13),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid))",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(5), 64),
        };
        let (failure, (cleanup_started, child_pid)) =
            tokio::join!(handler.handle(request), async {
                let child_pid = wait_for_fixture_pid(&guard, deadline).await;
                (Instant::now(), child_pid)
            });
        let failure = failure.expect_err("injected classification failure remains observable");
        let cleanup_elapsed = cleanup_started.elapsed();

        assert!(matches!(
            failure.failure,
            EffectFailure::Io { ref code, ref message, .. }
                if code == "process.resource.classify"
                    && message.contains("injected portable classification failure")
        ));
        #[cfg(unix)]
        assert!(
            process_exists(child_pid),
            "the portable Unix backend must not signal a process group after its root is reaped"
        );
        #[cfg(windows)]
        assert!(wait_until_process_stops_before(child_pid, deadline).await);
        #[cfg(windows)]
        assert!(
            cleanup_elapsed < Duration::from_millis(900),
            "classification cleanup took {cleanup_elapsed:?}"
        );
        #[cfg(unix)]
        let _ = cleanup_elapsed;
    }

    #[tokio::test]
    async fn classification_failure_preserves_termination_failure_detail() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("classification-and-termination-failure-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler_with_classification_and_termination_failure(output_dir);
        let deadline = tokio::time::Instant::now() + REAL_PROCESS_FIXTURE_BUDGET;
        let request = RunProcess {
            id: EffectId(14),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid))",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(5), 64),
        };

        let (failure, child_pid) = tokio::join!(
            handler.handle(request),
            wait_for_fixture_pid(&guard, deadline)
        );
        let failure = failure.expect_err("classification remains primary");

        assert!(matches!(
            failure.failure,
            EffectFailure::Io { ref code, ref message, .. }
                if code == "process.resource.classify"
                    && message.contains("supervised termination also failed")
                    && message.contains("injected portable termination failure")
        ));
        #[cfg(unix)]
        assert!(
            process_exists(child_pid),
            "the portable Unix backend must not signal a process group after its root is reaped"
        );
        #[cfg(windows)]
        assert!(wait_until_process_stops_before(child_pid, deadline).await);
    }

    #[test]
    fn portable_backend_reports_best_effort() {
        assert_eq!(
            PortableBackend::for_tests().mode(),
            ResourceMode::BestEffort
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn normal_linux_portable_backend_requires_explicit_opt_in() {
        let error = PortableBackend::new(false).unwrap_err();
        let message = error.to_string();
        assert_eq!(
            message,
            "portable resource limits require --allow-best-effort-memory: portable Linux uses per-process RLIMIT_AS and process groups",
        );
        assert!(!message.contains("RLIMIT_CPU"));
        assert_eq!(
            PortableBackend::new(true).unwrap().mode(),
            ResourceMode::BestEffort
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_portable_backend_requires_explicit_best_effort_opt_in() {
        let error = PortableBackend::new(false).unwrap_err();
        assert!(error.to_string().contains("--allow-best-effort-memory"));

        let backend = PortableBackend::new(true).unwrap();
        assert_eq!(backend.mode(), ResourceMode::BestEffort);
        assert_eq!(
            backend.diagnostic(),
            Some("macOS uses process groups; max-memory is not enforced"),
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_portable_backend_spawns_without_virtual_memory_rlimit() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());

        let event = handler
            .handle(run_python(
                70,
                "raise SystemExit(0)",
                limits(Duration::from_secs(5), 64),
            ))
            .await
            .unwrap();

        assert_eq!(event.termination, ProcessTermination::Exit(0));
    }
}

#[cfg(windows)]
mod job_object {
    use std::sync::Arc;

    use super::*;

    fn hard_handler(output_dir: &Utf8Path) -> ProcessHandler {
        let backend = WindowsBackend::new().unwrap();
        let handler = ProcessHandler::new(ResourceBackend::Windows(backend), output_dir.to_owned());
        assert_eq!(handler.resource_control().mode, ResourceMode::Hard);
        assert_eq!(handler.resource_control().mechanism, "windows_job_object");
        handler
    }

    fn native_python() -> CommandArg {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let configuration = fs::read_to_string(root.join(".venv/pyvenv.cfg")).unwrap();
        let home = configuration
            .lines()
            .find_map(|line| line.strip_prefix("home = "))
            .unwrap();
        let executable = std::path::Path::new(home).join("python.exe");
        assert!(executable.is_file());
        native_arg(executable.as_os_str())
    }

    fn native_run_python(id: u64, code: &str, limits: ProcessLimits) -> RunProcess {
        let mut request = super::run_python(id, code, limits);
        request.argv[0] = native_python();
        request
    }

    fn root_limits(memory: u64, processes: u32) -> ProcessLimits {
        ProcessLimits {
            max_memory_bytes: memory,
            max_processes: processes,
            ..limits(Duration::from_secs(15), 4096)
        }
    }

    #[tokio::test]
    async fn job_object_classifies_memory_and_process_limits_without_leaking_state() {
        let output = tempfile::tempdir().unwrap();
        let handler = hard_handler(Utf8Path::from_path(output.path()).unwrap());

        assert_eq!(handler.mode(), ResourceMode::Hard);
        let memory = handler
            .handle(native_run_python(
                101,
                "chunks=[bytearray(8*1024*1024) for _ in range(32)]",
                root_limits(128 * 1024 * 1024, 3),
            ))
            .await
            .unwrap();
        let processes = handler
            .handle(native_run_python(
                102,
                "import subprocess,sys,time\nchildren=[]\nfor _ in range(8): children.append(subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']))",
                root_limits(128 * 1024 * 1024, 3),
            ))
            .await
            .unwrap();

        assert_eq!(memory.termination, ProcessTermination::OutOfMemory);
        assert_eq!(processes.termination, ProcessTermination::ProcessLimit);
        handler.close().unwrap();
    }

    async fn assert_offender_isolated(memory: bool) {
        let output = tempfile::tempdir().unwrap();
        let directory = Utf8Path::from_path(output.path()).unwrap();
        let handler = hard_handler(directory);
        let offender_pid = directory.join("offender.pid");
        let healthy_pid = directory.join("healthy.pid");
        let offender_guard = FixtureChildGuard::new(offender_pid.clone());
        let healthy_guard = FixtureChildGuard::new(healthy_pid.clone());
        let release = directory.join("release");
        let extra_pid = directory.join("extra.pid");
        let extra_guard = FixtureChildGuard::new(extra_pid.clone());
        let fixture = |own: &Utf8Path, peer: &Utf8Path, action: &str| {
            format!(
                "import pathlib,subprocess,sys,time\nchild=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'])\npathlib.Path({own:?}).write_text(str(child.pid))\ndeadline=time.monotonic()+10\nwhile not pathlib.Path({peer:?}).exists():\n if time.monotonic()>=deadline: raise TimeoutError('sibling not ready')\n time.sleep(0.01)\n{action}"
            )
        };
        let action = if memory {
            "try:\n chunks=[bytearray(8*1024*1024) for _ in range(32)]\nexcept MemoryError:\n time.sleep(2)".to_owned()
        } else {
            format!(
                "try:\n for _ in range(3):\n  extra=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'])\n  pathlib.Path({extra_pid:?}).write_text(str(extra.pid))\nexcept OSError:\n time.sleep(2)"
            )
        };
        let offender_code = fixture(&offender_pid, &healthy_pid, &action);
        let healthy_code = fixture(
            &healthy_pid,
            &offender_pid,
            &format!(
                "while not pathlib.Path({release:?}).exists():\n if time.monotonic()>=deadline: raise TimeoutError('offender not classified')\n time.sleep(0.01)\nsys.stdout.write('healthy')"
            ),
        );
        let offender = async {
            let result = handler
                .handle(native_run_python(
                    103,
                    &offender_code,
                    root_limits(128 * 1024 * 1024, 3),
                ))
                .await
                .unwrap();
            fs::write(&release, b"classified").unwrap();
            result
        };
        let healthy = handler.handle(native_run_python(
            104,
            &healthy_code,
            root_limits(128 * 1024 * 1024, 3),
        ));
        let (offender, healthy) = tokio::join!(offender, healthy);
        let healthy = healthy.unwrap();
        assert_eq!(
            offender.termination,
            if memory {
                ProcessTermination::OutOfMemory
            } else {
                ProcessTermination::ProcessLimit
            }
        );
        assert_eq!(healthy.termination, ProcessTermination::Exit(0));
        assert_eq!(
            fs::read(handler.spool_path(&healthy.output).unwrap()).unwrap(),
            b"healthy"
        );
        let mut descendants = vec![&offender_guard, &healthy_guard];
        if !memory {
            descendants.push(&extra_guard);
        }
        let cleanup_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        for guard in descendants {
            let pid = guard
                .pid()
                .expect("descendant published before limit violation");
            assert!(
                wait_until_process_stops_before(pid, cleanup_deadline).await,
                "descendant {pid} survived root cleanup"
            );
        }
        handler.close().unwrap();
    }

    #[tokio::test]
    async fn job_object_memory_offender_preserves_healthy_sibling_and_cleans_descendants() {
        assert_offender_isolated(true).await;
    }

    #[tokio::test]
    async fn job_object_process_offender_preserves_healthy_sibling_and_cleans_descendants() {
        assert_offender_isolated(false).await;
    }

    #[tokio::test]
    async fn job_object_timeout_is_isolated_from_sibling_root() {
        let output = tempfile::tempdir().unwrap();
        let handler = Arc::new(hard_handler(Utf8Path::from_path(output.path()).unwrap()));
        let timed_out = handler.handle(native_run_python(
            107,
            "import time; time.sleep(30)",
            limits(Duration::from_millis(200), 64),
        ));
        let sibling = handler.handle(native_run_python(
            108,
            "import sys,time; time.sleep(0.7); sys.stdout.write('alive')",
            limits(Duration::from_secs(5), 64),
        ));

        let (timed_out, sibling) = tokio::join!(timed_out, sibling);
        let timed_out = timed_out.unwrap();
        let sibling = sibling.unwrap();

        assert_eq!(timed_out.termination, ProcessTermination::Timeout);
        assert_eq!(sibling.termination, ProcessTermination::Exit(0));
        assert_eq!(
            fs::read(handler.spool_path(&sibling.output).unwrap()).unwrap(),
            b"alive"
        );
        handler.close().unwrap();
    }

    #[tokio::test]
    async fn job_object_close_terminates_all_descendants_and_rejects_future_spawn() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("hard-close-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = Arc::new(hard_handler(output_dir));
        let deadline = tokio::time::Instant::now() + REAL_PROCESS_FIXTURE_BUDGET;
        let request = RunProcess {
            id: EffectId(109),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                native_python(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(10), 64),
        };
        let running = handler.handle(request);
        let close = async {
            let child_pid = wait_for_fixture_pid(&guard, deadline).await;
            handler.close().unwrap();
            child_pid
        };

        let (finished, child_pid) = tokio::join!(running, close);
        finished.unwrap();
        assert!(wait_until_process_stops_before(child_pid, deadline).await);

        let failed = handler
            .handle(native_run_python(
                110,
                "raise SystemExit(0)",
                limits(Duration::from_secs(1), 64),
            ))
            .await
            .unwrap_err();
        assert_eq!(failed.id, EffectId(110));
        assert!(matches!(
            failed.failure,
            EffectFailure::Io { ref code, .. } if code == "process.resource.setup"
        ));
    }
}

#[test]
fn portable_backend_description_is_stable_and_reaches_process_handler() {
    let backend = ResourceBackend::Portable(PortableBackend::new(true).unwrap());
    let policy = backend.resource_control();
    assert_eq!(policy.mode, ResourceMode::BestEffort);
    assert_eq!(policy.mechanism, "portable");
    let temp = tempfile::tempdir().unwrap();
    let handler = ProcessHandler::new(
        backend,
        Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap(),
    );
    assert_eq!(handler.resource_control(), policy);
    assert_eq!(handler.mode(), policy.mode);
}
