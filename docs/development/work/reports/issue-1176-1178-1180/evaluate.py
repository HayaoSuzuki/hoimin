"""Reproduce this fixed research comparison using the existing hoimin CLI.

Owns corpus generation, candidate identity checks, budgets and evidence storage.
It does not apply edits, infer specifications or implement a mutation executor.
Run with the repository's development Python; defaults to target/debug/hoimin.
"""

from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

from hypothesis import Phase, given, seed, settings
from hypothesis import strategies as st


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
PROJECT = HERE / "project"
EVIDENCE = HERE / "evidence"
BINARY = ROOT / "target" / "debug" / ("hoimin.exe" if os.name == "nt" else "hoimin")
START = time.monotonic()
GENERATION = []


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(argv, timeout=100):
    remaining = 900 - (time.monotonic() - START)
    if remaining <= 0:
        raise TimeoutError("15-minute research execution budget exhausted")
    started = time.monotonic()
    result = subprocess.run(
        [str(arg) for arg in argv], cwd=PROJECT,
        env={**os.environ, "PYTEST_DISABLE_PLUGIN_AUTOLOAD": "1", "PYTHONHASHSEED": "0"},
        capture_output=True, text=True, encoding="utf-8", errors="replace",
        timeout=min(timeout, remaining), check=False,
    )
    return {"argv": [str(arg) for arg in argv], "returncode": result.returncode,
            "seconds": time.monotonic() - started,
            "stdout": result.stdout, "stderr": result.stderr}


def generated(strategy, value, label):
    values = []
    started = time.monotonic()

    @seed(value)
    @settings(max_examples=40, database=None, deadline=None, phases=[Phase.generate])
    @given(strategy)
    def collect(case):
        values.append(case)

    collect()
    GENERATION.append({"domain": label, "seed": value, "requested": 40,
                       "generated": len(values), "seconds": time.monotonic() - started,
                       "failures": 0, "cases": values})
    return values


def prepare():
    """Freeze inputs and extract the real function without loading network code."""
    EVIDENCE.mkdir(exist_ok=True)
    source_path = ROOT / "tools/pypi_provenance.py"
    source = source_path.read_text(encoding="utf-8")
    tree = ast.parse(source)
    function = next(node for node in tree.body if isinstance(node, ast.FunctionDef)
                    and node.name == "verify_with_propagation")
    constants = [node for node in tree.body if isinstance(node, ast.Assign)
                 and isinstance(node.targets[0], ast.Name)
                 and node.targets[0].id.startswith("PROPAGATION_")]
    excerpt = ast.get_source_segment(source, function)
    header = ('"""Verbatim retry function projection; see ../evidence/provenance.json."""\n'
              'import http\nimport logging\nfrom pathlib import Path\nimport time\nimport urllib.error\n'
              'LOGGER = logging.getLogger(__name__)\n'
              'def verify_index(*args, **kwargs):\n'
              '    raise AssertionError("external verification must be replaced by the test")\n')
    text = header + "\n".join(ast.get_source_segment(source, n) for n in constants) + "\n\n" + excerpt + "\n"
    (PROJECT / "retry_subject.py").write_text(text, encoding="utf-8")
    save(EVIDENCE / "provenance.json", {
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "source": "tools/pypi_provenance.py", "source_sha256": digest(source_path),
        "function_lines": [function.lineno, function.end_lineno],
        "function_sha256": hashlib.sha256(excerpt.encode()).hexdigest(),
        "projection_sha256": digest(PROJECT / "retry_subject.py"),
        "binary_sha256": digest(BINARY), "python": sys.version,
        "platform": platform.platform(), "hypothesis": __import__("hypothesis").__version__,
    })
    narrow = [[1], [2, 2], [1, 2, 3]]
    broad = [[], [0], [-1], [-3, 0, 3, 3]]
    for value in (1176, 1177):
        narrow.extend(generated(st.lists(st.integers(1, 3), min_size=1, max_size=8), value, "narrow"))
        broad.extend(generated(st.lists(st.integers(-3, 3), max_size=8), value, "broad"))
    heldout = generated(st.lists(st.integers(-3, 3), max_size=8), 2176, "heldout")
    single = [[["put", key, value], ["get", key, 0]] for key in (0, 1) for value in (-1, 1)]
    history = single + [
        [["put", 0, 1], ["get", 0, 0], ["put", 0, 2], ["get", 0, 0]],
        [["put", 0, 1], ["get", 0, 0], ["delete", 0, 0], ["get", 0, 0]],
        [["put", 0, 1], ["delete", 0, 0], ["get", 0, 0]],
    ]
    histories = generated(st.lists(st.tuples(st.sampled_from(["put", "get", "delete"]),
                                            st.integers(0, 2), st.integers(-2, 2)),
                                  min_size=1, max_size=12), 1178, "history")
    save(PROJECT / "corpus.json", {"narrow": narrow, "broad": narrow + broad,
                                    "heldout": heldout, "single": single,
                                    "history": history + histories})
    save(EVIDENCE / "generation.json", GENERATION)


