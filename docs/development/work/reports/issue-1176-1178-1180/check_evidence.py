"""Audit saved observations and replay fixed single edits for failure diagnostics.

The CLI owns the primary results. These isolated copies confirm why pytest failed;
they do not replace CLI execution or expand the reviewed candidate population.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile
import time


HERE = Path(__file__).resolve().parent
EVIDENCE = HERE / "evidence"
PROJECT = HERE / "project"


def events(name):
    return [json.loads(line) for line in (EVIDENCE / f"{name}.jsonl").read_text(encoding="utf-8").splitlines()]


def outcomes(name):
    records = events(name)
    baseline = [e for e in records if e["kind"] == "baseline_finished"]
    finish = records[-1]
    assert len(baseline) == 1 and baseline[0]["termination"] == {"Exit": 0}
    assert finish["kind"] == "run_finished" and finish["complete"] and finish["exit_code"] == 0
    rows = {e["candidate"]["id"]: e for e in records if e["kind"] == "mutant_finished"}
    assert len(rows) == records[0]["verification_selection"]["selected"]
    assert all(row["status"] in {"killed", "survived"} for row in rows.values())
    # This experiment invokes pytest: exit 2 is collection failure, not a witness.
    assert all(row["outcome"]["executed"]["termination"] ==
               {"Exit": int(row["status"] == "killed")} for row in rows.values())
    assert all(row["outcome"]["executed"]["output_state"] == "complete" for row in rows.values())
    return rows


def main():
    started = time.monotonic()
    execution = json.loads((EVIDENCE / "execution.json").read_text(encoding="utf-8"))
    matrix = {}
    arms = {}
    for entry in execution["arms"]:
        name = entry["name"]
        rows = outcomes(name)
        arms[name] = {**entry, "killed": sum(row["status"] == "killed" for row in rows.values()),
                      "survived": sum(row["status"] == "survived" for row in rows.values())}
        for identity, row in rows.items():
            matrix.setdefault(identity, {"candidate": row["candidate"], "arms": {}})["arms"][name] = row["status"]
    witnesses = []
    for arm in ("pbt-broad_strong", "stateful-history", "resilience-faults", "retry-faults"):
        rows = outcomes(arm)
        argv = json.loads((EVIDENCE / f"{arm}-normal.json").read_text(encoding="utf-8"))["argv"]
        for identity, row in rows.items():
            if row["status"] != "killed":
                continue
            candidate = row["candidate"]
            assert candidate["path"] in {"subjects.py", "retry_subject.py"}
            source = (PROJECT / candidate["path"]).read_bytes()
            offset = candidate["span"]["start"]
            end = offset + candidate["span"]["length"]
            assert source[offset:end] == candidate["original"].encode()
            replacement = source[:offset] + candidate["replacement"].encode() + source[end:]
            with tempfile.TemporaryDirectory(prefix="hoimin-research-witness-") as directory:
                destination = Path(directory)
                for filename in ("subjects.py", "retry_subject.py", "test_subject.py", "corpus.json", "pytest.ini"):
                    shutil.copyfile(PROJECT / filename, destination / filename)
                (destination / candidate["path"]).write_bytes(replacement)
                begin = time.monotonic()
                result = subprocess.run(argv, cwd=destination, capture_output=True, text=True,
                                        encoding="utf-8", errors="replace", timeout=8, check=False,
                                        env={**os.environ, "PYTEST_DISABLE_PLUGIN_AUTOLOAD": "1", "PYTHONHASHSEED": "0"})
            witnesses.append({"arm": arm, "candidate_id": identity, "original_sha256": hashlib.sha256(source).hexdigest(),
                              "mutated_sha256": hashlib.sha256(replacement).hexdigest(),
                              "seconds": time.monotonic() - begin, "returncode": result.returncode,
                              "stdout": result.stdout, "stderr": result.stderr})
            assert result.returncode == 1, (arm, identity, result.stdout, result.stderr)
    sys.path.insert(0, str(PROJECT))
    import subjects
    import test_subject
    reset_ns = []
    for _ in range(1000):
        begin = time.perf_counter_ns()
        cache = subjects.Cache()
        backend = test_subject.Backend(2)
        reset_ns.append(time.perf_counter_ns() - begin)
    summary = {"arms": arms, "matrix": matrix, "witnesses": witnesses,
               "audit_seconds": time.monotonic() - started,
               "reset": {"samples": len(reset_ns), "median_ns": statistics.median(reset_ns),
                         "max_ns": max(reset_ns), "total_ns": sum(reset_ns),
                         "scope": "construction of one Cache and one Backend; no external I/O"},
               "source_hashes": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                                 for p in PROJECT.iterdir() if p.is_file()}}
    (EVIDENCE / "analysis.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"arms": arms, "witness_count": len(witnesses), "reset": summary["reset"]}, indent=2))


if __name__ == "__main__":
    main()
