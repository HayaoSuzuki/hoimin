"""Build the allowlisted documentation without changing its Markdown sources."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import override
from urllib.parse import quote, unquote, urlsplit, urlunsplit
from xml.etree.ElementTree import Element

from markdown import Markdown
from markdown.extensions import Extension
from markdown.treeprocessors import Treeprocessor
from mkdocs.commands.build import build
from mkdocs.config import load_config
from mkdocs.config.base import Config
from mkdocs.config.defaults import MkDocsConfig
from mkdocs.plugins import BasePlugin
from mkdocs.structure.files import Files
from mkdocs.structure.pages import Page

REPOSITORY = "https://github.com/HayaoSuzuki/hoimin"
PUBLIC_PAGES = (
    "README.md",
    "docs/usage.md",
    "docs/cli-reference.md",
    "docs/compatibility.md",
    "docs/support.md",
    "docs/licensing.md",
    "docs/development.md",
    "docs/releases.md",
    "docs/pypi-publishing.md",
    "docs/dependency-policy.md",
    "docs/dependency-build-scripts.md",
    "docs/artifact-smoke.md",
    "docs/ci.md",
    "docs/documentation.md",
)


def site_path(source: str) -> str:
    return "index.md" if source == "README.md" else source


def rewrite_url(root: Path, source: Path, url: str, revision: str) -> str:
    parsed = urlsplit(url)
    prefix = f"{REPOSITORY}/blob/main/"
    if url.startswith(prefix):
        parsed = urlsplit(url.removeprefix(prefix))
        target = root / unquote(parsed.path)
    elif parsed.scheme or parsed.netloc or not parsed.path:
        return url
    else:
        target = source.parent / unquote(parsed.path)
    target = target.resolve()
    try:
        relative = target.relative_to(root.resolve()).as_posix()
    except ValueError as error:
        msg = f"Documentation link escapes repository: {url}"
        raise ValueError(msg) from error
    if not target.exists():
        msg = f"Missing documentation link in {source}: {url}"
        raise ValueError(msg)
    if relative in PUBLIC_PAGES:
        parent = Path(site_path(source.relative_to(root).as_posix())).parent
        local = Path(os.path.relpath(site_path(relative), parent)).as_posix()
        return urlunsplit(("", "", quote(local), parsed.query, parsed.fragment))
    kind = "tree" if target.is_dir() else "blob"
    return urlunsplit(
        (
            "https",
            "github.com",
            f"/HayaoSuzuki/hoimin/{kind}/{revision}/{quote(relative)}",
            parsed.query,
            parsed.fragment,
        )
    )


class LinkProcessor(Treeprocessor):
    def __init__(self, md: Markdown, links: RepositoryLinks) -> None:
        super().__init__(md)
        self.links = links

    @override
    def run(self, root: Element) -> Element:
        for element in root.iter():
            for attribute in ("href", "src"):
                url = element.get(attribute)
                if url is not None:
                    element.set(
                        attribute,
                        rewrite_url(
                            self.links.root,
                            self.links.source,
                            url,
                            self.links.revision,
                        ),
                    )
        return root


class RepositoryLinks(Extension):
    def __init__(self, root: Path, revision: str) -> None:
        super().__init__()
        self.root = root.resolve()
        self.revision = revision
        self.source = self.root / "README.md"

    @override
    def extendMarkdown(self, md: Markdown) -> None:
        # Parsed links include reference links and exclude literal/fenced code.
        # Run after inline parsing (20), before MkDocs relative links (0).
        md.treeprocessors.register(LinkProcessor(md, self), "repository_links", 9)


class SourceConfig(Config):
    """No user configuration is accepted by the internal source plugin."""


class SourcePlugin(BasePlugin[SourceConfig]):
    def __init__(self, links: RepositoryLinks) -> None:
        super().__init__()
        self.links = links

    @override
    def on_page_markdown(
        self, markdown: str, *, page: Page, config: MkDocsConfig, files: Files
    ) -> str:
        del config, files
        source = "README.md" if page.file.src_uri == "index.md" else page.file.src_uri
        self.links.source = self.links.root / source
        revision = self.links.revision
        return (
            f"Main documentation · [Source commit {revision}]"
            f"({REPOSITORY}/commit/{revision}).\n\n" + markdown
        )


def prepare_sources(root: Path, destination: Path, revision: str) -> None:
    del revision  # The rendering plugin adds identity without editing source files.
    for source in PUBLIC_PAGES:
        target = destination / site_path(source)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root / source, target)


def build_site(root: Path, output: Path) -> None:
    root = root.resolve()
    output = output.resolve()
    # MkDocs cleans its output recursively: permit only the dedicated site folder.
    if output != root / "site" or output.is_symlink():
        msg = "Documentation output must be the repository's site directory"
        raise ValueError(msg)
    git = shutil.which("git")
    if git is None:
        msg = "Git is required to identify documentation sources"
        raise FileNotFoundError(msg)
    revision = subprocess.run(  # noqa: S603 -- resolved git, fixed arguments, no shell
        [git, "-C", str(root), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        check=True,
        timeout=30,
    ).stdout.strip()
    with tempfile.TemporaryDirectory(prefix="hoimin-docs-") as temporary:
        sources = Path(temporary) / "sources"
        prepare_sources(root, sources, revision)
        links = RepositoryLinks(root, revision)
        config = load_config(
            str(root / "mkdocs.yml"),
            docs_dir=str(sources),
            site_dir=str(output),
            strict=True,
        )
        # MkDocs annotates strings only; Python-Markdown supports Extension objects.
        config.markdown_extensions.append(links)  # ty: ignore[invalid-argument-type]
        config.plugins["repository-links"] = SourcePlugin(links)
        build(config)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("site"))
    args = parser.parse_args()
    build_site(Path(__file__).resolve().parents[1], args.output)


if __name__ == "__main__":
    main()