def selection(filename, symbol):
    source = (PROJECT / filename).read_text(encoding="utf-8")
    node = next(node for node in ast.parse(source).body
                if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name == symbol)
    return f"{filename}:{node.lineno}-{node.end_lineno}"


def main():
    prepare()
    preparation_seconds = time.monotonic() - START
    all_tests = command([sys.executable, "-m", "pytest", "-q", "--tb=short"])
    save(EVIDENCE / "original-tests.json", all_tests)
    if all_tests["returncode"] != 0:
        raise RuntimeError("Original specification tests failed; see original-tests.json")
    groups = [
        ("pbt", "subjects.py", "nonnegative", "compare_order,comprehension_filter_remove,comprehension_filter_invert",
         [(arm, f"test_pbt_{arm}") for arm in ("narrow_weak", "broad_weak", "narrow_strong", "broad_strong", "heldout_weak", "heldout_strong")]),
        ("stateful", "subjects.py", "Cache", "statement_delete",
         [("single", "test_stateful_single"), ("history", "test_stateful_history")]),
        ("resilience", "subjects.py", "publish", "statement_delete,finally_unwrap,integer_literal_neighbor",
         [("happy", "test_resilience_happy"), ("faults", "test_resilience")]),
        ("retry", "retry_subject.py", "verify_with_propagation", "statement_delete,integer_literal_neighbor",
         [("happy", "test_retry_happy"), ("faults", "test_retry")]),
    ]
    results = {"preparation_seconds": preparation_seconds, "arms": []}
    for name, filename, symbol, operators, arms in groups:
        selected = None
        for arm, expression in arms:
            label = f"{name}-{arm}"
            test_argv = [sys.executable, "-m", "pytest", "-q", "--tb=short", "-x", "-k", expression]
            normal = command(test_argv, timeout=30)
            save(EVIDENCE / f"{label}-normal.json", normal)
            if normal["returncode"] != 0:
                raise RuntimeError(f"Original arm failed: {label}")
            plan_result = command([
                BINARY, "plan", "--root", PROJECT, "--line", selection(filename, symbol),
                "--operators", operators, "--max-mutants", "24", "--max-candidates", "100",
                "--jobs", "1", "--baseline-timeout", "15s", "--mutant-timeout", "4s",
                "--total-timeout", "90s", "--max-memory", "512MiB", "--max-output", "256KiB",
                "--max-workspace-size", "64MiB", "--min-free-space", "128MiB", "--", *test_argv,
            ])
            save(EVIDENCE / f"{label}-plan-process.json", plan_result)
            if plan_result["returncode"] != 0:
                raise RuntimeError(f"plan failed: {label}")
            plan = json.loads(plan_result["stdout"])
            if any(c["path"] != filename for c in plan["candidates"]):
                raise RuntimeError(f"Unexpected mutation target outside subject: {label}")
            plan_path = EVIDENCE / f"{label}-plan.json"
            save(plan_path, plan)
            identities = [(c["id"], c["operator"], c["original"], c["replacement"]) for c in plan["candidates"][:24]]
            if selected is None:
                selected = identities
            if identities != selected or not identities:
                raise RuntimeError(f"Non-comparable or empty candidate selection: {label}")
            report = command([BINARY, "verify", plan_path, "--top", str(len(identities))])
            save(EVIDENCE / f"{label}-verify-process.json", report)
            (EVIDENCE / f"{label}.jsonl").write_text(report["stdout"], encoding="utf-8")
            results["arms"].append({"name": label, "normal_seconds": normal["seconds"],
                                    "plan_seconds": plan_result["seconds"],
                                    "verify_seconds": report["seconds"],
                                    "returncode": report["returncode"], "candidates": len(identities)})
            results["total_seconds"] = time.monotonic() - START
            save(EVIDENCE / "execution.json", results)
            print(json.dumps(results["arms"][-1]), flush=True)
            if report["returncode"] != 0:
                raise RuntimeError(f"Verification incomplete: {label}; inspect retained diagnostics")


if __name__ == "__main__":
    main()
