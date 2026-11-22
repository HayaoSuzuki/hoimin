"""Recompute the public evidence summary and reject inconsistent records."""

import argparse
import gzip
import hashlib
import json
from pathlib import Path

from analysis import stable_run, summarize
from evaluate import completion, select_candidates

HERE = Path(__file__).resolve().parent


def canonical(value):
    return json.loads(json.dumps(value))


def verify(record, case):
    """Check provenance, frozen selection, repeats and derived ranks for one case."""
    assert record["case"] == case
    assert not record.get("error"), record.get("error")
    receipt = record["prepared_inputs"]
    assert receipt["case"] == case
    assert receipt["files"]["buggy-source.bin"] == record["source_sha256"]
    assert receipt["files"]["fixed-source.bin"] == record["fixed_source_sha256"]
    assert set(record["harness_sha256"]) == {
        "analysis.py",
        "collect.py",
        "evaluate.py",
        "prepare_public.py",
    }
    for name, digest in record["harness_sha256"].items():
        # Git can normalize the Windows run's CRLF source on another checkout.
        source = (HERE / name).read_bytes().replace(b"\r\n", b"\n")
        assert digest in {
            hashlib.sha256(source).hexdigest(),
            hashlib.sha256(source.replace(b"\n", b"\r\n")).hexdigest(),
        }, name
    fixed = stable_run(record["fixed"])
    assert all(test["call"] == "passed" for test in fixed["tests"].values())
    baseline = dict(stable_run(record["baseline"], mode="outcome"))
    assert baseline["tests"].keys() == fixed["tests"].keys()
    try:
        stable_run(record["baseline"])
        baseline["fingerprint_stable"] = True
    except ValueError:
        baseline["fingerprint_stable"] = False
    failed = {
        name for name, test in baseline["tests"].items() if test["call"] == "failed"
    }
    eligible = {
        int(line)
        for line, ids in baseline["coverage"][case["source"]].items()
        if set(ids) & (failed | {""})
    }
    assert sorted(eligible) == record["eligible_lines"]
    plan = record["plan"]
    assert all(candidate["path"] == case["source"] for candidate in plan["candidates"])
    selected = select_candidates(
        plan["candidates"], eligible, record["candidate_limit"]
    )
    assert [candidate["id"] for candidate in selected] == record["selected_ids"]
    eligible_count = sum(c["line"] in eligible for c in plan["candidates"])
    assert eligible_count == record["eligible_candidates"]
    assert record["sample_exhaustive"] == (
        len(selected) == eligible_count and not plan["truncated"]
    )
    variants = record["variants"]
    assert [variant["candidate"] for variant in variants] == selected[: len(variants)]
    assert all(len(variant["observations"]) <= 2 for variant in variants)
    assert completion(variants, len(selected)) == (
        record["complete"],
        record["stop_reason"],
    )
    analysis = summarize(baseline, variants, case["source"], set(case["targets"]))
    assert canonical(analysis) == record["analysis"]
    ranks = {
        mode: analysis[mode]["rank"] for mode in ("coverage", "outcome", "fingerprint")
    }
    return {
        "case": case["name"],
        "record_sha256": hashlib.sha256(
            json.dumps(record, sort_keys=True).encode()
        ).hexdigest(),
        "complete": record["complete"],
        "sample_exhaustive": record["sample_exhaustive"],
        "stop_reason": record["stop_reason"],
        "seconds": record["seconds"],
        "preparation_seconds": record["preparation_seconds"],
        "baseline_observation_seconds": sum(
            r["process"]["seconds"] for r in record["baseline"]
        ),
        "generated": len(plan["candidates"]),
        "plan_truncated": plan["truncated"],
        "eligible": record["eligible_candidates"],
        "selected": len(selected),
        "attempted": len(variants),
        "ranks": ranks,
        "top_k_bounds": {
            mode: {
                str(k): None
                if rank is None
                else {
                    "possible": rank["best"] <= k,
                    "guaranteed": rank["worst"] <= k,
                }
                for k in (1, 3, 5, 10)
            }
            for mode, rank in ranks.items()
        },
        **{
            name: analysis[name]
            for name in (
                "accepted_by_mode",
                "rejected_by_mode",
                "universe_lines",
                "failing_tests",
                "passing_tests",
                "unmapped_mutants",
                "traceback_lines",
                "traceback_hits",
            )
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--records", type=Path, default=HERE / "evidence")
    parser.add_argument("--write-summary", type=Path)
    args = parser.parse_args()
    cases = json.loads((HERE / "cases.json").read_text(encoding="utf-8"))
    summaries = []
    for case in cases:
        path = args.records / (case["name"] + ".json")
        content = (
            path.read_bytes()
            if path.exists()
            else gzip.decompress(path.with_suffix(".json.gz").read_bytes())
        )
        summaries.append(verify(json.loads(content), case))
    if args.write_summary:
        args.write_summary.write_text(
            json.dumps(summaries, indent=2) + "\n", encoding="utf-8"
        )
    else:
        expected = json.loads(
            (HERE / "evidence" / "summary.json").read_text(encoding="utf-8")
        )
        assert summaries == expected
    print(
        f"Verified {len(summaries)} public cases, selections, observations and rankings."
    )


if __name__ == "__main__":
    main()
