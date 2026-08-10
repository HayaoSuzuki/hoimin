import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GUARD = ROOT / "formal" / "HoiminOracle" / "tools" / "lean_resource_guard.py"


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
                "payload=bytearray(32*1024*1024); "
                f"open({str(marker)!r},'w').write(str(os.getpid())); time.sleep(30)"
            )

            completed, stats = self.run_guard(
                sys.executable,
                "-c",
                allocating_child,
                timeout=3,
                rss_limit_mib=8,
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


if __name__ == "__main__":
    unittest.main()
