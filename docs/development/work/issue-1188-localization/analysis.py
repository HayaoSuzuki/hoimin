"""Pure inference; rejected observations are never evidence of localization.

The formulas receive complete test observations, not pytest/process objects.
Rank intervals express inspection ambiguity, not probability of a defect.
"""

import math


def ochiai(failed: int, passed: int, total_failed: int) -> float:
    """Score an element changed/covered by failing and passing tests."""
    if any(type(n) is not int for n in (failed, passed, total_failed)):
        raise ValueError("counts must be integers")
    if not 0 <= failed <= total_failed or passed < 0 or total_failed <= 0:
        raise ValueError("impossible test counts")
    return failed / math.sqrt(total_failed * (failed + passed)) if failed else 0.0


def _validated_tests(sample: dict) -> dict:
    if sample.get("complete") is not True or not sample.get("tests"):
        raise ValueError("incomplete or empty test execution")
    for name, item in sample["tests"].items():
        if not isinstance(name, str) or not name:
            raise ValueError("invalid test identity")
        if (
            item.get("setup") != "passed"
            or item.get("teardown") != "passed"
            or item.get("call") not in {"passed", "failed"}
            or item.get("special") is not False
        ):
            raise ValueError("unsupported phase or special test result")
        fingerprint = item.get("fingerprint")
        if item["call"] == "failed" and (
            not isinstance(fingerprint, str) or not fingerprint
        ):
            raise ValueError("missing failure fingerprint")
        if item["call"] == "passed" and fingerprint is not None:
            raise ValueError("unexpected passing fingerprint")
    return sample["tests"]


def stable_run(repeats: list[dict], mode="fingerprint") -> dict:
    """Require at least two complete identical observations; not a flake proof."""
    if len(repeats) < 2:
        raise ValueError("two observations are required")
    if mode not in {"outcome", "fingerprint"}:
        raise ValueError("unknown stability mode")

    def observed(sample):
        tests = _validated_tests(sample)
        return (
            tests
            if mode == "fingerprint"
            else {
                name: {
                    key: value for key, value in item.items() if key != "fingerprint"
                }
                for name, item in tests.items()
            }
        )

    first = observed(repeats[0])
    for sample in repeats[1:]:
        if observed(sample) != first:
            raise ValueError("unstable test identities or outcomes")
        if sample.get("coverage") != repeats[0].get("coverage"):
            raise ValueError("unstable coverage")
    return repeats[0]


def reactions(baseline: dict, mutant: dict, mode: str) -> set[str]:
    """Find changed outcomes; optionally include stable failure-message changes."""
    if mode not in {"outcome", "fingerprint"}:
        raise ValueError("unknown observation mode")
    original = _validated_tests(baseline)
    changed = _validated_tests(mutant)
    if original.keys() != changed.keys():
        raise ValueError("test identities changed")
    return {
        name
        for name, item in original.items()
        if item["call"] != changed[name]["call"]
        or (
            mode == "fingerprint"
            and item["fingerprint"] != changed[name]["fingerprint"]
        )
    }


def rank_intervals(scores: dict[int, float], targets: set[int]) -> dict | None:
    """Bound inspections to first target, accounting for other target lines in ties."""
    if any(not math.isfinite(s) or not 0 <= s <= 1 for s in scores.values()):
        raise ValueError("invalid score")
    present = targets & scores.keys()
    if not present:
        return None
    highest = max(scores[line] for line in present)
    above = sum(score > highest for score in scores.values())
    tied = {line for line, score in scores.items() if score == highest}
    return {
        "best": above + 1,
        "worst": above + len(tied - present) + 1,
        "tie_end": above + len(tied),
        "score": highest,
        "target_lines_present": sorted(present),
    }


def summarize(baseline, variants, source, targets):
    """Compare both reaction models against a shared executable-line universe."""
    tests = _validated_tests(baseline)
    failed = {name for name, item in tests.items() if item["call"] == "failed"}
    passed = tests.keys() - failed
    if not failed or not passed:
        raise ValueError("both passing and failing baseline tests are required")
    universe = baseline["statements"][source]
    spectra = baseline["coverage"][source]
    scores = {
        mode: dict.fromkeys(universe, 0.0)
        for mode in ("coverage", "outcome", "fingerprint")
    }
    for line in universe:
        covering = set(spectra.get(str(line), []))
        scores["coverage"][line] = ochiai(
            len(covering & failed), len(covering & passed), len(failed)
        )
    accepted = dict.fromkeys(("outcome", "fingerprint"), 0)
    rejected = dict.fromkeys(("outcome", "fingerprint"), 0)
    for variant in variants:
        for mode in ("outcome", "fingerprint"):
            try:
                if mode == "fingerprint" and not baseline.get(
                    "fingerprint_stable", True
                ):
                    raise ValueError("unstable baseline failure rendering")
                mutant = stable_run(variant["observations"], mode=mode)
                changed = reactions(baseline, mutant, mode)
            except ValueError:
                rejected[mode] += 1
                continue
            accepted[mode] += 1
            line = variant["candidate"]["line"]
            value = ochiai(len(changed & failed), len(changed & passed), len(failed))
            if line in scores[mode]:
                scores[mode][line] = max(scores[mode][line], value)
    result = {
        mode: {"rank": rank_intervals(values, targets), "scores": values}
        for mode, values in scores.items()
    }
    if not baseline.get("fingerprint_stable", True):
        result["fingerprint"] = {
            "rank": None,
            "scores": {},
            "unavailable": "unstable baseline failure rendering",
        }
    frames = {
        line
        for locations in baseline.get("tracebacks", {}).values()
        for filename, line in locations
        if filename == source
    }
    result.update(
        accepted_mutants=accepted["outcome"],
        rejected_mutants=rejected["outcome"],
        accepted_by_mode=accepted,
        rejected_by_mode=rejected,
        universe_lines=len(universe),
        failing_tests=len(failed),
        passing_tests=len(passed),
        unmapped_mutants=sum(
            v["candidate"]["line"] not in scores["outcome"] for v in variants
        ),
        traceback_lines=sorted(frames),
        traceback_hits=sorted(frames & targets),
    )
    return result
