#!/usr/bin/env python3
"""Bounded review source-membership probe; writes only a temporary fixture."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--candidates", type=int, default=2000)
    parser.add_argument("--source-counts", type=int, nargs="+", default=[1000, 10000],
                        help="unused source records to add; optionally include 40000")
    parser.add_argument("--timeout", type=float, default=15.0,
                        help="seconds per subprocess")
    args = parser.parse_args()
    if args.candidates < 1 or any(n < 1 for n in args.source_counts) or args.timeout <= 0:
        parser.error("counts and timeout must be positive")
    binary = str(args.binary.resolve(strict=True))
    with open(binary, "rb") as executable:
        digest = hashlib.file_digest(executable, "sha256").hexdigest()
    print(json.dumps({"binary": binary, "sha256": digest}),
          flush=True)

    with tempfile.TemporaryDirectory(prefix="hoimin-review-scan-") as temporary:
        root = Path(temporary)
        (root / "src").mkdir()
        (root / "src/z.py").write_text(
            "".join(f"v{i} = left + right\n" for i in range(args.candidates)),
            encoding="utf-8",
        )
        plan_command = [binary, "plan", "--root", str(root), "--source", "src",
                        "--operators", "binary_add_sub", "--max-candidates",
                        str(args.candidates + 1), "--allow-best-effort-memory",
                        "--", sys.executable, "-m", "pytest"]
        print(json.dumps({"plan_command": plan_command}), flush=True)
        result = subprocess.run(plan_command, capture_output=True, text=True,
                                timeout=args.timeout, check=True)
        plan = json.loads(result.stdout)
        assert len(plan["candidates"]) == args.candidates
        assert len(plan["sources"]) == 1
        matching_sources = plan["sources"]

        for count in args.source_counts:
            # read_for_review validates these records' paths/hashes and membership;
            # unused source records need not exist for this reader to accept them.
            unused = [{"path": f"src/a{i:06}.py", "hash": "0" * 64}
                      for i in range(count)]
            for position in ("first", "last"):
                plan["sources"] = (matching_sources + unused if position == "first"
                                   else unused + matching_sources)
                manifest = root / f"plan-{count}-{position}.json"
                manifest.write_text(json.dumps(plan, separators=(",", ":")),
                                    encoding="utf-8")
                command = [binary, "review", "list", str(manifest), "--store",
                           str(root / "absent-reviews.sqlite3"), "--format", "json"]
                start = time.monotonic()
                try:
                    result = subprocess.run(command, stdout=subprocess.DEVNULL,
                                            stderr=subprocess.PIPE, text=True,
                                            timeout=args.timeout)
                except subprocess.TimeoutExpired:
                    print(json.dumps({"sources": count + 1, "matching_source": position,
                                      "result": "timeout; subprocess killed",
                                      "timeout_seconds": args.timeout}), flush=True)
                    return 1
                print(json.dumps({"candidates": args.candidates, "sources": count + 1,
                                  "matching_source": position,
                                  "bytes": manifest.stat().st_size,
                                  "seconds": round(time.monotonic() - start, 4),
                                  "exit": result.returncode, "stderr": result.stderr}),
                      flush=True)
                if result.returncode != 0:
                    return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
