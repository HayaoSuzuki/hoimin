"""Fixed-corpus inheritance research; deliberately not a product analyzer."""

import argparse
import ast
import hashlib
import json
import platform
import random
import runpy
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[4]
HELPER_PATH = REPOSITORY / "docs/superpowers/reports/issues-1125-1131/experiment.py"
HELPERS = runpy.run_path(str(HELPER_PATH), run_name="inheritance_research_helpers")
Candidate = HELPERS["Candidate"]
BOUNDS = HELPERS["bounds"]
CHECK = HELPERS["check"]
PLAN = HELPERS["plan"]
APPLY = HELPERS["apply"]
INCUMBENT = HELPERS["incumbent"]
NEAREST = "statement_delete,function_body_erase,function_body_return_constant,method_call_remove"
PER_POOL_LIMIT = 64


@dataclass(frozen=True)
class Example:
    name: str
    proposed: str
    requirement: str
    source: str
    weak: str
    strong: str


EXAMPLES = (
    Example(
        "override_dispatch",
        "override_method_delete",
        "The specialized dispatcher uses the child provider, including when providers differ.",
        "class Base:\n"
        "    def run(self, providers):\n        return providers['base']()\n"
        "class Child(Base):\n"
        "    def run(self, providers):\n        return providers['child']()\n",
        "assert Child().run({'base': lambda: 2, 'child': lambda: 2}) == 2",
        "assert Child().run({'base': lambda: 2, 'child': lambda: 5}) == 5",
    ),
    Example(
        "override_equivalent",
        "override_method_delete",
        "Return the supplied object; both implementations satisfy this contract.",
        "class Base:\n    def run(self, value):\n        return value\n"
        "class Child(Base):\n    def run(self, value):\n        return value\n",
        "assert Child().run(2) == 2",
        "value = object()\nassert Child().run(value) is value",
    ),
    Example(
        "constructor_interface_risk",
        "override_method_delete",
        "Construction accepts a token; inherited construction has an incompatible signature.",
        "class Base:\n    def __init__(self):\n        self.token = None\n"
        "class Child(Base):\n    def __init__(self, token):\n        self.token = token\n",
        "Child('token')",
        "assert Child('token').token == 'token'",
    ),
    Example(
        "super_order",
        "super_call_move",
        "All three events occur exactly once in before, parent, after order.",
        "class Base:\n    def run(self, events):\n        events.append('parent')\n"
        "class Child(Base):\n    def run(self, events):\n"
        "        events.append('before')\n        super().run(events)\n"
        "        events.append('after')\n",
        "events=[]\nChild().run(events)\nassert sorted(events)==['after','before','parent']",
        "events=[]\nChild().run(events)\nassert events==['before','parent','after']",
    ),
    Example(
        "super_order_callback",
        "super_call_move",
        "The supplied event sink receives before, parent, after exactly once in that order.",
        "class Base:\n    def run(self, emit):\n        emit('parent')\n"
        "class Child(Base):\n    def run(self, emit):\n"
        "        emit('before')\n        super().run(emit)\n"
        "        emit('after')\n",
        "events=[]\nChild().run(events.append)\nassert sorted(events)==['after','before','parent']",
        "events=[]\nChild().run(events.append)\nassert events==['before','parent','after']",
    ),
    Example(
        "super_commuting_control",
        "super_call_move",
        "Both labels belong to the result set; relative insertion order is not promised.",
        "class Base:\n    def run(self, labels):\n        labels.add('parent')\n"
        "class Child(Base):\n    def run(self, labels):\n"
        "        labels.add('child')\n        super().run(labels)\n",
        "labels=set()\nChild().run(labels)\nassert 'parent' in labels",
        "labels=set()\nChild().run(labels)\nassert labels=={'child','parent'}",
    ),
    Example(
        "super_data_dependency_risk",
        "super_call_move",
        "Parent initialization precedes consumption of its state; moving it can raise AttributeError.",
        "class Base:\n    def run(self, events):\n        self.ready = True\n"
        "class Child(Base):\n    def run(self, events):\n"
        "        super().run(events)\n        events.append(self.ready)\n",
        "Child().run([])",
        "events=[]\nChild().run(events)\nassert events==[True]",
    ),
    Example(
        "super_delete_overlap",
        "super_call_delete",
        "The parent delegation appends its event, even though the return value is None.",
        "class Base:\n    def run(self, events):\n        events.append('parent')\n"
        "class Child(Base):\n    def run(self, events):\n        super().run(events)\n",
        "assert Child().run([]) is None",
        "events=[]\nChild().run(events)\nassert events==['parent']",
    ),
    Example(
        "super_delete_no_observation",
        "super_call_delete",
        "The API promises only None; parent delegation has no observable effect here.",
        "class Base:\n    def run(self, events):\n        pass\n"
        "class Child(Base):\n    def run(self, events):\n        super().run(events)\n",
        "assert Child().run([]) is None",
        "events=[]\nassert Child().run(events) is None\nassert events==[]",
    ),
)


