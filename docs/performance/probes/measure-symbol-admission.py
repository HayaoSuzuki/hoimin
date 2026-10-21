"""Offline macOS release CLI comparison for Issue 1060; run from the repo root."""

import argparse
import datetime
import hashlib
import json
import platform
import re
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--baseline", required=True, type=Path)
parser.add_argument("--candidate", required=True, type=Path)
parser.add_argument("--output", required=True, type=Path)
parser.add_argument(
    "--baseline-revision", help="Caller-verified source revision, if known"
)
parser.add_argument(
    "--candidate-comparison-revision",
    help="Caller-verified candidate comparison revision, if known",
)
parser.add_argument("--candidate-working-tree", choices=["clean", "modified"])
parser.add_argument(
    "--build-description", help="Caller-verified build command, if known"
)
args = parser.parse_args()
if platform.system() != "Darwin":
    parser.error("this probe uses macOS /usr/bin/time -l and byte-valued RSS")
if args.output.exists():
    parser.error("output must be a new file")
binaries = {
    "baseline": args.baseline.resolve(strict=True),
    "candidate": args.candidate.resolve(strict=True),
}
result = {
    "schema_version": 1,
    "observed_on": datetime.datetime.now().astimezone().date().isoformat(),
    "baseline_revision": args.baseline_revision,
    "candidate_comparison_revision": args.candidate_comparison_revision,
    "candidate_working_tree": args.candidate_working_tree,
    "environment": {
        "platform": platform.platform(),
        "python": sys.version,
        "rust": subprocess.check_output(["rustc", "--version"], text=True).strip(),
    },
    "build": args.build_description,
    "provenance_note": "Revision and build fields are caller declarations, not inferred from binary hashes. Checkout source hashes describe the current checkout, not attested binary provenance.",
    "binaries": {
        name: {
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "bytes": path.stat().st_size,
        }
        for name, path in binaries.items()
    },
    "checkout_source_sha256": {
        p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
        for p in [
            "crates/hoimin-python/src/analyzer/rust/selection.rs",
            "crates/hoimin-cli/src/progress/duplicate.rs",
            "crates/hoimin-cli/src/artifacts/run_report.rs",
            "crates/hoimin-cli/src/artifacts/run_report/dispatch.rs",
            "crates/hoimin-cli/src/resource/portable.rs",
            "crates/hoimin-cli/src/workspace/copy.rs",
        ]
    },
    "method": "One discarded warmup per binary/case; five paired samples, alternating execution order. Whole CLI plan wall time includes /usr/bin/time/process startup; Darwin maximum resident set size is bytes. No builds/tests during samples. No timing pass/fail threshold or confidence interval.",
    "limits": "One process invocation at a time; 20 second external timeout; generated inputs at most 512 functions/selectors; no test command executes. No destructive or continuously growing fixture.",
    "runs": [],
    "summaries": [],
}
for case, count, selectors in [
    ("small-symbol", 1, 1),
    ("unrestricted", 32, 0),
    ("distinct-symbols", 512, 512),
]:
    with tempfile.TemporaryDirectory(prefix="hoimin-1060-measure-") as tmp:
        root = Path(tmp)
        source = "".join(
            f"def subject_{i:04}():\n    return True\n" for i in range(count)
        )
        (root / "case.py").write_text(source)
        common = [
            "plan",
            "--root",
            str(root),
            "--source",
            ".",
            "--operators",
            "boolean_literal",
            "--max-candidates",
            "2048",
            "--jobs",
            "1",
            "--allow-best-effort-memory",
            "--max-workspace-size",
            "8GiB",
            "--min-free-space",
            "10GiB",
        ]
        for i in range(selectors):
            common += ["--symbol", f"case:subject_{i:04}"]
        common += ["--", sys.executable, "-c", "pass"]
        candidate_digest = None
        for sample in range(-1, 5):
            order = list(binaries) if sample % 2 == 0 else list(reversed(binaries))
            for label in order:
                stats = root / "time.txt"
                started = time.perf_counter_ns()
                proc = subprocess.run(
                    [
                        "/usr/bin/time",
                        "-l",
                        "-o",
                        str(stats),
                        str(binaries[label]),
                        *common,
                    ],
                    capture_output=True,
                    check=False,
                    timeout=20,
                )
                elapsed_ms = (time.perf_counter_ns() - started) / 1e6
                assert proc.returncode == 0, (
                    label,
                    case,
                    proc.returncode,
                    proc.stderr.decode(),
                )
                document = json.loads(proc.stdout)
                assert (
                    not document["truncated"] and len(document["candidates"]) == count
                )
                digest = hashlib.sha256(
                    json.dumps(document["candidates"], sort_keys=True).encode()
                ).hexdigest()
                if candidate_digest is None:
                    candidate_digest = digest
                assert digest == candidate_digest, (
                    case,
                    label,
                    "candidate values/order differ",
                )
                rss = int(
                    re.search(r"(\d+)\s+maximum resident set size", stats.read_text())[
                        1
                    ]
                )
                if sample >= 0:
                    result["runs"].append(
                        {
                            "case": case,
                            "label": label,
                            "pair": sample,
                            "order": order.index(label),
                            "functions": count,
                            "distinct_selectors": selectors,
                            "source_sha256": hashlib.sha256(
                                source.encode()
                            ).hexdigest(),
                            "candidate_sha256": digest,
                            "candidates": count,
                            "truncated": False,
                            "exit_code": proc.returncode,
                            "output_document_bytes": len(proc.stdout),
                            "elapsed_ms": elapsed_ms,
                            "maximum_resident_set_size_bytes": rss,
                        }
                    )
        for label in binaries:
            runs = [
                r for r in result["runs"] if r["case"] == case and r["label"] == label
            ]
            result["summaries"].append(
                {
                    "case": case,
                    "label": label,
                    "median_elapsed_ms": statistics.median(
                        r["elapsed_ms"] for r in runs
                    ),
                    "median_maximum_resident_set_size_bytes": statistics.median(
                        r["maximum_resident_set_size_bytes"] for r in runs
                    ),
                }
            )
with args.output.open("x") as out:
    out.write(json.dumps(result, indent=2) + "\n")
for row in result["summaries"]:
    print(json.dumps(row))
