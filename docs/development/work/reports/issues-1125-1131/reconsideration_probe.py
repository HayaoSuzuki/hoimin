"""Fixed feasibility probes for revised methods; not product candidate generation."""

import ast
import json
import platform
import subprocess
import sys
import time
from pathlib import Path


def capture_replacement(parameter):
    # Keep the public parameter spelling and arity; helpers are wrapper-local.
    used = {parameter}
    helpers = []
    for stem in ("_hoimin_get", "_hoimin_missing"):
        name = stem
        while name in used:
            name += "_"
        used.add(name)
        helpers.append(name)
    getter, missing = helpers
    return (
        f"(lambda {getter}, {missing}: lambda {parameter}={missing}: "
        f"{getter}() if {parameter} is {missing} else {parameter})"
        f"(lambda: {parameter}, [])"
    )


def check(source, assertions):
    ast.parse(source)
    started = time.perf_counter()
    process = subprocess.run(
        [sys.executable, "-B", "-c", source + "\n" + assertions],
        capture_output=True,
        text=True,
        timeout=5,
        check=False,
    )
    if process.returncode == 0:
        verdict = "passed"
    elif "AssertionError" in process.stderr:
        verdict = "assertion_failed"
    else:
        verdict = "error"
    return {
        "verdict": verdict,
        "elapsed_seconds": time.perf_counter() - started,
        "stderr": process.stderr[-1000:],
    }


def main():
    cases = []
    for factory, empty, update in (
        ("list", "[]", "a.items.append(3)"),
        ("dict", "{}", "a.items['x']=3"),
        ("set", "{*()}", "a.items.add(3)"),
    ):
        source = (
            "import dataclasses as dc\n@dc.dataclass\nclass Bag:\n"
            f"    items: object = dc.field(default_factory={factory})\n"
        )
        cases.append(
            {
                "issue": 1126,
                "name": f"plain_dataclass_{factory}",
                "original": source,
                "mutant": source.replace(
                    f"default_factory={factory}",
                    f"default_factory=lambda shared={empty}: shared",
                ),
                "checks": {
                    "weak": "assert len(Bag().items)==0",
                    "strong": f"a,b=Bag(),Bag()\n{update}\nassert len(b.items)==0",
                },
            }
        )
    source = "def run(rows):\n    return sorted(rows,key=lambda row:(row[0], row[1]))\n"
    cases.append(
        {
            "issue": 1127,
            "name": "sorted_two_read_keys",
            "original": source,
            "mutant": source.replace("(row[0], row[1])", "(row[1], row[0])"),
            "checks": {
                "weak": "assert run([(2,2),(1,1)])==[(1,1),(2,2)]",
                "strong": "assert run([(2,1),(1,9),(1,3)])==[(1,3),(1,9),(2,1)]",
                "ties": "assert run([(1,3),(1,2),(1,1)])==[(1,1),(1,2),(1,3)]",
            },
        }
    )
    for parameter in ("value", "_hoimin_get", "_hoimin_missing"):
        source = (
            "def build(values):\n    callbacks=[]\n"
            f"    for {parameter} in values:\n"
            f"        callbacks.append(lambda {parameter}={parameter}: {parameter})\n"
            "    return callbacks\n"
        )
        cases.append(
            {
                "issue": 1128,
                "name": f"call_interface_preserving_{parameter}",
                "original": source,
                "mutant": source.replace(
                    f"lambda {parameter}={parameter}: {parameter}",
                    capture_replacement(parameter),
                ),
                "checks": {
                    "weak": "assert [f() for f in build([4])]==[4]",
                    "strong": "assert [f() for f in build([4,9])]==[4,9]",
                    "explicit_positional": "assert [f(4) for f in build([4,9])]==[4,4]",
                    "explicit_keyword_none": (
                        f"assert [f({parameter}=None) for f in build([4,9])]==[None,None]"
                    ),
                    "explicit_mutable": "payload=[]\nassert all(f(payload) is payload for f in build([4,9]))",
                },
            }
        )
    for style, expression in (
        ("method", "data.decode('utf-8', errors='surrogateescape')"),
        ("codecs", "codecs.decode(data, 'utf-8', errors='surrogateescape')"),
    ):
        source = (
            "import codecs\ndef run(data):\n    return "
            + expression
            + ".encode('utf-8', errors='surrogateescape')\n"
        )
        cases.append(
            {
                "issue": 1131,
                "name": f"lossless_{style}",
                "original": source,
                "mutant": source.replace(
                    "errors='surrogateescape'", "errors='ignore'", 1
                ),
                "checks": {
                    "weak": "assert run(b'abc')==b'abc'",
                    "strong": "assert run(b'a\\xffb')==b'a\\xffb'",
                },
            }
        )
    for case in cases:
        case["results"] = {
            kind: {
                phase: check(case[kind], assertions)
                for phase, assertions in case["checks"].items()
            }
            for kind in ("original", "mutant")
        }
        assert all(
            result["verdict"] == "passed"
            for result in case["results"]["original"].values()
        ), case["name"]
        assert case["results"]["mutant"]["weak"]["verdict"] == "passed", case["name"]
        assert case["results"]["mutant"]["strong"]["verdict"] == "assertion_failed", (
            case["name"]
        )
        for phase, result in case["results"]["mutant"].items():
            if phase != "strong":
                assert result["verdict"] == "passed", (case["name"], phase)
    result = {
        "python": sys.version,
        "platform": platform.platform(),
        "git_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True
        ).strip(),
        "scope": "fixed feasibility edits; not a product mutator or new incumbent comparison",
        "cases": cases,
    }
    Path(sys.argv[1]).write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