BASE_ORDER_EXAMPLES = (
    Example(
        "base_order_dispatch",
        "base_order_shuffle",
        "The leftmost provider wins, including when provider values differ.",
        "class A:\n    def run(self, providers):\n        return providers['a']()\n"
        "class B:\n    def run(self, providers):\n        return providers['b']()\n"
        "class Child(A, B):\n    pass\n",
        "assert Child().run({'a': lambda: 2, 'b': lambda: 2}) == 2",
        "assert Child().run({'a': lambda: 2, 'b': lambda: 5}) == 2",
    ),
    Example(
        "base_order_super_callback",
        "base_order_shuffle",
        "The cooperative sink receives a, b exactly once in that order.",
        "class Root:\n    def run(self, emit):\n        pass\n"
        "class A(Root):\n    def run(self, emit):\n"
        "        emit('a')\n        super().run(emit)\n"
        "class B(Root):\n    def run(self, emit):\n"
        "        emit('b')\n        super().run(emit)\n"
        "class Child(A, B):\n    pass\n",
        "events=[]\nChild().run(events.append)\nassert sorted(events)==['a','b']",
        "events=[]\nChild().run(events.append)\nassert events==['a','b']",
    ),
    Example(
        "base_order_three_super_callback",
        "base_order_shuffle",
        "Three cooperative callbacks occur exactly once in a, b, c order.",
        "class Root:\n    def run(self, emit):\n        pass\n"
        "class A(Root):\n    def run(self, emit):\n"
        "        emit('a')\n        super().run(emit)\n"
        "class B(Root):\n    def run(self, emit):\n"
        "        emit('b')\n        super().run(emit)\n"
        "class C(Root):\n    def run(self, emit):\n"
        "        emit('c')\n        super().run(emit)\n"
        "class Child(A, B, C):\n    pass\n",
        "events=[]\nChild().run(events.append)\nassert sorted(events)==['a','b','c']",
        "events=[]\nChild().run(events.append)\nassert events==['a','b','c']",
    ),
    Example(
        "base_order_commuting_control",
        "base_order_shuffle",
        "Both labels are required; their insertion order is not contractual.",
        "class Root:\n    def run(self, labels):\n        pass\n"
        "class A(Root):\n    def run(self, labels):\n"
        "        labels.add('a')\n        super().run(labels)\n"
        "class B(Root):\n    def run(self, labels):\n"
        "        labels.add('b')\n        super().run(labels)\n"
        "class Child(A, B):\n    pass\n",
        "labels=set()\nChild().run(labels)\nassert 'a' in labels",
        "labels=set()\nChild().run(labels)\nassert labels=={'a','b'}",
    ),
    Example(
        "base_order_explicit_call_control",
        "base_order_shuffle",
        "Explicit delegation promises a, b order independently of the base list.",
        "class A:\n    def run(self, emit):\n        emit('a')\n"
        "class B:\n    def run(self, emit):\n        emit('b')\n"
        "class Child(A, B):\n    def run(self, emit):\n"
        "        A.run(self, emit)\n        B.run(self, emit)\n",
        "events=[]\nChild().run(events.append)\nassert sorted(events)==['a','b']",
        "events=[]\nChild().run(events.append)\nassert events==['a','b']",
    ),
    Example(
        "base_order_mro_risk",
        "base_order_shuffle",
        "Child(B, A) is constructible; reversing dependent bases invalidates C3 MRO.",
        "class A:\n    pass\nclass B(A):\n    pass\nclass Child(B, A):\n    pass\n",
        "Child()",
        "assert isinstance(Child(), A)",
    ),
)


def proposed(source: str, operator: str, seed: int = 0) -> list:
    """Only inspect the fixed, known Base/Child examples, never import the target.

    This is a semantic experiment, not an implementation of eligibility guards.
    In particular constructor and data-dependency cases are intentional risks.
    """
    tree = ast.parse(source)
    child = next(
        n for n in tree.body if isinstance(n, ast.ClassDef) and n.name == "Child"
    )
    if operator == "base_order_shuffle":
        # This fixed-corpus probe handles distinct simple bases only. Keep the
        # edit inside the base list, so class bodies and keywords are untouched.
        assert len(child.bases) >= 2 and all(
            isinstance(b, ast.Name) for b in child.bases
        )
        original = [base.id for base in child.bases]
        assert len(set(original)) == len(original)
        reordered = original.copy()
        if len(reordered) > 2:
            random.Random(seed).shuffle(reordered)
        if reordered == original:
            reordered[0], reordered[1] = reordered[1], reordered[0]
        start = BOUNDS(source, child.bases[0])[0]
        end = BOUNDS(source, child.bases[-1])[1]
        candidate = Candidate(
            "prototype:" + operator,
            start,
            end - start,
            source[start:end],
            ", ".join(reordered),
            child.lineno,
            operator,
        )
        compile(APPLY(source, candidate), "<inheritance-probe>", "exec")
        return [candidate]
    method = next(n for n in child.body if isinstance(n, ast.FunctionDef))
    if operator == "override_method_delete":
        node = method
        replacement = "pass"
    else:
        index, call = next(
            (i, n)
            for i, n in enumerate(method.body)
            if isinstance(n, ast.Expr)
            and isinstance(n.value, ast.Call)
            and isinstance(n.value.func, ast.Attribute)
            and isinstance(n.value.func.value, ast.Call)
            and isinstance(n.value.func.value.func, ast.Name)
            and n.value.func.value.func.id == "super"
        )
        if operator == "super_call_delete":
            node = call
            replacement = "pass"
        else:
            node = method
            statements = list(method.body)
            statements.pop(index)
            statements.insert(len(statements) if index == 0 else 0, call)
            # These ASCII fixtures deliberately have one physical line per statement.
            start, body_start = (
                BOUNDS(source, method)[0],
                BOUNDS(source, method.body[0])[0],
            )
            replacement = source[start:body_start] + "\n        ".join(
                ast.get_source_segment(source, n) for n in statements
            )
    start, end = BOUNDS(source, node)
    candidate = Candidate(
        "prototype:" + operator,
        start,
        end - start,
        source[start:end],
        replacement,
        node.lineno,
        operator,
    )
    # Parsing and byte agreement are prerequisites, not evidence of usefulness.
    compile(APPLY(source, candidate), "<inheritance-probe>", "exec")
    return [candidate]


