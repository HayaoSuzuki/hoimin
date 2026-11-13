"""Small filesystem fixtures for the knowledge reference inventory."""

import json
import sys
from pathlib import Path

import pytest
from pytest_mock import MockerFixture

from tools import knowledge_references
from tools.knowledge_references import inventory


def test_missing_sources_links_and_unreachable_pages_are_independent(
    tmp_path: Path,
) -> None:
    (tmp_path / "index.md").write_text(
        "[entry](entry.md#section)\n[remote](https://example.org/x)\n"
    )
    (tmp_path / "entry.md").write_text(
        "---\ntype: Reference\nsources:\n"
        "  - id: missing\n    resource: absent.rs\n"
        "  - id: external\n    resource: https://example.org/source\n---\n"
        "[bad](gone.rs)\n[again](gone.rs)\n[ref][next]\n"
        "[next]: nested/child.md\n```\n[fake](ignored.rs)\n```\n"
    )
    (tmp_path / "nested").mkdir()
    (tmp_path / "nested/child.md").write_text("[root](/index.md)\n")
    (tmp_path / "orphan.md").write_text("orphan\n")
    result = inventory(tmp_path)

    assert len(result["missing_markdown"]) == 2
    assert {row["target"] for row in result["missing_markdown"]} == {"gone.rs"}
    assert [row["id"] for row in result["missing_sources"]] == ["missing"]
    assert result["unreachable_pages"] == ["orphan.md"]
    assert len(result["sources"]) == 2


def test_source_resource_does_not_make_a_concept_navigable(tmp_path: Path) -> None:
    (tmp_path / "index.md").write_text(
        '---\nokf_version: "0.2"\nsources:\n'
        "  - id: source\n    resource: hidden.md\n---\n"
    )
    (tmp_path / "hidden.md").write_text("hidden\n")
    result = inventory(tmp_path)

    assert result["unreachable_pages"] == ["hidden.md"]
    assert result["missing_sources"] == []


def test_missing_entry_point_is_rejected(tmp_path: Path) -> None:
    (tmp_path / "concept.md").write_text("concept\n")
    with pytest.raises(FileNotFoundError, match="Knowledge entry point missing"):
        inventory(tmp_path)


def test_unused_reference_definition_does_not_create_navigation(tmp_path: Path) -> None:
    (tmp_path / "index.md").write_text("[unused]: hidden.md\n")
    (tmp_path / "hidden.md").write_text("hidden\n")
    result = inventory(tmp_path)

    assert result["unreachable_pages"] == ["hidden.md"]
    assert [row["target"] for row in result["markdown"]] == ["hidden.md"]


def test_used_references_and_footnotes_create_navigation(tmp_path: Path) -> None:
    (tmp_path / "index.md").write_text(
        "[full][CHILD] [collapsed][] [shortcut] [^used]\n"
        "[child]: child.md\n[collapsed]: collapsed.md\n[shortcut]: shortcut.md\n"
        "[^used]: [nested][footnote-source]\n"
        "[footnote-source]: evidence.md\n"
        "[^unused]: [hidden](hidden.md)\n"
    )
    for name in ("child", "collapsed", "shortcut", "evidence", "hidden"):
        (tmp_path / f"{name}.md").write_text(name)
    result = inventory(tmp_path)

    assert result["unreachable_pages"] == ["hidden.md"]
    assert result["missing_markdown"] == []


def test_fragment_only_links_refer_to_current_page(tmp_path: Path) -> None:
    (tmp_path / "index.md").write_text("[section](#details)\n", encoding="utf-8")
    result = inventory(tmp_path)
    assert result["missing_markdown"] == []
    assert result["unreachable_pages"] == []
    assert result["markdown"] == [
        {"page": "index.md", "target": "#details", "local": True, "exists": True}
    ]


def test_multiline_footnote_links_are_reachable(tmp_path: Path) -> None:
    (tmp_path / "index.md").write_text(
        "See [^note].\n\n[^note]: First paragraph.\n\n    [continued](child.md)\n",
        encoding="utf-8",
    )
    (tmp_path / "child.md").write_text("Child", encoding="utf-8")
    result = inventory(tmp_path)
    assert result["unreachable_pages"] == []
    assert result["missing_markdown"] == []


@pytest.mark.parametrize("broken", [False, True])
def test_inventory_cli_reports_navigation_failures(
    tmp_path: Path,
    mocker: MockerFixture,
    capsys: pytest.CaptureFixture[str],
    *,
    broken: bool,
) -> None:
    (tmp_path / "index.md").write_text(
        "[missing](missing.md)" if broken else "Entry", encoding="utf-8"
    )
    mocker.patch.object(sys, "argv", ["knowledge-references", "--root", str(tmp_path)])
    assert knowledge_references.main() == int(broken)
    result = json.loads(capsys.readouterr().out)
    assert result["page_count"] == 1
    assert bool(result["missing_markdown"]) is broken


def test_cyclic_navigation_and_repeated_references_terminate(tmp_path: Path) -> None:
    (tmp_path / "index.md").write_text("[a](a.md) [b](b.md)\n", encoding="utf-8")
    (tmp_path / "a.md").write_text(
        "[b][B] [b][B] [^n] [^n]\n[B]: b.md\n[^n]: [^n]\n", encoding="utf-8"
    )
    (tmp_path / "b.md").write_text("[a](a.md)\n", encoding="utf-8")
    assert inventory(tmp_path)["unreachable_pages"] == []


def test_mismatched_fence_markers_do_not_end_code() -> None:
    body = "````\n~~~\n```\n[hidden](missing.md)\n````\n[shown](shown.md)"
    assert list(knowledge_references.markdown_targets(body)) == ["shown.md"]
