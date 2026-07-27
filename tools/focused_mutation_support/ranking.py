from collections.abc import Iterable, Sequence

from .discovery import RepositorySnapshot, _normalize_path
from .model import Candidate, RankingReason


SCORES = {
    "explicit_symbol": 1_000,
    "explicit_file": 900,
    "dirty_worktree": 500,
    "changed_since_base": 400,
    "recent_change": 100,
    "risk_state_transition": 80,
    "risk_cancellation": 80,
    "risk_timeout": 70,
    "risk_resource": 70,
    "risk_filesystem": 60,
    "risk_session_resume": 60,
    "risk_report_completion": 60,
    "conditional_or_error_path": 30,
}

_RISK_KEYWORDS = {
    "risk_state_transition": ("state", "transition"),
    "risk_cancellation": ("cancel", "cancellation"),
    "risk_timeout": ("timeout", "deadline"),
    "risk_resource": ("resource", "limit", "budget", "capacity"),
    "risk_filesystem": ("filesystem", "file_system", "path", "directory"),
    "risk_session_resume": ("session", "resume", "iterate"),
    "risk_report_completion": ("report", "completion", "complete"),
    "conditional_or_error_path": (
        "if ",
        "match ",
        "condition",
        "error",
        "err",
        "result",
    ),
}


def _reason(code: str) -> RankingReason:
    return RankingReason(
        code=code,
        score=SCORES[code],
        detail=code.replace("_", " "),
    )


def rank_candidates(
    candidates: Iterable[Candidate],
    snapshot: RepositorySnapshot,
    explicit_files: Sequence[str] = (),
    explicit_symbols: Sequence[str] = (),
) -> list[Candidate]:
    explicit_path_set = {_normalize_path(path) for path in explicit_files}
    explicit_symbol_set = {symbol.casefold() for symbol in explicit_symbols}
    dirty = set(snapshot.dirty_paths)
    base = set(snapshot.base_paths)
    recent = set(snapshot.recent_paths)
    ranked: list[Candidate] = []

    for candidate in candidates:
        candidate.path = _normalize_path(candidate.path)
        reasons: list[RankingReason] = []
        if candidate.symbol.casefold() in explicit_symbol_set:
            reasons.append(_reason("explicit_symbol"))
        if candidate.path in explicit_path_set:
            reasons.append(_reason("explicit_file"))
        if candidate.path in dirty:
            reasons.append(_reason("dirty_worktree"))
        if candidate.path in base:
            reasons.append(_reason("changed_since_base"))
        if candidate.path in recent:
            reasons.append(_reason("recent_change"))

        searchable = " ".join(
            (
                candidate.path,
                candidate.symbol,
                candidate.mutant_name or "",
            )
        ).casefold()
        for code, keywords in _RISK_KEYWORDS.items():
            if any(keyword in searchable for keyword in keywords):
                reasons.append(_reason(code))

        reasons.sort(key=lambda item: (-item.score, item.code))
        candidate.reasons = reasons
        candidate.score = sum(item.score for item in reasons)
        ranked.append(candidate)

    return sorted(
        ranked,
        key=lambda item: (
            -item.score,
            item.path,
            item.symbol,
            item.mutant_name or "",
        ),
    )
