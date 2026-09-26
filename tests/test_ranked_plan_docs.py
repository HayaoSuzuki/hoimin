from pathlib import Path

import pytest


@pytest.fixture(scope="module")
def readme() -> str:
    return (Path(__file__).resolve().parents[1] / "README.md").read_text(
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
def test_documents_ranked_two_command_workflow(readme: str, text: str) -> None:
    assert text in readme


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
    readme: str, text: str
) -> None:
    assert text in readme
