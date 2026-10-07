from pathlib import Path

import pytest


@pytest.fixture(scope="module")
def usage() -> str:
    return (Path(__file__).resolve().parents[1] / "docs/usage.md").read_text(
        encoding="utf-8"
    )


@pytest.mark.parametrize(
    "text",
    [
        "hoimin verify PLAN.json --top 10",
        "ranking_reasons",
        "ordering heuristics",
        "mutually exclusive",
        "Version-1 manifests",
        "top N among retained candidates",
    ],
)
def test_documents_ranked_two_command_workflow(usage: str, text: str) -> None:
    assert text in usage


@pytest.mark.parametrize(
    "text",
    [
        "never re-ranks",
        "lower-ranked candidates remain valid",
        "selects every retained candidate",
        "reports the actual selected count",
    ],
)
def test_documents_saved_rank_and_oversized_top_semantics(
    usage: str, text: str
) -> None:
    assert text in usage
