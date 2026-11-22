"""Audit complete CLI observations and retain isolated failure witnesses.

Replay explains existing results only; it is not a replacement mutation engine.
The truth-table probes are explicitly seeded models, separate from CLI mutants.
"""

import hashlib
import itertools
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
PROJECT = HERE / "project"
EVIDENCE = HERE / "evidence"


def read(name):
    return json.loads((EVIDENCE / name).read_text(encoding="utf-8"))


def main():
    started = time.monotonic()
    execution = read("execution.json")
    matrix, groups = {}, {}
    for arm in execution["arms"]:
        name = arm["name"]
        plan = read(f"{name}-plan.json")
        group = name.split("-", 1)[0]
        candidates = plan["candidates"][:24]
        assert groups.setdefault(group, candidates) == candidates
        events = [json.loads(line) for line in (EVIDENCE / f"{name}.jsonl").read_text(encoding="utf-8").splitlines()]
        assert events[-1]["kind"] == "run_finished" and events[-1]["complete"] and events[-1]["exit_code"] == 0
        assert next(e for e in events if e["kind"] == "baseline_finished")["termination"] == {"Exit": 0}
        outcomes = [e for e in events if e["kind"] == "mutant_finished"]
        assert len(outcomes) == len(candidates)
        assert {e["candidate"]["id"] for e in outcomes} == {c["id"] for c in candidates}
        assert sum(e["status"] == "killed" for e in outcomes) == arm["killed"]
        for event in outcomes:
            outcome = event["outcome"]["executed"]
            assert event["status"] in {"killed", "survived"}
            assert outcome["termination"] == {"Exit": int(event["status"] == "killed")}
            assert outcome["output_state"] == "complete"
            candidate = event["candidate"]
            row = matrix.setdefault(candidate["id"], {"candidate": candidate, "arms": {}})
            row["arms"][name] = event["status"]
    witnesses = []
    priorities = ["metrics-upstream_integration", "comb-full", "contract-real", "contract-strict"]
    for identity, row in matrix.items():
        arm = next(name for name in priorities if row["arms"].get(name) == "killed")
        argv = next(a["test_argv"] for a in execution["arms"] if a["name"] == arm)
        candidate = row["candidate"]
        assert candidate["path"] in {"tools/ci_metrics.py", "tools/ci_selection.py"}
        source = (PROJECT / candidate["path"]).read_bytes()
        start = candidate["span"]["start"]
        end = start + candidate["span"]["length"]
        assert source[start:end] == candidate["original"].encode()
        changed = source[:start] + candidate["replacement"].encode() + source[end:]
        with tempfile.TemporaryDirectory(prefix="hoimin-research-witness-") as directory:
            destination = Path(directory) / "project"
            shutil.copytree(PROJECT, destination, ignore=shutil.ignore_patterns("__pycache__", ".pytest_cache"))
            (destination / candidate["path"]).write_bytes(changed)
            begin = time.monotonic()
            result = subprocess.run(argv, cwd=destination, capture_output=True, text=True,
                encoding="utf-8", errors="replace", timeout=15, check=False,
                env={**os.environ, "PYTEST_DISABLE_PLUGIN_AUTOLOAD": "1", "PYTHONHASHSEED": "0"})
        assert result.returncode == 1 and "1 failed" in result.stdout, (arm, result.stdout, result.stderr)
        witnesses.append({"arm": arm, "candidate_id": identity, "seconds": time.monotonic()-begin,
                          "original_sha256": hashlib.sha256(source).hexdigest(),
                          "mutated_sha256": hashlib.sha256(changed).hexdigest(),
                          "returncode": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
    corpus = json.loads((PROJECT / "corpus.json").read_text(encoding="utf-8"))
    probes = {}
    for name, rows in corpus["combinations"].items():
        coverage = {}
        for strength in (2, 3, 4):
            projections = {(ix, tuple(row[i] for i in ix)) for row in rows
                           for ix in itertools.combinations(range(4), strength)}
            required = sum(1 for _ in itertools.combinations(range(4), strength)) * 2**strength
            coverage[str(strength)] = {"covered": len(projections), "required": required}
        # Deliberate specification-error probes, not mutants emitted by hoimin.
        drops = [any(all(row[:i]+row[i+1:]) != all(row) for row in rows) for i in range(4)]
        probes[name] = {"cases": len(rows), "coverage": coverage, "detected_seeded_clause_drops": drops}
    # Relations rejected independently on original behavior; these are not scored as kills.
    sys.path.insert(0, str(PROJECT))
    from tools import ci_metrics
    assert Path(ci_metrics.__file__).resolve() == (PROJECT / "tools/ci_metrics.py").resolve()
    floating = [1e308, 1e308, -1e308]
    counterexamples = {
        "unrestricted_float_permutation": [ci_metrics.total(floating), ci_metrics.total(list(reversed(floating)))],
        "duplicate_idempotence": [ci_metrics.total([1]), ci_metrics.total([1, 1])],
        "missing_is_zero": [ci_metrics.total([None]), ci_metrics.total([0])],
    }
    assert all(left != right for left, right in counterexamples.values())
    counterexamples = {name: list(map(repr, values)) for name, values in counterexamples.items()}
    summary = {"arms": execution["arms"], "matrix": matrix, "witnesses": witnesses,
               "seeded_model_probes": probes, "rejected_relation_counterexamples": counterexamples,
               "audit_seconds": time.monotonic()-started,
               "project_hashes": {str(p.relative_to(PROJECT)): hashlib.sha256(p.read_bytes()).hexdigest()
                                  for p in PROJECT.rglob("*") if p.is_file() and "__pycache__" not in p.parts}}
    (EVIDENCE / "analysis.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")
    print(json.dumps({"distinct_candidates": len(matrix), "executions": sum(a["candidates"] for a in execution["arms"]),
                      "witnesses": len(witnesses), "seeded_model_probes": probes, "seconds": execution["seconds"]}, indent=2))


if __name__ == "__main__":
    main()
