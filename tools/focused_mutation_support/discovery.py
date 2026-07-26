from dataclasses import dataclass
from pathlib import Path, PurePosixPath
import re
from typing import Protocol, Sequence

from .model import Candidate


_EXCLUDED_PARTS = frozenset({"tests", "target", ".worktrees", ".idea"})
_FUNCTION = re.compile(
    r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:(?:async|const|unsafe|extern(?:\s+\"[^\"]+\")?)\s+)*"
    r"fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>{}]*>)?\s*\("
)


@dataclass(frozen=True)
class RepositorySnapshot:
    root: Path
    head: str
    branch: str
    dirty_paths: tuple[str, ...]
    base_paths: tuple[str, ...]
    recent_paths: tuple[str, ...]


class CommandProbe(Protocol):
    def text(self, argv: list[str]) -> str:
        raise NotImplementedError


def _normalize_path(value: str) -> str:
    normalized = value.replace("\\", "/")
    path = PurePosixPath(normalized)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"path must be repository-relative: {value}")
    if not path.parts or normalized in {"", "."}:
        raise ValueError("path must name a repository file")
    return path.as_posix()


def _eligible_path(value: str) -> str | None:
    path = _normalize_path(value)
    parts = PurePosixPath(path).parts
    if not path.endswith(".rs") or any(part in _EXCLUDED_PARTS for part in parts):
        return None
    return path


def _unique_eligible(paths: Sequence[str]) -> tuple[str, ...]:
    result: list[str] = []
    seen: set[str] = set()
    for value in paths:
        path = _eligible_path(value)
        if path is not None and path not in seen:
            seen.add(path)
            result.append(path)
    return tuple(result)


def _status_paths(text: str) -> tuple[str, ...]:
    entries = text.split("\0")
    result: list[str] = []
    index = 0
    while index < len(entries):
        entry = entries[index]
        index += 1
        if not entry:
            continue
        if len(entry) < 4 or entry[2] != " ":
            raise ValueError("invalid git status --porcelain=v1 -z output")
        status = entry[:2]
        path = entry[3:]
        if "R" in status or "C" in status:
            index += 1
        if "D" not in status:
            result.append(path)
    return _unique_eligible(result)


def _nul_paths(text: str) -> tuple[str, ...]:
    return _unique_eligible(tuple(item for item in text.split("\0") if item))


def _line_paths(text: str) -> tuple[str, ...]:
    return _unique_eligible(tuple(text.splitlines()))


def discover_repository(
    repository: Path, base: str, probe: CommandProbe
) -> RepositorySnapshot:
    root_text = probe.text(["git", "rev-parse", "--show-toplevel"]).strip()
    root = Path(root_text)
    expected_root = repository.resolve()
    if not root.is_absolute() or root.resolve() != expected_root:
        raise ValueError(
            f"git repository root {root} does not match requested root {repository}"
        )
    head = probe.text(["git", "rev-parse", "HEAD"]).strip()
    branch = probe.text(["git", "branch", "--show-current"]).strip()
    status = probe.text(
        [
            "git",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ]
    )
    changed = probe.text(
        [
            "git",
            "diff",
            "--name-only",
            "-z",
            f"{base}...HEAD",
            "--",
            "*.rs",
        ]
    )
    history = probe.text(
        ["git", "log", "--first-parent", "-20", "--name-only", "--format="]
    )
    return RepositorySnapshot(
        root=root,
        head=head,
        branch=branch,
        dirty_paths=_status_paths(status),
        base_paths=_nul_paths(changed),
        recent_paths=_line_paths(history),
    )


def discover_candidates(
    snapshot: RepositorySnapshot,
    explicit_files: Sequence[str],
    explicit_symbols: Sequence[str],
    probe: CommandProbe,
) -> list[Candidate]:
    del explicit_symbols, probe
    seen_paths: set[str] = set()
    candidates: list[Candidate] = []
    seen_candidates: set[tuple[str, str]] = set()

    def add_path(value: str) -> None:
        path = _eligible_path(value)
        if path is None or path in seen_paths:
            return
        seen_paths.add(path)
        source_path = snapshot.root / path
        try:
            source = source_path.read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            return
        for symbol in _FUNCTION.findall(source):
            key = (path, symbol)
            if key not in seen_candidates:
                seen_candidates.add(key)
                candidates.append(Candidate(path, symbol, None))

    for values in (explicit_files, snapshot.dirty_paths, snapshot.base_paths):
        for value in values:
            add_path(value)
    if len(candidates) < 10:
        for value in snapshot.recent_paths:
            add_path(value)
            if len(candidates) >= 10:
                break
    return candidates
