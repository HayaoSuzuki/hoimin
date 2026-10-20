"""Run baseline/changed probes serially, alternating order for five paired runs.

Raw stdout/stderr and commands go to --logs (.superpowers/issue-1031/...).
Only JSON evidence is written to --results. No whole-process RSS claim is made.
"""
import argparse
import hashlib
import json
import pathlib
import platform
import statistics
import subprocess
import time


def summarize(rows):
    result = {}
    for case in sorted({row["case"] for row in rows}):
        result[case] = {}
        for revision in ["baseline", "changed"]:
            group = [row for row in rows if row["case"] == case and row["revision"] == revision]
            phases = {}
            for phase in ["setup", "scan", "saturation", "stopped", "drained"]:
                available = [row[phase] for row in group if row[phase] is not None]
                if available:
                    phases[phase] = {
                        key: {
                            "median": statistics.median(row[key] for row in available),
                            "min": min(row[key] for row in available),
                            "max": max(row[key] for row in available),
                        }
                        for key in available[0]
                    }
            result[case][revision] = phases
    return result


def compare(summary):
    result = []
    for case, values in summary.items():
        old, new = values["baseline"], values["changed"]
        row = {"case": case}
        for phase in ["scan", "saturation", "stopped", "drained"]:
            if phase in old:
                row[phase] = {
                    key: {
                        "baseline": old[phase][key]["median"],
                        "changed": new[phase][key]["median"],
                    }
                    for key in ["retained_bytes", "peak_bytes", "elapsed_ns", "records_read"]
                }
        result.append(row)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=pathlib.Path, required=True)
    parser.add_argument("--changed", type=pathlib.Path, required=True)
    parser.add_argument("--matrix", type=pathlib.Path, default=pathlib.Path(__file__).with_name("matrix.json"))
    parser.add_argument("--logs", type=pathlib.Path, required=True)
    parser.add_argument("--results", type=pathlib.Path, required=True)
    parser.add_argument("--repeats", type=int)
    parser.add_argument("--case", action="append")
    args = parser.parse_args()
    matrix = json.loads(args.matrix.read_text())
    repeats = args.repeats or matrix["repeats"]
    assert repeats > 0
    args.logs.mkdir(parents=True, exist_ok=True)
    args.results.mkdir(parents=True, exist_ok=True)
    binaries = {"baseline": args.baseline.resolve(), "changed": args.changed.resolve()}
    rows = []
    started = time.time()
    for case in matrix["cases"]:
        if args.case and case["name"] not in args.case:
            continue
        for repeat in range(repeats):
            pair = {}
            order = ["baseline", "changed"] if repeat % 2 == 0 else ["changed", "baseline"]
            for revision in order:
                command = [str(binaries[revision]), *map(str, [
                    case["n"], case["s"], case["jobs"], case["field_bytes"],
                    case["mode"], case["selection"],
                ])]
                stem = args.logs / f"{case['name']}-{repeat}-{revision}"
                stem.with_suffix(".command.json").write_text(json.dumps(command, indent=2) + "\n")
                with stem.with_suffix(".stdout").open("w") as stdout, stem.with_suffix(".stderr").open("w") as stderr:
                    subprocess.run(command, stdout=stdout, stderr=stderr, check=True, timeout=300)
                row = json.loads(stem.with_suffix(".stdout").read_text())
                assert row["revision"] == revision
                row.update(case=case["name"], repeat=repeat)
                pair[revision] = row
                rows.append(row)
                # Partial results survive interruptions; finished pairs are also checked below.
                (args.results / "measurements.json").write_text(json.dumps(rows, indent=2) + "\n")
            for key in ["started_digest", "rows_digest", "started", "finished", "killed", "not_run",
                        "spool_bytes", "selected_record_bytes", "selected_body_bytes"]:
                assert pair["baseline"][key] == pair["changed"][key], (case["name"], repeat, key)
            # Exact reader counters, not wall time, detect quadratic rescanning.
            assert pair["baseline"]["drained"]["records_read"] == case["n"]
            assert pair["changed"]["drained"]["records_read"] == case["n"] + case["s"]
            print(f"{case['name']} pair {repeat + 1}/{repeats} passed", flush=True)
    assert rows, "no matching cases"
    summary = summarize(rows)
    (args.results / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    (args.results / "comparison.json").write_text(json.dumps(compare(summary), indent=2) + "\n")
    for revision, binary in binaries.items():
        receipt = binary.with_suffix(".build.json")
        (args.results / f"{revision}-build.json").write_bytes(receipt.read_bytes())
    metadata = {
        "started_unix_seconds": started,
        "elapsed_seconds": time.time() - started,
        "platform": platform.platform(),
        "python": platform.python_version(),
        "repeats": repeats,
        "matrix_sha256": hashlib.sha256(args.matrix.read_bytes()).hexdigest(),
        "binary_sha256": {key: hashlib.sha256(path.read_bytes()).hexdigest() for key, path in binaries.items()},
        "boundary": "Actual core transitions and instrumented production spool source; setup IDs/index included; fixture writing, plan load, reports, children, allocator overhead, OS page cache excluded.",
        "timing": "Cumulative elapsed nanoseconds from after fixture creation; warm spool page cache; instrumented reads; no process execution.",
    }
    (args.results / "run.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    main()
