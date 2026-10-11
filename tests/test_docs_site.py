from pathlib import Path

import markdown
import pytest

from tools.docs_site import RepositoryLinks, rewrite_url

REVISION = "a" * 40


def test_excluded_source_links_to_exact_revision(tmp_path: Path) -> None:
    source = tmp_path / "docs" / "guide.md"
    target = tmp_path / "docs" / "research.md"
    target.parent.mkdir()
    target.write_text("# Research", encoding="utf-8")

    result = rewrite_url(tmp_path, source, "research.md#finding", REVISION)

    assert result == (
        f"https://github.com/HayaoSuzuki/hoimin/blob/{REVISION}"
        "/docs/research.md#finding"
    )


def test_nonexistent_source_is_not_hidden_by_rewriting(tmp_path: Path) -> None:
    source = tmp_path / "docs" / "guide.md"

    with pytest.raises(ValueError, match="Missing documentation link"):
        rewrite_url(tmp_path, source, "missing.md", REVISION)


def test_published_page_stays_local(tmp_path: Path) -> None:
    target = tmp_path / "docs" / "usage.md"
    target.parent.mkdir()
    target.write_text("# Usage", encoding="utf-8")

    result = rewrite_url(tmp_path, tmp_path / "README.md", "docs/usage.md", REVISION)

    assert result == "docs/usage.md"


def test_readme_links_resolve_to_site_landing_page(tmp_path: Path) -> None:
    (tmp_path / "README.md").write_text("# Home", encoding="utf-8")

    result = rewrite_url(
        tmp_path, tmp_path / "docs" / "usage.md", "../README.md#install", REVISION
    )

    assert result == "../index.md#install"


def test_link_cannot_escape_repository(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="escapes repository"):
        rewrite_url(tmp_path, tmp_path / "README.md", "../outside.md", REVISION)


def test_literal_code_is_not_rewritten(tmp_path: Path) -> None:
    extension = RepositoryLinks(tmp_path, REVISION)
    extension.source = tmp_path / "README.md"
    text = "`[literal](missing.md)`\n\n```md\n[example](missing.md)\n```"

    rendered = markdown.markdown(text, extensions=["fenced_code", extension])

    assert "[literal](missing.md)" in rendered
    assert "[example](missing.md)" in rendered


def test_reference_link_uses_source_checked_repository_url(tmp_path: Path) -> None:
    (tmp_path / "LICENSE").write_text("License", encoding="utf-8")
    extension = RepositoryLinks(tmp_path, REVISION)
    extension.source = tmp_path / "README.md"

    rendered = markdown.markdown("[license][l]\n\n[l]: LICENSE", extensions=[extension])

    assert (
        f'href="https://github.com/HayaoSuzuki/hoimin/blob/{REVISION}/LICENSE"'
        in rendered
    )
