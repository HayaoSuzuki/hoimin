"""Run alternating, serial release pairs and compare complete result digests."""

import hashlib
import json
import pathlib
import platform
import statistics
import subprocess
import sys


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    before, after, output = (pathlib.Path(arg).resolve() for arg in sys.argv[1:])
    binaries = {"before": before, "after": after}
    default_payload = 16 * 1024 * 1024
    # keywords, mode, candidate limit, payload bytes, iterations per invocation
    workloads = [
        (1, "all", 10, default_payload, 300),
        (1, "selected", 10, default_payload, 300),
        (1, "excluded", 10, 0, 300),
        (32, "excluded", 10_000, 0, 100),
        (1_000, "excluded", 10_000, 0, 1),
        (2_000, "excluded", 10_000, 0, 1),
        (4_000, "excluded", 10_000, 0, 1),
        (10_000, "excluded", 10_000, 0, 1),
        (1_000, "excluded", 1, 0, 1),
        (10_000, "excluded", 1, 0, 1),
        (64, "selected", 10_000, default_payload, 1),
        (64, "selected", 1, default_payload, 1),
        (64, "selected", 0, default_payload, 1),
        (64, "selected", 10, 0, 1),
        (2, "symbol", 10, default_payload, 1),
        (2, "symbol", 1, default_payload, 1),
        (2, "optional-child", 10, default_payload, 1),
        (1, "boolean-child", 10, default_payload, 1),
        (1_000, "reversed", 10_000, 0, 1),
    ]
    samples = []
    summaries = []
    stable = ["source_blake3", "identity_blake3", "candidates", "diagnostics", "truncated", "error"]
    for workload in workloads:
        paired = []
        for sample in range(5):
            order = ["before", "after"] if sample % 2 == 0 else ["after", "before"]
            for revision in order:
                command = [str(binaries[revision]), *map(str, workload)]
                result = subprocess.run(command, capture_output=True, text=True, check=True, timeout=45)
                row = json.loads(result.stdout)
                row.update(revision=revision, sample=sample, command=command)
                paired.append(row)
                samples.append(row)
        for field in stable:
            if len({json.dumps(row[field], sort_keys=True) for row in paired}) != 1:
                raise AssertionError((workload, field, paired))
        for revision in ["before", "after"]:
            rows = [row for row in paired if row["revision"] == revision]
            metrics = {}
            for field in ["elapsed_ns", "cumulative_requested_bytes", "allocation_calls", "cancellation_checks"]:
                values = [row[field] / row["iterations"] for row in rows]
                metrics[f"{field}_per_analysis"] = {
                    "median": statistics.median(values), "min": min(values), "max": max(values)
                }
            for field in ["retained_delta", "peak_delta"]:
                values = [row[field] for row in rows]
                metrics[field] = {
                    "median": statistics.median(values), "min": min(values), "max": max(values)
                }
            summaries.append({"workload": workload, "revision": revision, "metrics": metrics})
        print("measured", workload, flush=True)
    report = {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "base_revision": "1f7feae33fa310aae87fbadb390105f6d770439d",
        "method": {
            "order": "five serial pairs per workload; alternate before/after order",
            "warmup": "one analysis per process, outside measurement",
            "timing": "allocator-instrumented release Instant; batch includes prior output drops",
            "setup": "source, selectors, request, warmup and result hashing excluded",
            "heap": "successful requested allocation/reallocation bytes; signed retained and relative peak; not RSS",
            "cumulative_realloc": "full successful new request size, including shrinking realloc",
            "small_input": "300 analyses per process; retained snapshot keeps only the final output",
            "correctness": "full candidates, stable IDs, diagnostics, truncation/error and fixture digests match",
            "timeout_seconds_per_process": 45,
            "timing_threshold": None,
        },
        "artifacts": {
            revision: {
                "path": str(binary),
                "sha256": digest(binary),
                "build": json.loads(binary.with_suffix(".build.json").read_text()),
            }
            for revision, binary in binaries.items()
        },
        "driver_sha256": digest(pathlib.Path(__file__)),
        "samples": samples,
        "summaries": summaries,
    }
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
