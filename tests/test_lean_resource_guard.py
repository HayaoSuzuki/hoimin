import importlib.util
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
GUARD = ROOT / "formal" / "HoiminOracle" / "tools" / "lean_resource_guard.py"


class LeanResourceGuardSetupTests(unittest.TestCase):
    def test_rejects_unsupported_process_groups_before_spawning(self):
        spec = importlib.util.spec_from_file_location("lean_resource_guard", GUARD)
        assert spec is not None and spec.loader is not None
        guard = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(guard)
        with tempfile.TemporaryDirectory() as temporary_directory:
            stats = Path(temporary_directory) / "stats.json"
            marker = Path(temporary_directory) / "spawned"
            arguments = SimpleNamespace(
                timeout_seconds=1.0,
                rss_limit_mib=8,
                sample_ms=25,
                stats=stats,
                command=[sys.executable, "-c", f"open({str(marker)!r},'w').close()"],
            )

            with mock.patch.object(guard.os, "name", "unsupported"):
                exit_code = guard.run(arguments)

            self.assertEqual(exit_code, 126)
            self.assertEqual(json.loads(stats.read_text())["reason"], "monitor_error")
            self.assertFalse(marker.exists())


@unittest.skipUnless(
    os.name == "posix" and shutil.which("ps"),
    "Lean RSS guard requires POSIX process groups and ps",
)
class LeanResourceGuardTests(unittest.TestCase):
    def run_guard(self, *command: str, timeout: float, rss_limit_mib: int):
        temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(temporary_directory.cleanup)
        stats = Path(temporary_directory.name) / "stats.json"
        completed = subprocess.run(
            [
                sys.executable,
                str(GUARD),
                "--timeout-seconds",
                str(timeout),
                "--rss-limit-mib",
                str(rss_limit_mib),
                "--sample-ms",
                "25",
                "--stats",
                str(stats),
                "--",
                *command,
            ],
            check=False,
            timeout=5,
        )
        return completed, json.loads(stats.read_text())

    def test_terminates_process_group_at_rss_limit(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            marker = Path(temporary_directory) / "child.pid"
            allocating_child = (
                "import os,time; "
                f"open({str(marker)!r},'w').write(str(os.getpid())); "
                "time.sleep(.1); payload=bytearray(64*1024*1024); time.sleep(30)"
            )

            completed, stats = self.run_guard(
                sys.executable,
                "-c",
                allocating_child,
                timeout=3,
                rss_limit_mib=32,
            )

            self.assertEqual(completed.returncode, 125)
            self.assertEqual(stats["reason"], "rss_limit")
            child_pid = int(marker.read_text())
            for _ in range(20):
                try:
                    os.kill(child_pid, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.05)
            else:
                self.fail(f"guard left child process {child_pid} alive")

    def test_terminates_process_group_at_timeout(self):
        completed, stats = self.run_guard(
            sys.executable,
            "-c",
            "import time; time.sleep(30)",
            timeout=0.2,
            rss_limit_mib=768,
        )

        self.assertEqual(completed.returncode, 124)
        self.assertEqual(stats["reason"], "timeout")

    def test_propagates_normal_child_exit(self):
        completed, stats = self.run_guard(
            sys.executable,
            "-c",
            "import sys; sys.exit(7)",
            timeout=3,
            rss_limit_mib=768,
        )

        self.assertEqual(completed.returncode, 7)
        self.assertEqual(stats["reason"], "child_exit")

    def test_rejects_root_exit_that_leaves_a_descendant(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            marker = Path(temporary_directory) / "descendant.pid"
            descendant = (
                "import os,signal,time; "
                "signal.signal(signal.SIGTERM,signal.SIG_IGN); "
                f"open({str(marker)!r},'w').write(str(os.getpid())); time.sleep(30)"
            )
            spawning_root = (
                "import os,subprocess,sys,time\n"
                f"subprocess.Popen([sys.executable,'-c',{descendant!r}])\n"
                "deadline=time.monotonic()+2\n"
                f"marker={str(marker)!r}\n"
                "while not os.path.exists(marker) and time.monotonic()<deadline:\n"
                "    time.sleep(.01)\n"
            )

            completed, stats = self.run_guard(
                sys.executable,
                "-c",
                spawning_root,
                timeout=3,
                rss_limit_mib=768,
            )

            self.assertEqual(completed.returncode, 126)
            self.assertEqual(stats["reason"], "monitor_error")
            descendant_pid = int(marker.read_text())
            for _ in range(20):
                try:
                    os.kill(descendant_pid, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.05)
            else:
                os.kill(descendant_pid, signal.SIGKILL)
                self.fail(f"guard left descendant process {descendant_pid} alive")


if __name__ == "__main__":
    unittest.main()