def compare(binary: Path, root: Path, example: Example, seed: int = 0) -> dict:
    originals = {
        kind: CHECK(root, example.source, checks)
        for kind, checks in (("weak", example.weak), ("strong", example.strong))
    }
    assert all(row["verdict"] == "passed" for row in originals.values())
    rows = {}
    for family, operators in (("nearest", NEAREST), ("defaults", None)):
        started = time.perf_counter()
        saved = PLAN(binary, root, example.source, operators)
        pool = INCUMBENT(saved)
        rows[family] = {
            "generation_seconds": time.perf_counter() - started,
            "candidates": pool,
        }
    started = time.perf_counter()
    pool = proposed(example.source, example.proposed, seed)
    rows["proposed"] = {
        "generation_seconds": time.perf_counter() - started,
        "candidates": pool,
    }
    mutant_sources = {}
    for family, row in rows.items():
        pool = row.pop("candidates")
        row["pool_count"] = len(pool)
        row["executed"] = min(len(pool), PER_POOL_LIMIT)
        row["unexecuted"] = len(pool) - row["executed"]
        row["results"] = []
        mutant_sources[family] = {APPLY(example.source, c) for c in pool}
        for candidate in pool[:PER_POOL_LIMIT]:
            mutant = APPLY(example.source, candidate)
            row["results"].append(
                {
                    "candidate": asdict(candidate),
                    "weak": CHECK(root, mutant, example.weak),
                    "strong": CHECK(root, mutant, example.strong),
                }
            )
    prototype = next(iter(mutant_sources["proposed"]))
    base_order = {}
    if example.proposed == "base_order_shuffle":
        orders = []
        for source in (example.source, prototype):
            child = next(
                n
                for n in ast.parse(source).body
                if isinstance(n, ast.ClassDef) and n.name == "Child"
            )
            orders.append([ast.unparse(base) for base in child.bases])
        base_order = {
            "base_order": {"seed": seed, "before": orders[0], "after": orders[1]}
        }
    return {
        **asdict(example),
        **base_order,
        "originals": originals,
        "exact_source_overlap": {
            name: prototype in mutant_sources[name] for name in ("nearest", "defaults")
        },
        **rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hoimin", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--seed", type=int, default=0, help="Seed for base-order shuffling"
    )
    parser.add_argument(
        "--base-order-only",
        action="store_true",
        help="Run only the six base-order examples",
    )
    args = parser.parse_args()
    binary = args.hoimin.resolve()
    started = time.perf_counter()
    with tempfile.TemporaryDirectory(prefix="hoimin-1123-") as directory:
        root = Path(directory)
        assert all(shutil.disk_usage(p).free > 10 * 1024**3 for p in (root, REPOSITORY))
        (root / "check.py").write_text("import subject\n")
        result = {
            "python": sys.version,
            "platform": platform.platform(),
            "source_revision": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=REPOSITORY, text=True
            ).strip(),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "helper_sha256": hashlib.sha256(HELPER_PATH.read_bytes()).hexdigest(),
            "nearest_operators": NEAREST,
            "per_pool_limit": PER_POOL_LIMIT,
            "seed": args.seed,
            "comparisons": [
                compare(binary, root, e, args.seed)
                for e in (
                    BASE_ORDER_EXAMPLES
                    if args.base_order_only
                    else EXAMPLES + BASE_ORDER_EXAMPLES
                )
            ],
        }
        result["temporary_file_bytes_at_end"] = sum(
            p.stat().st_size for p in root.rglob("*") if p.is_file()
        )
    result["temporary_directory_removed"] = not root.exists()
    result["elapsed_seconds"] = time.perf_counter() - started
    args.output.write_text(json.dumps(result, indent=2) + "\n", newline="")


if __name__ == "__main__":
    main()
