"""Reproduce fixed comparisons through hoimin; owns budgets and evidence identity.

No test synthesis API is exposed. Generated cases are frozen before any mutation
run, and rejected/incomplete runs cannot be counted as research successes.
"""

import ast
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import random
import shutil
import subprocess
import sys
import time

from hypothesis import Phase, given, seed, settings
from hypothesis import strategies as st

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
PROJECT = HERE / "project"
EVIDENCE = HERE / "evidence"
BINARY = ROOT / "target/debug" / ("hoimin.exe" if os.name == "nt" else "hoimin")
START = time.monotonic()


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(argv, timeout=100):
    remaining = 900 - (time.monotonic() - START)
    if remaining <= 0:
        raise TimeoutError("15-minute experiment budget exhausted")
    begin = time.monotonic()
    result = subprocess.run([str(x) for x in argv], cwd=PROJECT,
        env={**os.environ, "PYTEST_DISABLE_PLUGIN_AUTOLOAD": "1", "PYTHONHASHSEED": "0"},
        capture_output=True, text=True, encoding="utf-8", errors="replace",
        timeout=min(timeout, remaining), check=False)
    return {"argv": list(map(str, argv)), "returncode": result.returncode,
            "seconds": time.monotonic() - begin, "stdout": result.stdout, "stderr": result.stderr}


def prepare():
    EVIDENCE.mkdir(exist_ok=True)
    (PROJECT / "tools").mkdir(exist_ok=True)
    provenance = {}
    for name in ("ci_metrics", "ci_selection"):
        source = ROOT / f"tools/{name}.py"
        shutil.copyfile(source, PROJECT / f"tools/{name}.py")
        provenance[str(source.relative_to(ROOT))] = digest(source)
    # Preserve the original test bodies and parametrizations, with minimal imports.
    for name, names in (
        ("ci_metrics", {"test_total_preserves_missing_measurements", "test_metrics_separate_queue_execution_skips_and_attempts"}),
        ("ci_selection", {"git", "test_diff_includes_deleted_and_both_renamed_paths",
                          "test_plan_cli_outputs_are_complete_and_cgroup_is_trusted_push_only"}),
    ):
        source = ROOT / f"tests/test_{name}.py"
        content = source.read_text(encoding="utf-8")
        lines = content.splitlines(keepends=True)
        selected = []
        for node in ast.parse(content).body:
            if isinstance(node, ast.FunctionDef) and node.name in names:
                start = min([node.lineno, *[d.lineno for d in node.decorator_list]])
                selected.append("".join(lines[start-1:node.end_lineno]))
        assert len(selected) == len(names)
        header = f"import os, shutil, subprocess, sys\nfrom pathlib import Path\nimport pytest\nfrom tools import {name}\n\n"
        (PROJECT / f"test_upstream_{name}.py").write_text(header + "\n\n".join(selected), encoding="utf-8")
        provenance[str(source.relative_to(ROOT))] = digest(source)
    sys.path.insert(0, str(ROOT / ".superpowers/evaluation-deps"))
    from allpairspy import AllPairs
    corpus = {}
    generation = []
    for label, number, low, high in (("narrow", 1186, 0, 0), ("broad", 1177, 0, 9), ("heldout", 2177, 0, 9)):
        cases = []
        begin = time.monotonic()
        @seed(number)
        @settings(max_examples=40, database=None, deadline=None, phases=[Phase.generate])
        @given(st.lists(st.integers(low, high), max_size=8))
        def collect(values):
            cases.append(values)
        collect()
        generated_count = len(cases)
        cases = (cases * 40)[:40]
        corpus[label] = cases
        generation.append({"name": label, "seed": number, "cases": len(cases), "generated": generated_count,
                           "seconds": time.monotonic()-begin})
    full = list(itertools.product((False, True), repeat=4))
    suites = {"full": full}
    for strength in (2, 3):
        begin = time.monotonic()
        rows = list(AllPairs([[False, True]] * 4, n=strength))
        indices = list(itertools.combinations(range(4), strength))
        required = {(ix, values) for ix in indices for values in itertools.product((False, True), repeat=strength)}
        def covered(selected):
            return {(ix, tuple(row[i] for i in ix)) for row in selected for ix in indices}
        raw = list(rows)
        missing = required - covered(rows)
        initial_missing = sorted(missing)
        # Preflight found n=3 incomplete. Keep raw output and deterministically fill
        # holes from the fixed 16-row universe; never call the raw output 3-way.
        while missing:
            chosen = max(full, key=lambda row: len(covered([row]) & missing))
            rows.append(list(chosen))
            missing -= covered([chosen])
        assert covered(rows) == required
        label = f"way{strength}" + ("completed" if initial_missing else "")
        suites[label] = rows
        generation.append({"name": label, "raw_rows": raw, "missing_projections": initial_missing,
                           "cases": len(rows), "seconds": time.monotonic()-begin})
        for number in (1179, 2179, 3179):
            suites[f"random{strength}s{number}"] = random.Random(number).sample(full, len(rows))
    corpus["combinations"] = suites
    save(PROJECT / "corpus.json", corpus)
    save(EVIDENCE / "generation.json", generation)
    save(EVIDENCE / "provenance.json", {
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "sources": provenance, "binary_sha256": digest(BINARY), "python": sys.version,
        "platform": platform.platform(), "hypothesis": __import__("hypothesis").__version__,
        "allpairspy": "2.5.1", "generator_sha256": digest(Path(__import__("allpairspy.allpairs", fromlist=["__file__"]).__file__)),
    })


