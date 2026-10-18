"""Bounded macOS observations for Issue 978; no fixed RSS pass threshold."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=Path, required=True)
parser.add_argument("--revision", required=True)
parser.add_argument("--repetitions", type=int, default=3)
args = parser.parse_args()
binary = args.binary.resolve(strict=True)
print(json.dumps({
    "binary": str(binary),
    "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "revision": args.revision,
    "profile": "release; default features; incremental=0",
    "scope": "public plan CLI; RSS is supplementary, not charged payload",
}), flush=True)

with tempfile.TemporaryDirectory(prefix="hoimin-978-large-") as temporary:
    root = Path(temporary)
    source = root / "source.py"
    for count in (8, 16, 32):
        source.write_text("items = [" + ",".join(
            repr(str(i) + "x" * (1200000 // count)) for i in range(count)
        ) + "]\n")
        source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        command = [str(binary), "plan", "--root", str(root), "--file", "source.py",
                   "--operators", "container_element_delete", "--allow-best-effort-memory",
                   "--", "python3", "-c", "pass"]
        for repetition in range(args.repetitions):
            with tempfile.TemporaryFile() as error:
                start = time.perf_counter()
                child = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=error)
                timed_out = threading.Event()

                def stop_child():
                    timed_out.set()
                    child.kill()

                timer = threading.Timer(20, stop_child)
                timer.start()
                try:
                    _, status, usage = os.wait4(child.pid, 0)
                    child.returncode = os.waitstatus_to_exitcode(status)
                finally:
                    timer.cancel()
                    timer.join()
                elapsed = time.perf_counter() - start
                error.seek(0)
                stderr = error.read().decode()
            print(json.dumps({
                "elements": count, "source_bytes": source.stat().st_size,
                "source_sha256": source_hash, "repetition": repetition,
                "seconds": elapsed, "cpu_seconds": usage.ru_utime + usage.ru_stime,
                "exit": child.returncode, "watchdog_fired": timed_out.is_set(),
                "peak_rss_macos_bytes": usage.ru_maxrss, "stderr": stderr,
            }), flush=True)
