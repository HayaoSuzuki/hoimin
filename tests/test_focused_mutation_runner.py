from collections.abc import Callable
from datetime import datetime, timezone
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from typing import Any

from tools.focused_mutation_support.model import CommandRecord
from tools.focused_mutation_support.runner import (
    CommandInterrupted,
    CommandRunner,
    CommandTimedOut,
    wait_for_log_release,
)
from tools.focused_mutation_support.store import RunStore


FAKE = f"""#!{sys.executable}
import os, sys, time
print("OUT:" + "|".join(sys.argv[1:]), flush=True)
print("ERR:" + os.getcwd(), file=sys.stderr, flush=True)
if "--sleep" in sys.argv:
    time.sleep(30)
raise SystemExit(int(os.environ.get("FAKE_EXIT", "0")))
"""


class InterruptingProcess:
    pid = 12345
    returncode: int | None = None

    def __init__(self) -> None:
        self.wait_calls: list[float | None] = []
        self.terminated = False

    def wait(self, timeout: float | None = None) -> int:
        self.wait_calls.append(timeout)
        if len(self.wait_calls) == 1:
            raise KeyboardInterrupt
        self.returncode = -15
        return self.returncode

    def terminate(self) -> None:
        self.terminated = True

    def kill(self) -> None:
        raise AssertionError("kill should not be needed after successful termination")


class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0
        self.sleeps: list[float] = []

    def monotonic(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


class RunnerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name)
        self.work = root / "work"
        self.work.mkdir()
        self.output = root / "output"
        self.output.mkdir()
        self.store = RunStore(self.output)
        self.store.commands.mkdir()
        self.fake = root / "fake-command"
        self.fake.write_text(FAKE)
        self.fake.chmod(0o755)

    def runner(
        self,
        *,
        extra_env: dict[str, str] | None = None,
        popen_factory: Callable[..., Any] = subprocess.Popen,
    ) -> CommandRunner:
        return CommandRunner(
            self.store,
            extra_env=extra_env,
            popen_factory=popen_factory,
            utc_now=lambda: datetime(2026, 7, 26, tzinfo=timezone.utc),
        )

    def stdout(self, record: CommandRecord) -> str:
        return Path(record.stdout_path).read_text()

    def test_log_release_wait_retries_sharing_violation_until_success(self) -> None:
        clock = FakeClock()
        attempts = 0

        def probe(_: Path) -> None:
            nonlocal attempts
            attempts += 1
            if attempts < 3:
                error = OSError(32, "sharing violation")
                error.winerror = 32
                raise error

        errors = wait_for_log_release(
            [Path("stderr.log")],
            probe=probe,
            monotonic=clock.monotonic,
            sleep=clock.sleep,
            timeout=0.2,
            poll_interval=0.05,
        )

        self.assertEqual(errors, [])
        self.assertEqual(attempts, 3)
        self.assertEqual(clock.sleeps, [0.05, 0.05])

    def test_log_release_wait_reports_each_path_at_deadline(self) -> None:
        clock = FakeClock()

        def locked(_: Path) -> None:
            error = OSError(32, "sharing violation")
            error.winerror = 32
            raise error

        errors = wait_for_log_release(
            [Path("stdout.log"), Path("stderr.log")],
            probe=locked,
            monotonic=clock.monotonic,
            sleep=clock.sleep,
            timeout=0.1,
            poll_interval=0.05,
        )

        self.assertEqual(len(errors), 2)
        self.assertIn("stdout.log", errors[0])
        self.assertIn("stderr.log", errors[1])
        self.assertTrue(all("0.1 seconds" in error for error in errors))

    def test_log_release_wait_reports_nonsharing_error_without_retry(self) -> None:
        clock = FakeClock()

        def denied(path: Path) -> None:
            raise OSError(5, "access denied", str(path))

        errors = wait_for_log_release(
            [Path("stderr.log")],
            probe=denied,
            monotonic=clock.monotonic,
            sleep=clock.sleep,
        )

        self.assertEqual(len(errors), 1)
        self.assertIn("access denied", errors[0].lower())
        self.assertEqual(clock.sleeps, [])

    @unittest.skipUnless(os.name == "nt", "requires Windows file sharing")
    def test_delete_probe_reports_a_live_nonsharing_handle(self) -> None:
        from tools.focused_mutation_support.windows_file import (
            open_without_delete_sharing_for_tests,
            probe_delete_access,
        )

        path = self.output / "locked.log"
        path.write_bytes(b"partial")
        handle = open_without_delete_sharing_for_tests(path)
        self.addCleanup(handle.close)

        with self.assertRaises(OSError) as caught:
            probe_delete_access(path)

        self.assertEqual(caught.exception.winerror, 32)
        self.assertEqual(path.read_bytes(), b"partial")

    def assert_recorded_working_directory(self, record: CommandRecord) -> None:
        stderr = Path(record.stderr_path).read_text()
        self.assertTrue(stderr.startswith("ERR:"), stderr)
        recorded = Path(stderr.removeprefix("ERR:").strip())
        self.assertTrue(recorded.samefile(self.work), (recorded, self.work))

    def test_records_native_arguments_output_and_nonzero_exit(self) -> None:
        record = self.runner(extra_env={"FAKE_EXIT": "7"}).run(
            [sys.executable, str(self.fake), "a b", "$(never-run)"],
            cwd=self.work,
            timeout=5.0,
            label="baseline",
        )
        self.assertEqual(record.argv[-2:], ["a b", "$(never-run)"])
        self.assertEqual(record.exit_code, 7)
        self.assertEqual(record.started_at, "2026-07-26T00:00:00+00:00")
        self.assertEqual(record.ended_at, "2026-07-26T00:00:00+00:00")
        self.assertIn("OUT:a b|$(never-run)", self.stdout(record))
        self.assert_recorded_working_directory(record)

    def test_timeout_terminates_process_and_keeps_partial_logs(self) -> None:
        with self.assertRaises(CommandTimedOut) as caught:
            self.runner().run(
                [sys.executable, str(self.fake), "--sleep"],
                cwd=self.work,
                timeout=0.5,
                label="mutation",
            )
        record = caught.exception.record
        self.assertTrue(record.timed_out)
        self.assertIsNotNone(record.elapsed_seconds)
        self.assertIn("OUT:--sleep", self.stdout(record))
        self.assert_recorded_working_directory(record)

    def test_interruption_terminates_process_and_returns_completed_record(self) -> None:
        process = InterruptingProcess()
        popen_factory = mock.Mock(return_value=process)
        def invoke() -> None:
            self.runner(popen_factory=popen_factory).run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="interrupted",
            )

        if os.name == "nt":
            with self.assertRaises(CommandInterrupted) as caught:
                invoke()
        else:
            with (
                mock.patch(
                    "tools.focused_mutation_support.runner.os.killpg"
                ) as killpg,
                self.assertRaises(CommandInterrupted) as caught,
            ):
                invoke()
            killpg.assert_called_once_with(process.pid, 15)

        record = caught.exception.record
        self.assertTrue(record.interrupted)
        self.assertEqual(record.exit_code, -15)
        self.assertIsNotNone(record.ended_at)
        self.assertIsNotNone(record.elapsed_seconds)
        if os.name == "nt":
            self.assertTrue(process.terminated)
        self.assertEqual(process.wait_calls, [5.0, 2.0])


if __name__ == "__main__":
    unittest.main()
