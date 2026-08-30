from dataclasses import dataclass, field, fields, is_dataclass
from datetime import datetime, timezone
from enum import StrEnum
from typing import cast


SCHEMA_VERSION = 2
RANKING_RULE_VERSION = 1


class RunState(StrEnum):
    RUNNING = "running"
    COMPLETED = "completed"
    BUDGET_EXHAUSTED = "budget_exhausted"
    BASELINE_FAILED = "baseline_failed"
    TOOL_UNAVAILABLE = "tool_unavailable"
    COMMAND_FAILED = "command_failed"
    INTERRUPTED = "interrupted"
    REPORT_FAILED = "report_failed"
    DISK_LIMIT = "disk_limit"


class CandidateState(StrEnum):
    PENDING = "pending"
    KILLED = "killed"
    SURVIVED = "survived"
    TIMEOUT = "timeout"
    UNVIABLE = "unviable"
    NOT_RUN = "not_run"
    ERROR = "error"


@dataclass
class RankingReason:
    code: str
    score: int
    detail: str


@dataclass
class Candidate:
    path: str
    symbol: str
    mutant_name: str | None
    score: int = 0
    reasons: list[RankingReason] = field(default_factory=list)
    state: CandidateState = CandidateState.PENDING
    not_run_reason: str | None = None
    command_sequences: list[int] = field(default_factory=list)
    manual_classification: str | None = None
    diagnostic: str | None = None
    diagnostic_observed_bytes: int = 0
    diagnostic_retained_bytes: int = 0
    diagnostic_truncated: bool = False


@dataclass
class CommandRecord:
    sequence: int
    label: str
    argv: list[str]
    cwd: str
    started_at: str
    ended_at: str | None = None
    elapsed_seconds: float | None = None
    exit_code: int | None = None
    timed_out: bool = False
    interrupted: bool = False
    stdout_path: str = ""
    stderr_path: str = ""
    cleanup_errors: list[str] = field(default_factory=list)
    stdout_observed_bytes: int = 0
    stdout_retained_bytes: int = 0
    stdout_truncated: bool = False
    stderr_observed_bytes: int = 0
    stderr_retained_bytes: int = 0
    stderr_truncated: bool = False
    disk_stop_code: str | None = None
    _spool: object | None = field(default=None, repr=False, compare=False)


@dataclass
class RunRecord:
    schema_version: int
    ranking_rule_version: int
    state: RunState
    total_budget_seconds: float
    repository: dict[str, object]
    tools: dict[str, str]
    candidates: list[Candidate]
    commands: list[CommandRecord]
    started_at: str
    ended_at: str | None
    elapsed_seconds: float | None
    comparison: dict[str, object] | None
    error: str | None
    report_error: str | None = None
    disk_policy: dict[str, object] = field(default_factory=dict)
    disk_observations: list[dict[str, object]] = field(default_factory=list)
    disk_summary: dict[str, object] = field(default_factory=dict)
    disk_stop: dict[str, object] | None = None
    secondary_errors: list[dict[str, object]] = field(default_factory=list)
    disk_enforcement: list[str] = field(default_factory=list)
    stale_cleanup: list[dict[str, object]] = field(default_factory=list)
    stale_cleanup_omitted_count: int = 0
    stale_cleanup_diagnostics: list[str] = field(default_factory=list)
    stale_cleanup_diagnostics_omitted_count: int = 0
    scratch: dict[str, object] | None = None
    cleanup: dict[str, object] | None = None
    output_recovery: dict[str, object] = field(default_factory=dict)
    jobs: int = 1

    @classmethod
    def new(cls, total_budget_seconds: float) -> "RunRecord":
        return cls(
            schema_version=SCHEMA_VERSION,
            ranking_rule_version=RANKING_RULE_VERSION,
            state=RunState.RUNNING,
            total_budget_seconds=total_budget_seconds,
            repository={},
            tools={},
            candidates=[],
            commands=[],
            started_at=datetime.now(timezone.utc).isoformat(),
            ended_at=None,
            elapsed_seconds=None,
            comparison=None,
            error=None,
        )

    def to_dict(self) -> dict[str, object]:
        def encode(value: object) -> object:
            if isinstance(value, StrEnum):
                return value.value
            if is_dataclass(value) and not isinstance(value, type):
                return {
                    item.name: encode(getattr(value, item.name))
                    for item in fields(value)
                    if not item.name.startswith("_")
                }
            if isinstance(value, list):
                return [encode(item) for item in value]
            if isinstance(value, dict):
                return {str(key): encode(item) for key, item in value.items()}
            return value

        return cast(dict[str, object], encode(self))
