from .model import CandidateState, RunRecord


def _code(value: object) -> str:
    return f"`{str(value).replace('`', '&#96;')}`"


def render_markdown(record: RunRecord) -> str:
    elapsed = record.elapsed_seconds if record.elapsed_seconds is not None else 0.0
    head = record.repository.get("head", "unknown")
    dirty = "yes" if record.repository.get("dirty", False) else "no"
    conclusive_states = {
        CandidateState.KILLED,
        CandidateState.SURVIVED,
        CandidateState.UNVIABLE,
    }
    verified = [
        candidate
        for candidate in record.candidates
        if candidate.state in conclusive_states
    ]
    investigation = [
        candidate
        for candidate in verified
        if candidate.state
        in {
            CandidateState.SURVIVED,
            CandidateState.TIMEOUT,
            CandidateState.UNVIABLE,
            CandidateState.ERROR,
        }
    ]
    unverified = [
        candidate
        for candidate in record.candidates
        if candidate.state not in conclusive_states
    ]
    lines = [
        "# Focused mutation report",
        "",
        f"- State: {_code(record.state.value)}",
        f"- Elapsed: `{elapsed:.1f}s` of `{record.total_budget_seconds:.1f}s`",
        f"- Commit: {_code(head)}",
        f"- Dirty worktree: `{dirty}`",
        "",
        "## Verified candidates",
    ]
    lines.extend(
        f"- {_code(item.symbol)} — {item.state.value} — {_code(item.path)}"
        for item in verified
    )
    if not verified:
        lines.append("- none")
    lines.extend(["", "## Investigation results"])
    lines.extend(
        f"- {_code(item.symbol)} — {item.state.value} — inspect the recorded command artifacts"
        for item in investigation
    )
    if not investigation:
        lines.append("- none")
    lines.extend(["", "## Unverified candidates"])
    lines.extend(
        f"- {_code(item.symbol)} — "
        f"{item.not_run_reason or item.state.value}"
        for item in unverified
    )
    if not unverified:
        lines.append("- none")
    lines.extend(["", "## Next recommended order"])
    for index, item in enumerate(unverified, 1):
        reasons = ", ".join(reason.code for reason in item.reasons) or "none"
        lines.append(
            f"{index}. {_code(item.symbol)} — score `{item.score}` — {_code(reasons)}"
        )
    if not unverified:
        lines.append("1. none")
    lines.extend(["", "## Manual classification"])
    lines.extend(
        f"- {_code(item.symbol)} — {_code(item.manual_classification or 'unclassified')}"
        for item in record.candidates
    )
    if not record.candidates:
        lines.append("- none")
    lines.extend(["", "## Full-inventory comparison"])
    if record.comparison is None:
        lines.append("`reduction ratio not measured`")
    else:
        lines.append(
            f"`{record.comparison.get('focused_candidates', 0)} focused of "
            f"{record.comparison.get('full_candidates', 0)} full candidates; "
            f"reduction ratio {record.comparison.get('reduction_ratio', 0):.3f}`"
        )
    lines.append("")
    return "\n".join(lines)