def target(filename, symbol, variable=None):
    tree = ast.parse((PROJECT / filename).read_text(encoding="utf-8"))
    node = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == symbol)
    if variable:
        node = next(n for n in ast.walk(node) if isinstance(n, ast.Assign)
                    and any(isinstance(t, ast.Name) and t.id == variable for t in n.targets))
    return f"{filename}:{node.lineno}-{node.end_lineno}"


def main():
    prepare()
    normal = command([sys.executable, "-m", "pytest", "-q", "--tb=short"], 60)
    save(EVIDENCE / "original-tests.json", normal)
    assert normal["returncode"] == 0, normal
    base = "test_total_preserves_missing_measurements"
    arms = [("upstream", base), ("upstream_integration", "test_upstream_ci_metrics"),
            ("relations", "test_mr"), ("base_relations", f"{base} or test_mr"),
            ("combined", f"{base} or test_mr or (test_amplification and broad-strong)")]
    arms += [(f"{domain}-{oracle}", f"test_amplification and {domain}-{oracle}") for domain in ("narrow", "broad", "heldout") for oracle in ("weak", "strong")]
    suites = json.loads((PROJECT / "corpus.json").read_text(encoding="utf-8"))["combinations"]
    groups = [
        ("metrics", "tools/ci_metrics.py", target("tools/ci_metrics.py", "total"),
         "collection_any_all,identity,comprehension_filter_invert,aggregate_sum_count,function_body_return_constant", arms),
        ("comb", "tools/ci_selection.py", target("tools/ci_selection.py", "main", "cgroup"),
         "boolean_and_or,compare_eq_ne,condition_clause_delete",
         [("upstream", "test_plan_cli_outputs_are_complete")]+[(suite, f"test_combinations and {suite}") for suite in suites]),
        ("contract", "tools/ci_selection.py", target("tools/ci_selection.py", "changed_paths", "result"),
         "container_element_delete", [("upstream", "test_diff_includes_deleted"),
         *[(backend, f"test_dependency_contract and {backend}") for backend in ("fixed", "strict", "real")]]),
    ]
    execution = {"arms": []}
    for group, filename, lines, operators, variants in groups:
        identities = None
        for arm, expression in variants:
            name = f"{group}-{arm}"
            argv = [sys.executable, "-m", "pytest", "-q", "--tb=short", "-x", "-k", expression]
            plan_process = command([BINARY, "plan", "--root", PROJECT, "--line", lines,
                "--operators", operators, "--max-mutants", "24", "--max-candidates", "100", "--jobs", "1",
                "--baseline-timeout", "15s", "--mutant-timeout", "8s", "--total-timeout", "90s",
                "--max-memory", "512MiB", "--max-output", "256KiB", "--max-workspace-size", "64MiB",
                "--min-free-space", "10GiB", "--", *argv])
            assert plan_process["returncode"] == 0, plan_process
            plan = json.loads(plan_process["stdout"])
            candidates = plan["candidates"][:24]
            assert candidates and all(c["path"] == filename for c in candidates)
            if identities is None:
                identities = candidates
            assert candidates == identities
            plan_path = EVIDENCE / f"{name}-plan.json"
            save(plan_path, plan)
            result = command([BINARY, "verify", plan_path, "--top", str(len(candidates))])
            save(EVIDENCE / f"{name}-process.json", result)
            (EVIDENCE / f"{name}.jsonl").write_text(result["stdout"], encoding="utf-8")
            events = [json.loads(line) for line in result["stdout"].splitlines()]
            outcomes = [e for e in events if e["kind"] == "mutant_finished"]
            assert result["returncode"] == 0 and events[-1]["complete"], result
            assert len(outcomes) == len(candidates)
            assert next(e for e in events if e["kind"] == "baseline_finished")["termination"] == {"Exit": 0}
            assert all(e["status"] in {"killed", "survived"} and e["outcome"]["executed"]["termination"] == {"Exit": int(e["status"] == "killed")} for e in outcomes)
            row = {"name": name, "candidates": len(candidates), "killed": sum(e["status"] == "killed" for e in outcomes),
                   "plan_seconds": plan_process["seconds"], "verify_seconds": result["seconds"], "test_argv": argv}
            execution["arms"].append(row)
            execution["seconds"] = time.monotonic() - START
            save(EVIDENCE / "execution.json", execution)
            print(json.dumps(row), flush=True)


if __name__ == "__main__":
    main()
