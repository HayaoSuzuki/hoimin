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
