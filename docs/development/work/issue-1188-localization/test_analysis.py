"""Independent checks of the experiment's inference boundary."""

import math

import pytest
from analysis import ochiai, rank_intervals, reactions, stable_run
from hypothesis import given
from hypothesis import strategies as st


def run(tests):
    return {"complete": True, "tests": tests}


def observation(state="passed", fingerprint=None):
    return {
        "setup": "passed",
        "call": state,
        "teardown": "passed",
        "fingerprint": fingerprint,
        "special": False,
    }


@pytest.mark.parametrize(
    "failed,passed,total,expected",
    [
        (0, 0, 1, 0.0),
        (1, 0, 1, 1.0),
        (1, 1, 2, 0.5),
        (2, 2, 2, math.sqrt(0.5)),
    ],
)
def test_ochiai_matches_hand_calculated_scores(failed, passed, total, expected):
    # Arrange
    counts = failed, passed, total
    # Act
    actual = ochiai(*counts)
    # Assert
    assert actual == pytest.approx(expected)


@pytest.mark.parametrize(
    "counts", [(-1, 0, 1), (1, -1, 1), (2, 0, 1), (0, 0, 0), (True, 0, 1)]
)
def test_ochiai_rejects_impossible_counts(counts):
    # Arrange / Act / Assert
    with pytest.raises(ValueError):
        ochiai(*counts)


@pytest.mark.parametrize("mode,expected", [("outcome", set()), ("fingerprint", {"f"})])
def test_fail_to_fail_change_is_separate_from_outcome_change(mode, expected):
    # Arrange
    base = run({"p": observation(), "f": observation("failed", "a")})
    mutant = run({"p": observation(), "f": observation("failed", "b")})
    # Act
    actual = reactions(base, mutant, mode)
    # Assert
    assert actual == expected


@pytest.mark.parametrize(
    "field,value",
    [
        ("setup", "failed"),
        ("teardown", "failed"),
        ("call", "skipped"),
        ("call", None),
        ("special", True),
    ],
)
def test_unsupported_test_phases_are_incomparable(field, value):
    # Arrange
    item = observation()
    item[field] = value
    sample = run({"p": item})
    # Act / Assert
    with pytest.raises(ValueError):
        stable_run([sample, sample])


def test_test_identity_mismatch_is_incomparable():
    # Arrange
    base, mutant = run({"a": observation()}), run({"b": observation()})
    # Act / Assert
    with pytest.raises(ValueError):
        reactions(base, mutant, "outcome")


def test_repeated_failure_messages_must_agree():
    # Arrange
    first = run({"f": observation("failed", "one")})
    second = run({"f": observation("failed", "two")})
    # Act / Assert
    with pytest.raises(ValueError):
        stable_run([first, second])


def test_incomplete_execution_is_not_zero_reactions():
    # Arrange
    sample = {"complete": False, "tests": {"p": observation()}}
    # Act / Assert
    with pytest.raises(ValueError):
        stable_run([sample, sample])


@pytest.mark.parametrize(
    "targets,expected", [({2}, (2, 4)), ({2, 3}, (2, 3)), ({1}, (1, 1)), ({5}, (5, 5))]
)
def test_tie_ranks_do_not_depend_on_source_line_order(targets, expected):
    # Arrange
    scores = {1: 1.0, 2: 0.5, 3: 0.5, 4: 0.5, 5: 0.0}
    # Act
    result = rank_intervals(scores, targets)
    # Assert
    assert (result["best"], result["worst"]) == expected


def test_absent_target_is_not_reported_as_localized():
    # Arrange
    scores = {1: 0.0}
    # Act
    result = rank_intervals(scores, {9})
    # Assert
    assert result is None


@given(st.integers(1, 100), st.integers(0, 100))
def test_ochiai_is_bounded_for_valid_counts(total, passed):
    # Arrange
    failed = total
    # Act
    score = ochiai(failed, passed, total)
    # Assert
    assert 0 <= score <= 1


@given(st.integers(1, 100), st.integers(0, 100))
def test_additional_passing_reactions_cannot_raise_suspiciousness(failed, passed):
    # Arrange
    initial = ochiai(failed, passed, failed)
    # Act
    additional = ochiai(failed, passed + 1, failed)
    # Assert
    assert additional <= initial


@given(st.permutations([(1, 0.5), (2, 0.5), (3, 1.0), (4, 0.0)]))
def test_input_order_cannot_break_score_ties(entries):
    # Arrange
    scores = dict(entries)
    # Act
    actual = rank_intervals(scores, {1})
    # Assert
    assert (actual["best"], actual["worst"]) == (2, 3)


def test_aggregate_reactions_preserves_zero_score_lines_and_ties():
    # Arrange
    from analysis import summarize

    base = run({"p": observation(), "f": observation("failed", "a")})
    base.update(
        coverage={"s.py": {"1": ["f", "p"], "2": ["f"]}},
        statements={"s.py": [1, 2, 3]},
        tracebacks={"f": [["s.py", 1]]},
    )
    mutant = run({"p": observation(), "f": observation("failed", "b")})
    variants = [{"candidate": {"line": 1}, "observations": [mutant, mutant]}]
    # Act
    result = summarize(base, variants, "s.py", {1})
    # Assert
    assert result["fingerprint"]["rank"]["best"] == 1
    assert result["outcome"]["rank"]["worst"] == 3
    assert result["coverage"]["rank"]["best"] == 2


def test_missing_repeat_is_excluded_from_rank_evidence():
    # Arrange
    from analysis import summarize

    base = run({"p": observation(), "f": observation("failed", "a")})
    base.update(coverage={"s.py": {}}, statements={"s.py": [1]}, tracebacks={})
    mutant = run({"p": observation(), "f": observation()})
    # Act
    result = summarize(
        base, [{"candidate": {"line": 1}, "observations": [mutant]}], "s.py", {1}
    )
    # Assert
    assert result["accepted_mutants"] == 0
    assert result["rejected_mutants"] == 1


def test_stable_outcomes_do_not_require_stable_failure_rendering():
    # Arrange
    first = run({"f": observation("failed", "port-1")})
    second = run({"f": observation("failed", "port-2")})
    # Act
    actual = stable_run([first, second], mode="outcome")
    # Assert
    assert actual["tests"]["f"]["call"] == "failed"


def test_unstable_baseline_fingerprint_does_not_produce_fingerprint_ranking():
    # Arrange
    from analysis import summarize

    base = run({"p": observation(), "f": observation("failed", "a")})
    base.update(
        coverage={"s.py": {}}, statements={"s.py": [1]}, fingerprint_stable=False
    )
    mutant = run({"p": observation(), "f": observation()})
    # Act
    result = summarize(
        base,
        [{"candidate": {"line": 1}, "observations": [mutant, mutant]}],
        "s.py",
        {1},
    )
    # Assert
    assert result["fingerprint"]["rank"] is None
    assert result["outcome"]["rank"]["score"] == 1.0


def test_mutants_outside_line_universe_are_counted_explicitly():
    # Arrange
    from analysis import summarize

    base = run({"p": observation(), "f": observation("failed", "a")})
    base.update(coverage={"s.py": {}}, statements={"s.py": [1]})
    mutant = run({"p": observation(), "f": observation()})
    # Act
    result = summarize(
        base,
        [{"candidate": {"line": 2}, "observations": [mutant, mutant]}],
        "s.py",
        {1},
    )
    # Assert
    assert result["unmapped_mutants"] == 1
    assert result["outcome"]["rank"]["score"] == 0.0
