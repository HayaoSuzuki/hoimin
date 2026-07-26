from dataclasses import dataclass, field, fields, is_dataclass
from datetime import datetime, timezone
from enum import StrEnum
from typing import cast


SCHEMA_VERSION = 1
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
                }
            if isinstance(value, list):
                return [encode(item) for item in value]
            if isinstance(value, dict):
                return {str(key): encode(item) for key, item in value.items()}
            return value

        return cast(dict[str, object], encode(self))
