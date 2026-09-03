from io import StringIO
import shlex
from typing import Protocol

from .model import CandidateState, RunRecord


def _code(value: object) -> str:
    return f"`{str(value).replace('`', '&#96;')}`"


class TextWriter(Protocol):
    def write(self, value: str, /) -> int: ...


def write_markdown(record: RunRecord, writer: TextWriter) -> None:
    elapsed = record.elapsed_seconds if record.elapsed_seconds is not None else 0.0
    head = record.repository.get("head", "unknown")
    dirty = "yes" if record.repository.get("dirty", False) else "no"
    conclusive_states = {
        CandidateState.KILLED,
        CandidateState.SURVIVED,
    }
    verified = [
        candidate
        for candidate in record.candidates
        if candidate.state in conclusive_states
    ]
    investigation = [
        candidate
        for candidate in record.candidates
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
    def emit(value: str = "") -> None:
        writer.write(value)
        writer.write("\n")

    emit("# Focused mutation report")
    emit()
    emit(f"- State: {_code(record.state.value)}")
    emit(f"- Elapsed: `{elapsed:.1f}s` of `{record.total_budget_seconds:.1f}s`")
    emit(f"- Commit: {_code(head)}")
    emit(f"- Dirty worktree: `{dirty}`")
    emit(f"- Unverified candidates: `{len(unverified)}`")
    emit()
    emit("## Verified candidates")
    for item in verified:
        emit(f"- {_code(item.symbol)} — {item.state.value} — {_code(item.path)}")
    if not verified:
        emit("- none")
    emit()
    emit("## Investigation results")
    for item in investigation:
        emit(
            f"- {_code(item.symbol)} — {item.state.value} — "
            "inspect the recorded command artifacts"
        )
    if not investigation:
        emit("- none")
    emit()
    emit("## Unverified candidates")
    for item in unverified:
        emit(
            f"- {_code(item.symbol)} — "
            f"{item.not_run_reason or item.state.value}"
        )
    if not unverified:
        emit("- none")
    emit()
    emit("## Next recommended order")
    for index, item in enumerate(unverified, 1):
        reasons = ", ".join(reason.code for reason in item.reasons) or "none"
        emit(
            f"{index}. {_code(item.symbol)} — score `{item.score}` — {_code(reasons)}"
        )
    if not unverified:
        emit("1. none")
    emit()
    emit("## Manual classification")
    for item in record.candidates:
        emit(
            f"- {_code(item.symbol)} — "
            f"{_code(item.manual_classification or 'unclassified')}"
        )
    if not record.candidates:
        emit("- none")
    emit()
    emit("## Full-inventory comparison")
    if record.comparison is None:
        emit("`reduction ratio not measured`")
    else:
        emit(
            f"`{record.comparison.get('focused_candidates', 0)} focused of "
            f"{record.comparison.get('full_candidates', 0)} full candidates; "
            f"reduction ratio {record.comparison.get('reduction_ratio', 0):.3f}`"
        )
    emit()
    emit("## Command cleanup failures")
    cleanup_failure_count = 0
    for command in record.commands:
        exit_status = (
            "unknown" if command.exit_code is None else str(command.exit_code)
        )
        for error in command.cleanup_errors:
            cleanup_failure_count += 1
            emit(
                f"- {_code(command.label)} — exit {_code(exit_status)} — "
                f"{_code(error)}"
            )
    if cleanup_failure_count == 0:
        emit("- none")
    emit()
    emit("## Disk safety")
    stop = record.disk_stop or {}
    emit(f"- Stop code: {_code(stop.get('code', 'none'))}")
    emit(f"- Stop reason: {_code(stop.get('reason', 'none'))}")
    emit(f"- Samples: {_code(record.disk_summary.get('sample_count', 0))}")
    emit(
        "- Peak owned bytes: "
        f"{_code(record.disk_summary.get('peak_owned_bytes', 0))}"
    )
    emit(
        "- Minimum free bytes: "
        f"{_code(record.disk_summary.get('minimum_free_bytes', 'unknown'))}"
    )
    emit("- Secondary errors:")
    for secondary_item in record.secondary_errors:
        secondary_message = secondary_item.get("message")
        if secondary_message is None:
            secondary_message = secondary_item.get("reason", "")
        emit(
            f"  - {_code(secondary_item.get('kind', 'unknown'))} / "
            f"{_code(secondary_item.get('code', 'unknown'))}: "
            f"{_code(secondary_message)}"
        )
    if not record.secondary_errors:
        emit("  - none")
    emit()
    emit("## Scratch cleanup")
    cleanup = record.cleanup or {}
    emit(f"- Status: {_code(cleanup.get('status', 'unknown'))}")
    emit(
        "- Entries: examined "
        f"{_code(cleanup.get('examined_entries', 0))}, removed "
        f"{_code(cleanup.get('removed_entries', 0))}"
    )
    emit(
        "- Omitted cleanup details: "
        f"{_code(cleanup.get('omitted_detail_count', 0))}"
    )
    remaining = cleanup.get("remaining_root")
    if cleanup.get("status") == "retained" and isinstance(remaining, str):
        emit(f"- Manual removal: `rm -rf -- {shlex.quote(remaining)}`")
    else:
        emit("- Manual removal: `not required`")
    emit()


def render_markdown(record: RunRecord) -> str:
    """Compatibility collector for tests; production streams with write_markdown."""
    output = StringIO()
    write_markdown(record, output)
    return output.getvalue().removesuffix("\n")
