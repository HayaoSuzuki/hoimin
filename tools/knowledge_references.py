"""Inventory knowledge Markdown/source references and root navigation, read-only.

Requires the PyYAML used by the OKF workflow. Handles inline links and reference
link definitions outside fenced code; it does not validate heading fragments,
HTML links, external availability, or arbitrary Markdown extensions.
"""

import argparse
import json
import re
import sys
from collections.abc import Iterator
from pathlib import Path
from typing import Any, TypedDict
from urllib.parse import unquote, urlsplit

import yaml


class Inventory(TypedDict):
    page_count: int
    markdown: list[dict[str, Any]]
    sources: list[dict[str, Any]]
    missing_markdown: list[dict[str, Any]]
    missing_sources: list[dict[str, Any]]
    unreachable_pages: list[str]


def local_target(page: Path, target: str, root: Path) -> Path | None:
    parsed = urlsplit(target)
    if parsed.scheme or parsed.netloc:
        return None
    path = unquote(parsed.path)
    if not path:
        return page
    return (
        (root / path.lstrip("/")) if path.startswith("/") else page.parent / path
    ).resolve()


def unfenced_lines(body: str) -> Iterator[str]:
    fence: str | None = None
    for line in body.splitlines():
        marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
        if marker:
            value = str(marker[1])
            if fence is None:
                fence = value
            elif value[0] == fence[0] and len(value) >= len(fence):
                fence = None
            continue
        if fence is None:
            yield line


def reference_label(label: str) -> str:
    return " ".join(label.split()).casefold()


def reference_definitions(
    all_lines: list[str],
) -> tuple[dict[str, str], dict[str, list[str]], list[str]]:
    definitions: dict[str, str] = {}
    footnotes: dict[str, list[str]] = {}
    ordinary: list[str] = []
    active_note: str | None = None
    for line in all_lines:
        note = re.match(r"^\s{0,3}\[\^([^\]]+)\]:\s*(.*)", line)
        if note:
            active_note = reference_label(str(note[1]))
            footnotes[active_note] = [str(note[2])]
            continue
        if active_note is not None and (line.startswith(("    ", "\t")) or not line):
            footnotes[active_note].append(line)
            continue
        active_note = None
        definition = re.match(
            r"^\s{0,3}\[(?!\^)([^\]]+)\]:\s*(?:<([^>]+)>|(\S+))", line
        )
        if definition:
            label = reference_label(str(definition[1]))
            definitions.setdefault(label, str(definition[2] or definition[3]))
        else:
            ordinary.append(line)

    return definitions, footnotes, ordinary


def reachable_footnote_lines(
    ordinary: list[str], footnotes: dict[str, list[str]]
) -> list[str]:
    # Expand reachable footnotes, including notes referenced by another note.
    pending = ordinary.copy()
    used_notes: set[str] = set()
    lines: list[str] = []
    while pending:
        line = pending.pop()
        lines.append(line)
        for raw_label in re.findall(r"\[\^([^\]]+)\]", line):
            label = reference_label(raw_label)
            if label not in used_notes:
                used_notes.add(label)
                pending.extend(footnotes.get(label, []))
    return lines


def markdown_targets(body: str, *, navigation: bool = False) -> Iterator[str]:
    """Inventory definitions, but use only referenced definitions for navigation.

    Footnote bodies contribute navigation only when their note is referenced.
    Reference labels support full, collapsed and shortcut forms. This intentionally
    remains a limited Markdown scanner, not a complete renderer.
    """
    all_lines = list(unfenced_lines(body))
    definitions, footnotes, ordinary = reference_definitions(all_lines)
    lines = reachable_footnote_lines(ordinary, footnotes) if navigation else all_lines

    for line in lines:
        for match in re.finditer(
            r"\]\(\s*(?:<([^>]+)>|([^\s)]+))(?:\s+[^)]*)?\)", line
        ):
            yield str(match[1] or match[2])

    if not navigation:
        yield from definitions.values()
        return
    used_labels: set[str] = set()
    for line in lines:
        for match in re.finditer(r"\[(?!\^)([^\]]+)\](?:\[([^\]]*)\])?", line):
            # Inline link text is not a shortcut reference to a same-name label.
            if line[match.end() :].startswith("("):
                continue
            label = reference_label(str(match[2] or match[1]))
            if label in definitions and label not in used_labels:
                used_labels.add(label)
                yield definitions[label]


def inventory(root: Path) -> Inventory:
    root = Path(root).resolve()
    if not (root / "index.md").is_file():
        message = f"Knowledge entry point missing: {root / 'index.md'}"
        raise FileNotFoundError(message)
    pages = sorted(root.rglob("*.md"))
    links: list[dict[str, Any]] = []
    sources: list[dict[str, Any]] = []
    graph: dict[Path, set[Path]] = {}
    for page in pages:
        text = page.read_text(encoding="utf-8")
        meta = {}
        body = text
        if text.startswith("---\n"):
            front, body = text[4:].split("\n---\n", 1)
            meta = yaml.safe_load(front) or {}
        name = str(page.relative_to(root))
        graph[page] = set()
        for target in markdown_targets(body):
            resolved = local_target(page, target, root)
            links.append(
                {
                    "page": name,
                    "target": target,
                    "local": resolved is not None,
                    "exists": resolved.exists() if resolved is not None else None,
                }
            )
        for target in markdown_targets(body, navigation=True):
            resolved = local_target(page, target, root)
            if resolved is not None:
                graph[page].add(resolved)
        for source in meta.get("sources", []):
            resolved = local_target(page, source["resource"], root)
            sources.append(
                {
                    "page": name,
                    **source,
                    "local": resolved is not None,
                    "exists": resolved.exists() if resolved is not None else None,
                }
            )
    reached: set[Path] = set()
    pending = [root / "index.md"]
    while pending:
        page = pending.pop()
        if page in reached:
            continue
        reached.add(page)
        pending.extend(graph.get(page, set()) - reached)
    return {
        "page_count": len(pages),
        "markdown": links,
        "sources": sources,
        "missing_markdown": [row for row in links if row["exists"] is False],
        "missing_sources": [row for row in sources if row["exists"] is False],
        "unreachable_pages": [
            str(page.relative_to(root)) for page in pages if page not in reached
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("docs/knowledge"))
    args = parser.parse_args()
    result = inventory(args.root)
    sys.stdout.write(json.dumps(result, indent=2, default=str) + "\n")
    return int(
        bool(
            result["missing_markdown"]
            or result["missing_sources"]
            or result["unreachable_pages"]
        )
    )


if __name__ == "__main__":
    raise SystemExit(main())
