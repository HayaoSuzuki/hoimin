import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import create_autospec

import markdown
import pytest
from pytest_mock import MockerFixture

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
    tmp_path: Path, mocker: MockerFixture
) -> None:
    root = tmp_path / "repository"
    styles = root / "docs" / "assets" / "stylesheets"
    styles.mkdir(parents=True)
    (styles / "extra.css").write_bytes(b".md-sidebar--secondary { display: none; }")
    (styles / "private.css").write_bytes(b"unpublished")
    destination = tmp_path / "sources"
    mocker.patch.object(docs_site, "PUBLIC_PAGES", ())

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


@pytest.mark.parametrize(
    ("source", "published"),
    [
        ("docs/user/guides/usage.md", "docs/usage.md"),
        ("docs/user/reference/cli.md", "docs/cli-reference.md"),
        ("docs/development/guides/contributing.md", "docs/development.md"),
    ],
    ids=["user-guide", "user-reference", "contributor-guide"],
)
def test_published_page_stays_local(
    tmp_path: Path, source: str, published: str
) -> None:
    target = tmp_path / source
    target.parent.mkdir(parents=True)
    target.write_text("# Guide", encoding="utf-8")

    result = rewrite_url(tmp_path, tmp_path / "README.md", source, REVISION)

    assert result == published


def test_readme_links_resolve_to_site_landing_page(tmp_path: Path) -> None:
    (tmp_path / "README.md").write_text("# Home", encoding="utf-8")

    result = rewrite_url(
        tmp_path,
        tmp_path / "docs" / "user" / "guides" / "usage.md",
        "../../../README.md#install",
        REVISION,
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


def test_project_root_without_trailing_slash() -> None:
    assert docs_site.project_path("https://hayaosuzuki.github.io/hoimin") == ""


def test_github_main_links_are_pinned(tmp_path: Path) -> None:
    (tmp_path / "private.txt").write_text("private")
    assert (
        rewrite_url(
            tmp_path,
            tmp_path / "README.md",
            docs_site.REPOSITORY + "/blob/main/private.txt",
            REVISION,
        )
        == docs_site.REPOSITORY + "/blob/" + REVISION + "/private.txt"
    )
    assert (
        rewrite_url(
            tmp_path, tmp_path / "README.md", "mailto:user@example.test", REVISION
        )
        == "mailto:user@example.test"
    )


def test_html_boolean_attributes_and_legacy_anchors() -> None:
    page = docs_site.BuiltPage('<a download name="legacy" href="#legacy">link</a>')
    assert page.anchors == {"legacy"}
    assert page.urls == ["#legacy"]


def test_built_link_cannot_escape_output(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="escapes site"):
        docs_site.built_target(tmp_path.resolve(), tmp_path / "index.html", "../secret")


def test_build_rejects_unsafe_output(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="repository's site directory"):
        docs_site.build_site(tmp_path, tmp_path / "other")


def test_build_rejects_missing_git(tmp_path: Path, mocker: MockerFixture) -> None:
    original_which_spec = docs_site.shutil.which
    mocker.patch.object(
        docs_site.shutil,
        "which",
        create_autospec(original_which_spec, side_effect=lambda _: None),
    )

    with pytest.raises(FileNotFoundError, match="Git is required"):
        docs_site.build_site(tmp_path, tmp_path / "site")


@pytest.fixture
def documentation_repository(tmp_path: Path) -> Path:
    root = Path(__file__).resolve().parents[1]
    repository = tmp_path / "repository"
    repository.mkdir()
    for source in (
        *docs_site.PUBLIC_PAGES,
        "mkdocs.yml",
        "docs/assets/stylesheets/extra.css",
    ):
        target = repository / source
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root / source, target)
    # Render minimal allowlisted sources so links need no unpublished fixtures.
    for source in docs_site.PUBLIC_PAGES:
        (repository / source).write_text(
            "# Example\n\n[Home]("
            + (
                "README.md"
                if source == "README.md"
                else str(
                    Path(
                        os.path.relpath(
                            repository / "README.md", (repository / source).parent
                        )
                    )
                )
            )
            + ")\n"
        )
    git = shutil.which("git")
    assert git is not None
    for arguments in (
        ["init"],
        [
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "--allow-empty",
            "-m",
            "source",
        ],
    ):
        subprocess.run(  # noqa: S603 -- fixed commands in isolated repository
            [git, "-C", str(repository), *arguments], check=True, capture_output=True
        )
    return repository


def test_full_documentation_build(documentation_repository: Path) -> None:
    repository = documentation_repository

    docs_site.build_site(repository, repository / "site")

    assert (repository / "site/index.html").is_file()
    assert "Source commit" in (repository / "site/index.html").read_text()


def test_build_rejects_mismatched_public_site_url(
    tmp_path: Path, mocker: MockerFixture
) -> None:

    original_which_spec = docs_site.shutil.which
    original_run_spec = docs_site.subprocess.run
    original_prepare_sources_spec = docs_site.prepare_sources
    original_load_config_spec = docs_site.load_config
    mocker.patch.object(
        docs_site.shutil,
        "which",
        create_autospec(original_which_spec, side_effect=lambda _: "/bin/git"),
    )
    mocker.patch.object(
        docs_site.subprocess,
        "run",
        create_autospec(
            original_run_spec,
            side_effect=lambda *_args, **_kwargs: subprocess.CompletedProcess(
                [], 0, stdout=REVISION
            ),
        ),
    )
    mocker.patch.object(
        docs_site,
        "prepare_sources",
        create_autospec(original_prepare_sources_spec, side_effect=lambda *_: None),
    )
    mocker.patch.object(
        docs_site,
        "load_config",
        create_autospec(
            original_load_config_spec,
            side_effect=lambda *_args, **_kwargs: SimpleNamespace(
                site_url="https://example.test/"
            ),
        ),
    )
    with pytest.raises(ValueError, match="site_url must match"):
        docs_site.build_site(tmp_path, tmp_path / "site")


def test_main_passes_output_to_builder(tmp_path: Path, mocker: MockerFixture) -> None:

    original_build_site_spec = docs_site.build_site
    calls = []
    mocker.patch.object(sys, "argv", ["docs", "--output", str(tmp_path / "site")])
    mocker.patch.object(
        docs_site,
        "build_site",
        create_autospec(
            original_build_site_spec,
            side_effect=lambda root, output: calls.append((root, output)),
        ),
    )
    docs_site.main()
    assert calls == [(Path(docs_site.__file__).resolve().parents[1], tmp_path / "site")]
