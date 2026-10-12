import re
from pathlib import Path

import markdown
import pytest

from tools import docs_site
from tools.docs_site import RepositoryLinks, rewrite_url

REVISION = "a" * 40


def test_site_renders_code_inside_numbered_steps() -> None:
    root = Path(__file__).resolve().parents[1]
    config = docs_site.load_config(config_file=str(root / "mkdocs.yml"))
    source = (
        "1. Create the source file:\n\n"
        "    ```python\n"
        "    def add(left, right):\n"
        "        return left + right\n"
        "    ```\n"
    )

    rendered = markdown.markdown(
        source,
        extensions=config.markdown_extensions,
        extension_configs=config.mdx_configs,
    )
    text = re.sub(r"<[^>]+>", "", rendered)

    assert re.search(
        r"<li>.*<pre\b.*<code\b.*</code>.*</pre>.*</li>", rendered, re.DOTALL
    )
    assert "def add(left, right):\n    return left + right" in text


def test_site_preparation_copies_only_allowlisted_theme_assets(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "repository"
    styles = root / "docs" / "stylesheets"
    styles.mkdir(parents=True)
    (styles / "extra.css").write_bytes(b".md-sidebar--secondary { display: none; }")
    (styles / "private.css").write_bytes(b"unpublished")
    destination = tmp_path / "sources"
    monkeypatch.setattr(docs_site, "PUBLIC_PAGES", ())

    docs_site.prepare_sources(root, destination, REVISION)

    assert (destination / "stylesheets" / "extra.css").read_bytes() == (
        styles / "extra.css"
    ).read_bytes()
    assert not (destination / "stylesheets" / "private.css").exists()


def test_project_site_absolute_path_becomes_same_site_url(tmp_path: Path) -> None:
    url = "/hoimin/docs/usage/?review=yes#results"

    result = rewrite_url(tmp_path, tmp_path / "README.md", url, REVISION)

    assert result == f"https://hayaosuzuki.github.io{url}"


def test_missing_same_site_absolute_url_is_rejected(tmp_path: Path) -> None:
    (tmp_path / "index.html").write_text(
        '<a href="https://hayaosuzuki.github.io/hoimin/missing/">bad</a>',
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="Missing built documentation target"):
        docs_site.validate_site(tmp_path)


def test_missing_same_site_absolute_fragment_is_rejected(tmp_path: Path) -> None:
    (tmp_path / "index.html").write_text(
        '<a href="https://hayaosuzuki.github.io/hoimin/#missing">bad</a>',
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="Missing built documentation anchor"):
        docs_site.validate_site(tmp_path)


def test_missing_raw_html_relative_link_is_rejected(tmp_path: Path) -> None:
    (tmp_path / "index.html").write_text(
        '<a href="docs/missing.html">bad</a>', encoding="utf-8"
    )

    with pytest.raises(ValueError, match="Missing built documentation target"):
        docs_site.validate_site(tmp_path)


def test_missing_raw_html_same_page_fragment_is_rejected(tmp_path: Path) -> None:
    (tmp_path / "index.html").write_text('<a href="#missing">bad</a>', encoding="utf-8")

    with pytest.raises(ValueError, match="Missing built documentation anchor"):
        docs_site.validate_site(tmp_path)


def test_built_links_preserve_encoded_query_and_fragment(tmp_path: Path) -> None:
    (tmp_path / "index.html").write_text(
        '<a href="/hoimin/docs/%75sage/?review=yes#results">usage</a>',
        encoding="utf-8",
    )
    target = tmp_path / "docs" / "usage" / "index.html"
    target.parent.mkdir(parents=True)
    target.write_text('<h1 id="results">Results</h1>', encoding="utf-8")

    docs_site.validate_site(tmp_path)


def test_html_checker_ignores_literal_examples_and_external_urls(
    tmp_path: Path,
) -> None:
    (tmp_path / "index.html").write_text(
        '<code>&lt;a href="missing.html"&gt;example&lt;/a&gt;</code>'
        '<a href="https://example.org/missing/#unknown">external</a>',
        encoding="utf-8",
    )

    docs_site.validate_site(tmp_path)


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
