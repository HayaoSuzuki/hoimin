use std::ffi::OsStr;
use std::fs;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::process::{ProcessCancellation, ProcessHandler, ProcessRequest};
use hoimin_cli::resource::{
    CgroupCapabilities, PortableBackend, ResourceBackend, parse_cgroup_event_counters,
    select_linux_backend,
};
use hoimin_core::{
    CommandArg, EffectFailure, EffectId, ProcessLimits, ProcessTermination, RawRunLimits,
    ResourceMode, RunLimits, RunProcess,
};

#[cfg(windows)]
use hoimin_cli::resource::WindowsBackend;
#[cfg(target_os = "linux")]
use hoimin_cli::resource::probe_linux_cgroup_with_launcher;

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

    use super::*;

    fn hard_handler(
        output_dir: &Utf8Path,
        max_memory: u64,
        max_processes: usize,
    ) -> Option<ProcessHandler> {
        let capabilities = probe_linux_cgroup_with_launcher(
            &hard_run_limits(max_memory, max_processes),
            OsString::from(env!("CARGO_BIN_EXE_hoimin")),
        );
        match capabilities {
            CgroupCapabilities::Available(backend) => Some(ProcessHandler::new(
                ResourceBackend::LinuxHard(backend),
                output_dir.to_owned(),
            )),
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

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    // SAFETY: signal 0 performs no mutation and accepts any numeric pid.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(unix)]
fn terminate_fixture_process(pid: u32) {
    // SAFETY: this is a best-effort test cleanup for the exact fixture pid.
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
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
            GetExitCodeProcess(handle, &mut exit_code) != 0 && exit_code == STILL_ACTIVE as u32;
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

async fn wait_until_process_stops(pid: u32) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while process_exists(pid) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    !process_exists(pid)
}

mod portable {
    use super::*;

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
            limits: limits(Duration::from_secs(1), 64),
        };

        let event = handler.handle(request).await.unwrap();
        let child_pid = guard.pid().expect("fixture child wrote its pid");

        assert_eq!(event.termination, ProcessTermination::Timeout);
        assert!(wait_until_process_stops(child_pid).await);
    }

    #[tokio::test]
    async fn cancellation_terminates_descendants() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("cancelled-fixture-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler(output_dir);
        let cancellation = ProcessCancellation::new();
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

        let (event, ()) = tokio::join!(handler.run(request), async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            while !pid_file.exists() && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            cancellation.cancel();
        });
        let child_pid = guard.pid().expect("fixture child wrote its pid");

        assert_eq!(event.unwrap().termination, ProcessTermination::Cancelled);
        assert!(wait_until_process_stops(child_pid).await);
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
        assert!(error.to_string().contains("--allow-best-effort-memory"));
        assert_eq!(
            PortableBackend::new(true).unwrap().mode(),
            ResourceMode::BestEffort
        );
    }
}

#[cfg(windows)]
mod job_object {
    use std::sync::Arc;

    use super::*;

    fn hard_handler(
        output_dir: &Utf8Path,
        max_memory: u64,
        max_processes: usize,
    ) -> ProcessHandler {
        let backend = WindowsBackend::new(&hard_run_limits(max_memory, max_processes)).unwrap();
        ProcessHandler::new(ResourceBackend::Windows(backend), output_dir.to_owned())
    }

    #[tokio::test]
    async fn job_object_classifies_memory_and_process_limits_without_leaking_state() {
        let output = tempfile::tempdir().unwrap();
        let handler = hard_handler(
            Utf8Path::from_path(output.path()).unwrap(),
            128 * 1024 * 1024,
            3,
        );

        assert_eq!(handler.mode(), ResourceMode::Hard);
        let memory = handler
            .handle(run_python(
                101,
                "chunks=[]\nwhile True: chunks.append(bytearray(8*1024*1024))",
                limits(Duration::from_secs(10), 64),
            ))
            .await
            .unwrap();
        let processes = handler
            .handle(run_python(
                102,
                "import subprocess,sys,time\nchildren=[]\nfor _ in range(8): children.append(subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']))",
                limits(Duration::from_secs(10), 4096),
            ))
            .await
            .unwrap();

        assert_eq!(memory.termination, ProcessTermination::OutOfMemory);
        assert_eq!(processes.termination, ProcessTermination::ProcessLimit);
        handler.close().unwrap();
    }

    #[tokio::test]
    async fn job_object_memory_limit_is_aggregate_across_concurrent_roots() {
        let output = tempfile::tempdir().unwrap();
        let handler = Arc::new(hard_handler(
            Utf8Path::from_path(output.path()).unwrap(),
            160 * 1024 * 1024,
            16,
        ));
        let first = handler.handle(run_python(
            103,
            "import time\nx=bytearray(96*1024*1024); x[::4096]=b'x'*(len(x[::4096])); time.sleep(0.7)",
            limits(Duration::from_secs(5), 64),
        ));
        let second = handler.handle(run_python(
            104,
            "import time\nx=bytearray(96*1024*1024); x[::4096]=b'x'*(len(x[::4096])); time.sleep(0.7)",
            limits(Duration::from_secs(5), 64),
        ));

        let (first, second) = tokio::join!(first, second);
        let terminations = [first.unwrap().termination, second.unwrap().termination];

        assert!(terminations.contains(&ProcessTermination::OutOfMemory));
        handler.close().unwrap();
    }

    #[tokio::test]
    async fn job_object_process_limit_is_aggregate_across_concurrent_roots() {
        let output = tempfile::tempdir().unwrap();
        let handler = Arc::new(hard_handler(
            Utf8Path::from_path(output.path()).unwrap(),
            512 * 1024 * 1024,
            3,
        ));
        let code = "import subprocess,sys,time\nchild=subprocess.Popen([sys.executable,'-c','import time; time.sleep(1)']); time.sleep(0.5); child.wait()";
        let first = handler.handle(run_python(105, code, limits(Duration::from_secs(5), 4096)));
        let second = handler.handle(run_python(106, code, limits(Duration::from_secs(5), 4096)));

        let (first, second) = tokio::join!(first, second);
        let terminations = [first.unwrap().termination, second.unwrap().termination];

        assert!(terminations.contains(&ProcessTermination::ProcessLimit));
        handler.close().unwrap();
    }

    #[tokio::test]
    async fn job_object_timeout_is_isolated_from_sibling_root() {
        let output = tempfile::tempdir().unwrap();
        let handler = Arc::new(hard_handler(
            Utf8Path::from_path(output.path()).unwrap(),
            512 * 1024 * 1024,
            16,
        ));
        let timed_out = handler.handle(run_python(
            107,
            "import time; time.sleep(30)",
            limits(Duration::from_millis(200), 64),
        ));
        let sibling = handler.handle(run_python(
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
        let handler = Arc::new(hard_handler(output_dir, 512 * 1024 * 1024, 16));
        let request = RunProcess {
            id: EffectId(109),
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
            limits: limits(Duration::from_secs(10), 64),
        };
        let running = handler.handle(request);
        let close = async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            while !pid_file.exists() && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            handler.close().unwrap();
        };

        let (finished, ()) = tokio::join!(running, close);
        finished.unwrap();
        let child_pid = guard.pid().expect("fixture child wrote its pid");
        assert!(wait_until_process_stops(child_pid).await);

        let failed = handler
            .handle(run_python(
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
