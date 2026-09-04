from __future__ import annotations

from dataclasses import dataclass, field, replace
import errno
from enum import StrEnum
from pathlib import Path
import re
import tempfile
import threading
import time
from collections.abc import Callable, Iterable

from .filesystem import (
    CreateDisposition,
    DirectoryCapability,
    DirectoryIterator,
    EntryKind,
    FileAccess,
    FileCapability,
    FileIdentity,
    FilesystemBackend,
    FilesystemIdentity,
    SharePolicy,
    default_filesystem_backend,
)


MIB = 1024**2
GIB = 1024**3

MAX_DISK_BYTES = 8 * GIB
MIN_FREE_BYTES = 10 * GIB
DEFAULT_JOBS = 1
MAX_JOBS = 4
MAX_LOG_BYTES = 16 * MIB
MAX_COMMAND_LOG_BYTES = 64 * MIB
MAX_TOOL_JSON_BYTES = 8 * MIB
MAX_SELECTOR_COUNT = 1_000
MAX_SELECTOR_BYTES = 16 * 1024
MAX_CANDIDATE_DIAGNOSTIC_BYTES = 16 * 1024
MAX_RUN_DIAGNOSTIC_BYTES = 16 * MIB
MAX_REPORT_BYTES = 32 * MIB
MAX_REPORTED_PATH_BYTES = 16 * 1024
SAMPLE_INTERVAL_SECONDS = 0.250

MAX_TREE_ENTRIES = 250_000
MAX_TREE_DEPTH = 128
MAX_OPEN_DIRECTORIES = 129
MAX_SCAN_SECONDS = 5.0
MAX_CLEANUP_SLICE_ENTRIES = 50_000
MAX_CLEANUP_SLICE_SECONDS = 5.0
MAX_CLEANUP_DEPTH = 4_096
MAX_CLEANUP_CURSOR_BYTES = 64 * 1024
MAX_CLEANUP_OPEN_DIRECTORIES = 3
OWNER_CLEANUP_SECONDS = 60.0
JANITOR_CLEANUP_SECONDS = 30.0
JANITOR_SELECTION_SECONDS = 5.0
MAX_RECLAIM_CANDIDATES = 256
MAX_DIAGNOSTIC_DETAILS = 256
MAX_DIAGNOSTIC_DETAIL_BYTES = 4 * 1024
MAX_DISK_OBSERVATIONS = 256

_BYTE_SIZE = re.compile(r"^(?P<count>[0-9]+)(?P<unit>KiB|MiB|GiB)?$")


def parse_byte_size(value: str) -> int:
    match = _BYTE_SIZE.fullmatch(value)
    if match is None:
        raise ValueError(
            f"invalid byte size {value!r}; use a positive integer with KiB, MiB, or GiB"
        )
    count = int(match.group("count"))
    if count <= 0:
        raise ValueError("byte size must be positive")
    multiplier = {None: 1, "KiB": 1024, "MiB": MIB, "GiB": GIB}[
        match.group("unit")
    ]
    return count * multiplier


def _bounded_secondary(value: str) -> str:
    return (
        value.encode("utf-8", errors="backslashreplace")[:512]
        .decode("utf-8", errors="ignore")
    )


def _add_close_note(primary: BaseException, error: BaseException) -> None:
    primary.add_note(
        _bounded_secondary(f"filesystem capability close failed: {error}")
    )


def _add_deadline_note(primary: BaseException, error: BaseException) -> None:
    primary.add_note(
        _bounded_secondary(f"post-close deadline check failed: {error}")
    )


def canonical_scratch_root(
    value: Path | None,
    *,
    backend: FilesystemBackend | None = None,
) -> Path:
    root = Path(tempfile.gettempdir()) if value is None else value
    try:
        canonical = root.resolve(strict=True)
    except OSError as error:
        raise ValueError(f"scratch root cannot be canonicalized: {root}: {error}") from error
    selected = default_filesystem_backend() if backend is None else backend
    capability: DirectoryCapability | None = None
    primary: BaseException | None = None
    try:
        capability = selected.open_root(canonical, SharePolicy.SCAN)
        if capability.kind is not EntryKind.DIRECTORY:
            raise NotADirectoryError(canonical)
        selected.available_bytes(capability)
    except BaseException as error:
        if isinstance(error, NotADirectoryError):
            primary = ValueError(f"scratch root is not a directory: {canonical}")
            primary.__cause__ = error
        elif isinstance(error, Exception):
            primary = ValueError(
                f"scratch root free space cannot be queried: {canonical}: {error}"
            )
            primary.__cause__ = error
        else:
            primary = error
    finally:
        if capability is not None:
            try:
                capability.close()
            except BaseException as close_error:
                if primary is None:
                    primary = ValueError(
                        "scratch root free space cannot be queried: "
                        f"{canonical}: {close_error}"
                    )
                    primary.__cause__ = close_error
                else:
                    _add_close_note(primary, close_error)
                if capability.is_open:
                    try:
                        capability.close()
                    except BaseException as retry_error:
                        assert primary is not None
                        _add_close_note(primary, retry_error)
    if primary is not None:
        raise primary
    return canonical


@dataclass(frozen=True)
class DiskPolicy:
    max_disk_bytes: int = MAX_DISK_BYTES
    min_free_bytes: int = MIN_FREE_BYTES
    jobs: int = DEFAULT_JOBS
    max_log_bytes: int = MAX_LOG_BYTES
    scratch_root: Path = field(default_factory=lambda: Path(tempfile.gettempdir()))
    keep_scratch: bool = False
    max_command_log_bytes: int = MAX_COMMAND_LOG_BYTES
    max_tool_json_bytes: int = MAX_TOOL_JSON_BYTES
    max_selector_count: int = MAX_SELECTOR_COUNT
    max_selector_bytes: int = MAX_SELECTOR_BYTES
    max_candidate_diagnostic_bytes: int = MAX_CANDIDATE_DIAGNOSTIC_BYTES
    max_run_diagnostic_bytes: int = MAX_RUN_DIAGNOSTIC_BYTES
    max_report_bytes: int = MAX_REPORT_BYTES
    max_reported_path_bytes: int = MAX_REPORTED_PATH_BYTES
    sample_interval_seconds: float = SAMPLE_INTERVAL_SECONDS

    def __post_init__(self) -> None:
        if self.max_disk_bytes <= 0:
            raise ValueError("max disk must be positive")
        if self.min_free_bytes <= 0:
            raise ValueError("minimum free space must be positive")
        if not 1 <= self.jobs <= MAX_JOBS:
            raise ValueError(f"jobs must be in 1..={MAX_JOBS}")
        if not 1 <= self.max_log_bytes <= MAX_COMMAND_LOG_BYTES:
            raise ValueError(
                f"max log size must be in 1..={MAX_COMMAND_LOG_BYTES} bytes"
            )
        object.__setattr__(
            self, "scratch_root", canonical_scratch_root(self.scratch_root)
        )


WORKSPACE_SIZE_EXCEEDED = "workspace.size.exceeded"
FILESYSTEM_RESERVE_REACHED = "filesystem.reserve.reached"
DISK_MEASUREMENT_FAILED = "disk.measurement.failed"
WORKSPACE_CLEANUP_FAILED = "workspace.cleanup.failed"
WORKSPACE_CLEANUP_DEFERRED = "workspace.cleanup.deferred"
PROCESS_LIFECYCLE_FAILED = "process.failed"


@dataclass(frozen=True)
class DiskObservation:
    owned_bytes: int
    available_bytes: int
    measured_in_seconds: float = 0.0
    conservative_entries: int = 0
    filesystem_available_bytes: dict[str, int] = field(default_factory=dict)
    root_owned_bytes: dict[str, int] = field(default_factory=dict)
    identity_bytes: dict[tuple[int, int], int] = field(
        default_factory=dict, repr=False, compare=False
    )
    root_owned_identities: dict[str, frozenset[tuple[int, int]]] = field(
        default_factory=dict, repr=False, compare=False
    )

    def __post_init__(self) -> None:
        if (
            self.owned_bytes < 0
            or self.available_bytes < 0
            or self.conservative_entries < 0
            or any(value < 0 for value in self.root_owned_bytes.values())
            or any(value < 0 for value in self.identity_bytes.values())
        ):
            raise ValueError("disk observations cannot be negative")


class DiskStopReason(StrEnum):
    WORKSPACE_SIZE_EXCEEDED = "workspace_size_exceeded"
    FILESYSTEM_RESERVE_REACHED = "filesystem_reserve_reached"
    MEASUREMENT_FAILED = "measurement_failed"
    PROCESS_FAILED = "process_failed"

    @property
    def code(self) -> str:
        return {
            self.WORKSPACE_SIZE_EXCEEDED: WORKSPACE_SIZE_EXCEEDED,
            self.FILESYSTEM_RESERVE_REACHED: FILESYSTEM_RESERVE_REACHED,
            self.MEASUREMENT_FAILED: DISK_MEASUREMENT_FAILED,
            self.PROCESS_FAILED: PROCESS_LIFECYCLE_FAILED,
        }[self]


@dataclass(frozen=True)
class DiskSecondary:
    reason: DiskStopReason | None = None
    observation: DiskObservation | None = None
    code: str | None = None
    message: str | None = None


@dataclass(frozen=True)
class DiskFailure:
    code: str
    reason: DiskStopReason
    observation: DiskObservation | None = None
    message: str | None = None
    secondary: tuple[DiskSecondary, ...] = ()


def evaluate_disk_policy(
    policy: DiskPolicy, observation: DiskObservation
) -> DiskFailure | None:
    reserve = observation.available_bytes <= policy.min_free_bytes
    size = observation.owned_bytes >= policy.max_disk_bytes
    if not reserve and not size:
        return None
    reason = (
        DiskStopReason.FILESYSTEM_RESERVE_REACHED
        if reserve
        else DiskStopReason.WORKSPACE_SIZE_EXCEEDED
    )
    secondary = (
        (
            DiskSecondary(
                reason=DiskStopReason.WORKSPACE_SIZE_EXCEEDED,
                observation=observation,
            ),
        )
        if reserve and size
        else ()
    )
    return DiskFailure(
        code=reason.code,
        reason=reason,
        observation=observation,
        secondary=secondary,
    )


class DiskRootId(StrEnum):
    EXECUTION = "execution"
    DELIVERY = "delivery"


class CleanupOutcome(StrEnum):
    CLEAN = "clean"
    FAILED = "failed"
    DEFERRED = "deferred"
    RETAINED = "retained"


class ComponentState(StrEnum):
    PENDING = "pending"
    SUCCEEDED = "succeeded"
    FAILED = "failed"


class EventKind(StrEnum):
    DISPATCH_REQUESTED = "dispatch_requested"
    OBSERVATION = "observation"
    MEASUREMENT_FAILED = "measurement_failed"
    PROCESS_DRAIN_SUCCEEDED = "process_drain_succeeded"
    PROCESS_DRAIN_FAILED = "process_drain_failed"
    OUTPUT_DRAIN_SUCCEEDED = "output_drain_succeeded"
    OUTPUT_DRAIN_FAILED = "output_drain_failed"
    MONITOR_JOIN_SUCCEEDED = "monitor_join_succeeded"
    MONITOR_JOIN_FAILED = "monitor_join_failed"
    REPORT_SUCCEEDED = "report_succeeded"
    REPORT_FAILED = "report_failed"
    CLEANUP_REQUESTED = "cleanup_requested"
    CLEANUP_COMPLETED = "cleanup_completed"
    FINISH_REQUESTED = "finish_requested"


@dataclass(frozen=True)
class DiskLifecycleEvent:
    kind: EventKind
    root: DiskRootId | None = None
    outcome: CleanupOutcome | None = None
    message: str | None = None
    policy: DiskPolicy | None = None
    value: DiskObservation | None = None

    @classmethod
    def dispatch_requested(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.DISPATCH_REQUESTED)

    @classmethod
    def observation(
        cls, policy: DiskPolicy, observation: DiskObservation
    ) -> "DiskLifecycleEvent":
        return cls(
            EventKind.OBSERVATION,
            policy=policy,
            value=observation,
        )

    @classmethod
    def measurement_failed(cls, message: str | None) -> "DiskLifecycleEvent":
        return cls(EventKind.MEASUREMENT_FAILED, message=message)

    @classmethod
    def process_drain_succeeded(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.PROCESS_DRAIN_SUCCEEDED)

    @classmethod
    def process_drain_failed(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.PROCESS_DRAIN_FAILED)

    @classmethod
    def output_drain_succeeded(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.OUTPUT_DRAIN_SUCCEEDED)

    @classmethod
    def output_drain_failed(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.OUTPUT_DRAIN_FAILED)

    @classmethod
    def monitor_join_succeeded(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.MONITOR_JOIN_SUCCEEDED)

    @classmethod
    def monitor_join_failed(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.MONITOR_JOIN_FAILED)

    @classmethod
    def report_succeeded(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.REPORT_SUCCEEDED)

    @classmethod
    def report_failed(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.REPORT_FAILED)

    @classmethod
    def finish_requested(cls) -> "DiskLifecycleEvent":
        return cls(EventKind.FINISH_REQUESTED)

    @classmethod
    def cleanup_requested(cls, root: DiskRootId) -> "DiskLifecycleEvent":
        return cls(EventKind.CLEANUP_REQUESTED, root=root)

    @classmethod
    def cleanup_completed(
        cls,
        root: DiskRootId,
        outcome: CleanupOutcome,
        message: str | None = None,
    ) -> "DiskLifecycleEvent":
        return cls(
            EventKind.CLEANUP_COMPLETED,
            root=root,
            outcome=outcome,
            message=message,
        )


class DiskLifecycle:
    def __init__(self, owned_roots: list[DiskRootId]) -> None:
        if len(set(owned_roots)) != len(owned_roots):
            raise ValueError("duplicate disk root")
        self.owned_roots = list(owned_roots)
        self.delivery_roots = [
            root for root in owned_roots if root is DiskRootId.DELIVERY
        ]
        self.stop: DiskFailure | None = None
        self.secondary: list[DiskSecondary] = []
        self.active = 0
        self.dispatched = 0
        self.cleanup_requested: list[DiskRootId] = []
        self.cleanup_clean: list[DiskRootId] = []
        self.cleanup_failed: list[DiskRootId] = []
        self.cleanup_deferred: list[DiskRootId] = []
        self.cleanup_retained: list[DiskRootId] = []
        self.process_drain = ComponentState.PENDING
        self.output_drain = ComponentState.PENDING
        self.monitor_join = ComponentState.PENDING
        self.report = ComponentState.PENDING
        self.finished = False


@dataclass(frozen=True)
class DiskLifecycleSnapshot:
    stop: DiskFailure | None
    secondary: tuple[DiskSecondary, ...]
    active: int
    dispatched: int
    owned_roots: tuple[DiskRootId, ...]
    delivery_roots: tuple[DiskRootId, ...]
    cleanup_requested: tuple[DiskRootId, ...]
    cleanup_clean: tuple[DiskRootId, ...]
    cleanup_failed: tuple[DiskRootId, ...]
    cleanup_deferred: tuple[DiskRootId, ...]
    cleanup_retained: tuple[DiskRootId, ...]
    process_drain: ComponentState
    output_drain: ComponentState
    monitor_join: ComponentState
    report: ComponentState
    finished: bool


@dataclass(frozen=True)
class DiskLifecycleTraceResult:
    snapshot: DiskLifecycleSnapshot
    accepted: bool
    rejected_at: int | None


def _settled(state: ComponentState) -> bool:
    return state is not ComponentState.PENDING


def _safety_settled(lifecycle: DiskLifecycle) -> bool:
    return all(
        _settled(value)
        for value in (
            lifecycle.process_drain,
            lifecycle.output_drain,
            lifecycle.monitor_join,
        )
    )


def _safety_succeeded(lifecycle: DiskLifecycle) -> bool:
    return all(
        value is ComponentState.SUCCEEDED
        for value in (
            lifecycle.process_drain,
            lifecycle.output_drain,
            lifecycle.monitor_join,
        )
    )


def _record_secondary(
    lifecycle: DiskLifecycle, secondary: DiskSecondary
) -> None:
    if lifecycle.stop is not None and _failure_matches_secondary(
        lifecycle.stop, secondary
    ):
        return
    if lifecycle.stop is not None and secondary in lifecycle.stop.secondary:
        return
    if secondary in lifecycle.secondary:
        return
    lifecycle.secondary.append(secondary)


def _failure_matches_secondary(
    failure: DiskFailure, secondary: DiskSecondary
) -> bool:
    if secondary.observation is not None:
        return (
            failure.reason is secondary.reason
            and failure.observation == secondary.observation
        )
    return (
        failure.reason is secondary.reason
        and failure.code == secondary.code
        and failure.message == secondary.message
    )


def _record_disk_failure(
    lifecycle: DiskLifecycle, failure: DiskFailure
) -> None:
    if lifecycle.stop is None:
        lifecycle.stop = failure
        return
    _record_secondary(
        lifecycle,
        DiskSecondary(
            reason=failure.reason,
            observation=failure.observation,
            code=failure.code,
            message=failure.message,
        ),
    )
    for secondary in failure.secondary:
        _record_secondary(lifecycle, secondary)


def apply_disk_lifecycle_event(
    lifecycle: DiskLifecycle, event: DiskLifecycleEvent
) -> bool:
    if lifecycle.finished:
        return False
    kind = event.kind
    if kind is EventKind.DISPATCH_REQUESTED:
        if (
            lifecycle.stop is not None
            or lifecycle.process_drain is not ComponentState.PENDING
        ):
            return False
        lifecycle.active += 1
        lifecycle.dispatched += 1
        return True
    component_events = {
        EventKind.PROCESS_DRAIN_SUCCEEDED: ("process_drain", ComponentState.SUCCEEDED),
        EventKind.PROCESS_DRAIN_FAILED: ("process_drain", ComponentState.FAILED),
        EventKind.OUTPUT_DRAIN_SUCCEEDED: ("output_drain", ComponentState.SUCCEEDED),
        EventKind.OUTPUT_DRAIN_FAILED: ("output_drain", ComponentState.FAILED),
        EventKind.MONITOR_JOIN_SUCCEEDED: ("monitor_join", ComponentState.SUCCEEDED),
        EventKind.MONITOR_JOIN_FAILED: ("monitor_join", ComponentState.FAILED),
        EventKind.REPORT_SUCCEEDED: ("report", ComponentState.SUCCEEDED),
        EventKind.REPORT_FAILED: ("report", ComponentState.FAILED),
    }
    if kind in component_events:
        attribute, state = component_events[kind]
        if getattr(lifecycle, attribute) is not ComponentState.PENDING:
            return False
        setattr(lifecycle, attribute, state)
        if attribute == "process_drain":
            lifecycle.active = 0
            if state is ComponentState.FAILED:
                _record_disk_failure(
                    lifecycle,
                    DiskFailure(
                        code=PROCESS_LIFECYCLE_FAILED,
                        reason=DiskStopReason.PROCESS_FAILED,
                    ),
                )
        return True
    if kind is EventKind.OBSERVATION:
        if event.policy is None or event.value is None:
            return False
        failure = evaluate_disk_policy(event.policy, event.value)
        if failure is not None:
            _record_disk_failure(lifecycle, failure)
        return True
    if kind is EventKind.MEASUREMENT_FAILED:
        _record_disk_failure(
            lifecycle,
            DiskFailure(
                code=DISK_MEASUREMENT_FAILED,
                reason=DiskStopReason.MEASUREMENT_FAILED,
                message=event.message,
            ),
        )
        return True
    if kind is EventKind.CLEANUP_REQUESTED:
        root = event.root
        if (
            root not in lifecycle.owned_roots
            or root in lifecycle.cleanup_requested
            or lifecycle.active != 0
            or not _safety_settled(lifecycle)
            or (root in lifecycle.delivery_roots and lifecycle.report is ComponentState.PENDING)
        ):
            return False
        lifecycle.cleanup_requested.append(root)
        return True
    if kind is EventKind.CLEANUP_COMPLETED:
        root = event.root
        if root not in lifecycle.cleanup_requested or any(
            root in values
            for values in (
                lifecycle.cleanup_clean,
                lifecycle.cleanup_failed,
                lifecycle.cleanup_deferred,
                lifecycle.cleanup_retained,
            )
        ):
            return False
        if event.outcome in {CleanupOutcome.CLEAN, CleanupOutcome.FAILED} and not _safety_succeeded(lifecycle):
            return False
        if event.outcome is None:
            return False
        target = {
            CleanupOutcome.CLEAN: lifecycle.cleanup_clean,
            CleanupOutcome.FAILED: lifecycle.cleanup_failed,
            CleanupOutcome.DEFERRED: lifecycle.cleanup_deferred,
            CleanupOutcome.RETAINED: lifecycle.cleanup_retained,
        }.get(event.outcome)
        if target is None:
            return False
        target.append(root)
        if event.outcome is CleanupOutcome.FAILED:
            _record_secondary(
                lifecycle,
                DiskSecondary(code=WORKSPACE_CLEANUP_FAILED, message=event.message)
            )
        elif event.outcome is CleanupOutcome.DEFERRED:
            _record_secondary(
                lifecycle,
                DiskSecondary(code=WORKSPACE_CLEANUP_DEFERRED, message=event.message)
            )
        return True
    if kind is EventKind.FINISH_REQUESTED:
        terminal_roots = set(
            lifecycle.cleanup_clean
            + lifecycle.cleanup_failed
            + lifecycle.cleanup_retained
        )
        may_finish = (
            lifecycle.active == 0
            and _safety_settled(lifecycle)
            and not lifecycle.cleanup_deferred
            and lifecycle.report is ComponentState.SUCCEEDED
            and set(lifecycle.owned_roots) <= terminal_roots
            and set(lifecycle.delivery_roots) <= set(lifecycle.cleanup_clean)
        )
        if not may_finish:
            return False
        lifecycle.finished = True
        return True
    return False


def snapshot_disk_lifecycle(
    lifecycle: DiskLifecycle,
) -> DiskLifecycleSnapshot:
    return DiskLifecycleSnapshot(
        stop=lifecycle.stop,
        secondary=tuple(lifecycle.secondary),
        active=lifecycle.active,
        dispatched=lifecycle.dispatched,
        owned_roots=tuple(lifecycle.owned_roots),
        delivery_roots=tuple(lifecycle.delivery_roots),
        cleanup_requested=tuple(lifecycle.cleanup_requested),
        cleanup_clean=tuple(lifecycle.cleanup_clean),
        cleanup_failed=tuple(lifecycle.cleanup_failed),
        cleanup_deferred=tuple(lifecycle.cleanup_deferred),
        cleanup_retained=tuple(lifecycle.cleanup_retained),
        process_drain=lifecycle.process_drain,
        output_drain=lifecycle.output_drain,
        monitor_join=lifecycle.monitor_join,
        report=lifecycle.report,
        finished=lifecycle.finished,
    )


class DiskLifecycleTraceRunner:
    """Execute a fixed public lifecycle trace without fixture state injection."""

    def __init__(self, owned_roots: list[DiskRootId]) -> None:
        self._lifecycle = DiskLifecycle(owned_roots)

    def snapshot(self) -> DiskLifecycleSnapshot:
        return snapshot_disk_lifecycle(self._lifecycle)

    @property
    def stop(self) -> DiskFailure | None:
        return self._lifecycle.stop

    def apply(self, event: DiskLifecycleEvent) -> bool:
        return apply_disk_lifecycle_event(self._lifecycle, event)

    def record_disk_failure(
        self, policy: DiskPolicy, failure: DiskFailure
    ) -> bool:
        return self.apply(
            DiskLifecycleEvent.observation(policy, failure.observation)
            if failure.observation is not None
            else DiskLifecycleEvent.measurement_failed(failure.message)
        )

    def dispatch(self) -> bool:
        return self.apply(DiskLifecycleEvent.dispatch_requested())

    def record_process_drain(self, succeeded: bool) -> bool:
        return self.apply(
            DiskLifecycleEvent.process_drain_succeeded()
            if succeeded
            else DiskLifecycleEvent.process_drain_failed()
        )

    def record_output_drain(self, succeeded: bool) -> bool:
        return self.apply(
            DiskLifecycleEvent.output_drain_succeeded()
            if succeeded
            else DiskLifecycleEvent.output_drain_failed()
        )

    def record_monitor_join(self, succeeded: bool) -> bool:
        return self.apply(
            DiskLifecycleEvent.monitor_join_succeeded()
            if succeeded
            else DiskLifecycleEvent.monitor_join_failed()
        )

    def request_cleanup(self, root: DiskRootId) -> bool:
        return self.apply(DiskLifecycleEvent.cleanup_requested(root))

    def complete_cleanup(
        self,
        root: DiskRootId,
        outcome: CleanupOutcome,
        message: str | None = None,
    ) -> bool:
        return self.apply(
            DiskLifecycleEvent.cleanup_completed(root, outcome, message)
        )

    def record_report(self, succeeded: bool) -> bool:
        return self.apply(
            DiskLifecycleEvent.report_succeeded()
            if succeeded
            else DiskLifecycleEvent.report_failed()
        )

    def finish(self) -> bool:
        return self.apply(DiskLifecycleEvent.finish_requested())

    def run(
        self, events: Iterable[DiskLifecycleEvent]
    ) -> DiskLifecycleTraceResult:
        for index, event in enumerate(events):
            if not self.apply(event):
                return DiskLifecycleTraceResult(
                    snapshot=self.snapshot(),
                    accepted=False,
                    rejected_at=index,
                )
        return DiskLifecycleTraceResult(
            snapshot=self.snapshot(),
            accepted=True,
            rejected_at=None,
        )


@dataclass(frozen=True)
class MeterRoot:
    path: Path
    charge_owned_bytes: bool = True
    enforcement: str = "owned"
    exact_path: Path | None = None
    capability_factory: Callable[[], DirectoryCapability] | None = None


class DiskMeasurementError(RuntimeError):
    pass


@dataclass(slots=True)
class _RootCapabilityState:
    root: MeterRoot
    capability: DirectoryCapability | None
    acquisition_error: str | None
    retained_identity: FileIdentity | None
    exact_identity: FileIdentity | None


@dataclass(slots=True)
class _MeterFrame:
    iterator: DirectoryIterator
    path_parts: tuple[str, ...]


def _bounded_depth_message(path_parts: tuple[str, ...]) -> str:
    relative = "/".join(path_parts)
    bounded = (
        relative.encode("utf-8", errors="backslashreplace")[:768]
        .decode("utf-8", errors="ignore")
    )
    return f"owned scratch depth exceeds {MAX_TREE_DEPTH} below {bounded}"


def _validate_root_capability(
    capability: object,
    backend: FilesystemBackend,
    expected_path: Path,
) -> DirectoryCapability:
    if not isinstance(capability, DirectoryCapability):
        raise RuntimeError("disk root capability kind is not a directory")
    if not capability.owned_by(backend):
        raise RuntimeError("disk root capability owner does not match backend")
    if not capability.is_open:
        raise RuntimeError("disk root capability is closed")
    if capability.kind is not EntryKind.DIRECTORY:
        raise RuntimeError("disk root capability kind is not a real directory")
    if capability.path_hint != expected_path:
        raise RuntimeError("disk root capability path does not match MeterRoot")
    return capability


def _close_capability_after_error(
    capability: DirectoryCapability | FileCapability | None,
    primary: BaseException,
    *,
    check_deadline: Callable[[], None] | None = None,
) -> None:
    if capability is None or not capability.is_open:
        return
    try:
        capability.close()
    except BaseException as close_error:
        _add_close_note(primary, close_error)
        if capability.is_open:
            try:
                capability.close()
            except BaseException as retry_error:
                _add_close_note(primary, retry_error)
            else:
                if check_deadline is not None:
                    try:
                        check_deadline()
                    except BaseException as deadline_error:
                        _add_deadline_note(primary, deadline_error)
    else:
        if check_deadline is not None:
            try:
                check_deadline()
            except BaseException as deadline_error:
                _add_deadline_note(primary, deadline_error)


def _close_iterator_after_error(
    iterator: DirectoryIterator,
    primary: BaseException,
    *,
    check_deadline: Callable[[], None],
) -> None:
    try:
        iterator.close()
    except BaseException as close_error:
        _add_close_note(primary, close_error)
        try:
            iterator.close()
        except BaseException as retry_error:
            _add_close_note(primary, retry_error)
        else:
            try:
                check_deadline()
            except BaseException as deadline_error:
                _add_deadline_note(primary, deadline_error)
    else:
        try:
            check_deadline()
        except BaseException as deadline_error:
            _add_deadline_note(primary, deadline_error)


def _as_measurement_error(primary: BaseException) -> BaseException:
    if isinstance(primary, DiskMeasurementError):
        return primary
    if not isinstance(primary, Exception):
        return primary
    error = DiskMeasurementError(f"{type(primary).__name__}: {primary}")
    for note in getattr(primary, "__notes__", ()):
        error.add_note(_bounded_secondary(note))
    error.__cause__ = primary
    return error


def _measure_capability(
    backend: FilesystemBackend,
    root: DirectoryCapability,
    *,
    monotonic: Callable[[], float],
    started: float,
    identities: set[FileIdentity],
    identity_bytes: dict[tuple[int, int], int],
) -> tuple[int, int]:
    def check_deadline() -> None:
        if monotonic() - started >= MAX_SCAN_SECONDS:
            raise DiskMeasurementError("owned scratch scan exceeded five seconds")

    entries = 0
    owned_bytes = 0
    conservative_entries = 0
    stack: list[_MeterFrame] = []
    primary: BaseException | None = None
    try:
        check_deadline()
        reopened = backend.reopen_directory(root)
        try:
            check_deadline()
            if (
                reopened.identity != root.identity
                or reopened.filesystem != root.filesystem
                or reopened.kind is not EntryKind.DIRECTORY
            ):
                raise DiskMeasurementError(
                    "owned root identity changed while reopening"
                )
            try:
                iterator = backend.entries_owned(reopened)
            except BaseException as error:
                _close_capability_after_error(
                    reopened,
                    error,
                    check_deadline=check_deadline,
                )
                raise
            try:
                stack.append(_MeterFrame(iterator, ()))
            except BaseException as error:
                _close_iterator_after_error(
                    iterator,
                    error,
                    check_deadline=check_deadline,
                )
                raise
            check_deadline()
        except BaseException as error:
            _close_capability_after_error(
                reopened,
                error,
                check_deadline=check_deadline,
            )
            raise
        while stack:
            check_deadline()
            current = stack[-1]
            try:
                entry = next(current.iterator)
            except StopIteration:
                check_deadline()
                current.iterator.close()
                check_deadline()
                stack.pop()
                continue
            check_deadline()
            entries += 1
            if entries > MAX_TREE_ENTRIES:
                raise DiskMeasurementError(
                    f"owned scratch entries exceed {MAX_TREE_ENTRIES}"
                )
            if entries % 256 == 0:
                check_deadline()
            if (
                entry.identity.volume <= 0
                or entry.identity.file <= 0
                or entry.filesystem.volume <= 0
                or entry.logical_size < 0
            ):
                raise DiskMeasurementError(
                    "owned scratch enumeration returned malformed metadata"
                )
            if entry.kind in {EntryKind.REPARSE, EntryKind.OTHER}:
                continue
            if entry.kind is EntryKind.DIRECTORY:
                child_depth = len(stack)
                next_parts = (*current.path_parts, entry.name)
                if child_depth > MAX_TREE_DEPTH:
                    raise DiskMeasurementError(_bounded_depth_message(next_parts))
                if entry.filesystem != root.filesystem:
                    raise DiskMeasurementError(
                        "owned scratch scan refuses to cross a filesystem boundary"
                    )
                child: DirectoryCapability | None = None
                try:
                    child = backend.open_directory(
                        current.iterator.directory,
                        entry.name,
                        SharePolicy.SCAN,
                    )
                except FileNotFoundError:
                    check_deadline()
                    continue
                try:
                    check_deadline()
                    if (
                        child.identity != entry.identity
                        or child.kind is not EntryKind.DIRECTORY
                    ):
                        raise DiskMeasurementError(
                            "owned scratch directory identity or kind changed while opening"
                        )
                    if (
                        child.filesystem != entry.filesystem
                        or child.filesystem != root.filesystem
                    ):
                        raise DiskMeasurementError(
                            "owned scratch scan refuses to cross a filesystem boundary"
                        )
                    try:
                        child_iterator = backend.entries_owned(child)
                    except BaseException as error:
                        _close_capability_after_error(
                            child,
                            error,
                            check_deadline=check_deadline,
                        )
                        raise
                    try:
                        stack.append(
                            _MeterFrame(child_iterator, next_parts[:8])
                        )
                    except BaseException as error:
                        _close_iterator_after_error(
                            child_iterator,
                            error,
                            check_deadline=check_deadline,
                        )
                        raise
                    check_deadline()
                except BaseException as error:
                    _close_capability_after_error(
                        child,
                        error,
                        check_deadline=check_deadline,
                    )
                    raise
                continue
            if entry.kind is EntryKind.REGULAR:
                opened_file: FileCapability | None = None
                try:
                    opened_file = backend.open_file(
                        current.iterator.directory,
                        entry.name,
                        access=FileAccess.READ,
                        disposition=CreateDisposition.OPEN_EXISTING,
                        share_policy=SharePolicy.SCAN,
                    )
                except FileNotFoundError:
                    check_deadline()
                    continue
                file_primary: BaseException | None = None
                try:
                    check_deadline()
                    if (
                        opened_file.identity != entry.identity
                        or opened_file.filesystem != entry.filesystem
                        or opened_file.kind is not EntryKind.REGULAR
                    ):
                        raise DiskMeasurementError(
                            "owned scratch file identity, filesystem, or kind changed while opening"
                        )
                    identity = opened_file.identity
                    size = opened_file.logical_size
                except BaseException as error:
                    file_primary = error
                try:
                    opened_file.close()
                except BaseException as close_error:
                    if file_primary is None:
                        file_primary = close_error
                    else:
                        _add_close_note(file_primary, close_error)
                    if opened_file.is_open:
                        try:
                            opened_file.close()
                        except BaseException as retry_error:
                            assert file_primary is not None
                            _add_close_note(file_primary, retry_error)
                        else:
                            try:
                                check_deadline()
                            except BaseException as deadline_error:
                                assert file_primary is not None
                                _add_deadline_note(
                                    file_primary,
                                    deadline_error,
                                )
                else:
                    try:
                        check_deadline()
                    except BaseException as deadline_error:
                        if file_primary is None:
                            file_primary = deadline_error
                        else:
                            file_primary.add_note(
                                _bounded_secondary(str(deadline_error))
                            )
                if file_primary is not None:
                    raise file_primary
                if identity in identities:
                    continue
                if len(identities) >= MAX_TREE_ENTRIES:
                    raise DiskMeasurementError(
                        "owned hard-link identity set exceeds entry limit"
                    )
                identities.add(identity)
                identity_bytes[(identity.volume, identity.file)] = size
                owned_bytes += size
                if owned_bytes < 0:
                    raise DiskMeasurementError("owned byte count overflow")
    except BaseException as error:
        primary = error
    finally:
        while stack:
            try:
                frame = stack[-1]
                frame.iterator.close()
                check_deadline()
            except BaseException as close_error:
                if primary is None:
                    primary = close_error
                else:
                    _add_close_note(primary, close_error)
                try:
                    frame.iterator.close()
                    check_deadline()
                except BaseException as retry_error:
                    assert primary is not None
                    _add_close_note(primary, retry_error)
            finally:
                stack.pop()
    if primary is not None:
        raise _as_measurement_error(primary)
    return owned_bytes, conservative_entries


class DiskGuard:
    def __init__(
        self,
        policy: DiskPolicy,
        roots: Iterable[MeterRoot],
        *,
        monotonic: Callable[[], float] = time.monotonic,
        heartbeat: Callable[[], None] | None = None,
        backend: FilesystemBackend | None = None,
    ) -> None:
        self.policy = policy
        self.roots = list(roots)
        self._monotonic = monotonic
        self._backend = (
            default_filesystem_backend() if backend is None else backend
        )
        self._heartbeat = heartbeat
        self._last_heartbeat: float | None = None
        self._lock = threading.Lock()
        self._sample_lock = threading.Lock()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self.observations: list[DiskObservation] = []
        self.sample_count = 0
        self.peak_owned_bytes = 0
        self.minimum_free_bytes: int | None = None
        self.maximum_measurement_seconds = 0.0
        self.start_free_bytes: dict[str, int] = {}
        self.end_free_bytes: dict[str, int] = {}
        self.failure: DiskFailure | None = None
        self.latest_failure: DiskFailure | None = None
        self.close_errors: list[str] = []
        self._root_states: list[_RootCapabilityState] = []
        for root in self.roots:
            acquired: object | None = None
            factory = root.capability_factory
            if factory is None:
                try:
                    acquired = self._backend.open_root(
                        root.path,
                        SharePolicy.SCAN,
                    )
                except OSError as error:
                    try:
                        state = _RootCapabilityState(
                            root=root,
                            capability=None,
                            acquisition_error=(
                                f"{type(error).__name__}: {error}"
                            ),
                            retained_identity=None,
                            exact_identity=None,
                        )
                        self._root_states.append(state)
                    except BaseException as primary_error:
                        self._rollback_constructor_capabilities(primary_error)
                        raise
                    continue
            else:
                try:
                    acquired = factory()
                except BaseException as primary_error:
                    self._rollback_constructor_capabilities(primary_error)
                    raise
            try:
                capability = _validate_root_capability(
                    acquired,
                    self._backend,
                    root.path,
                )
            except BaseException as primary_error:
                if isinstance(acquired, (DirectoryCapability, FileCapability)):
                    _close_capability_after_error(acquired, primary_error)
                self._rollback_constructor_capabilities(primary_error)
                raise
            try:
                state = _RootCapabilityState(
                    root=root,
                    capability=capability,
                    acquisition_error=None,
                    retained_identity=capability.identity,
                    exact_identity=(
                        capability.identity
                        if root.exact_path == root.path
                        else None
                    ),
                )
                self._root_states.append(state)
            except BaseException as primary_error:
                _close_capability_after_error(capability, primary_error)
                self._rollback_constructor_capabilities(primary_error)
                raise

    def _rollback_constructor_capabilities(
        self,
        primary_error: BaseException,
    ) -> None:
        for state in reversed(self._root_states):
            capability = state.capability
            if capability is None or not capability.is_open:
                continue
            _close_capability_after_error(capability, primary_error)
        self._root_states.clear()

    def sample(self) -> DiskFailure | None:
        with self._sample_lock:
            return self._sample_locked()

    def reserve_additional_bytes(
        self,
        additional_bytes: int,
        *,
        filesystem: DirectoryCapability,
    ) -> DiskFailure | None:
        """Fail before a bounded write would cross either disk threshold."""
        if additional_bytes < 0:
            raise ValueError("reserved disk bytes cannot be negative")
        with self._lock:
            if self.failure is not None:
                return self.failure
            failure: DiskFailure | None
            if not self.observations:
                failure = DiskFailure(
                    code=DISK_MEASUREMENT_FAILED,
                    reason=DiskStopReason.MEASUREMENT_FAILED,
                    message="disk write reservation has no successful observation",
                )
            else:
                if not filesystem.owned_by(self._backend):
                    raise RuntimeError(
                        "reservation filesystem capability belongs to another backend"
                    )
                if not filesystem.is_open:
                    raise RuntimeError(
                        "reservation filesystem capability is not open"
                    )
                try:
                    filesystem_identity = filesystem.filesystem
                    actual_available = self._backend.available_bytes(filesystem)
                    fragment_size = self._backend.allocation_unit(filesystem)
                    if fragment_size <= 0 or actual_available < 0:
                        raise DiskMeasurementError(
                            "command spool filesystem reported invalid capacity"
                        )
                except Exception as error:
                    failure = DiskFailure(
                        code=DISK_MEASUREMENT_FAILED,
                        reason=DiskStopReason.MEASUREMENT_FAILED,
                        message=(
                            "command spool capacity measurement failed: "
                            f"{type(error).__name__}: {error}"
                        ),
                    )
                else:
                    base = self.observations[-1]
                    filesystem_key = (
                        f"{filesystem_identity.volume}:"
                        f"{filesystem_identity.discriminator_a}:"
                        f"{filesystem_identity.discriminator_b}"
                    )
                    projected_available = dict(
                        base.filesystem_available_bytes
                    )
                    prior_available = projected_available.get(filesystem_key)
                    if prior_available is None and not projected_available:
                        prior_available = base.available_bytes
                    spool_available = (
                        actual_available
                        if prior_available is None
                        else min(actual_available, prior_available)
                    )
                    rounded_payload = (
                        (additional_bytes + fragment_size - 1)
                        // fragment_size
                        * fragment_size
                    )
                    # Two spool inodes plus one directory-entry update are
                    # charged as a conservative block apiece.  The fresh
                    # post-write sample below replaces this projection with
                    # measured filesystem state.
                    physical_reservation = rounded_payload + 3 * fragment_size
                    projected_available[filesystem_key] = max(
                        0, spool_available - physical_reservation
                    )
                    projected_roots = dict(base.root_owned_bytes)
                    projected_roots["owned:command-spool-reservation"] = (
                        projected_roots.get(
                            "owned:command-spool-reservation", 0
                        )
                        + additional_bytes
                    )
                    projected = replace(
                        base,
                        owned_bytes=base.owned_bytes + additional_bytes,
                        available_bytes=min(projected_available.values()),
                        filesystem_available_bytes=projected_available,
                        root_owned_bytes=projected_roots,
                        identity_bytes={},
                        root_owned_identities={},
                    )
                    failure = evaluate_disk_policy(self.policy, projected)
            self.latest_failure = failure
            if failure is not None:
                self.failure = failure
            return self.failure

    def _sample_locked(self) -> DiskFailure | None:
        started = self._monotonic()

        def check_deadline() -> None:
            if self._monotonic() - started >= MAX_SCAN_SECONDS:
                raise DiskMeasurementError(
                    "owned scratch scan exceeded five seconds"
                )

        owned = 0
        root_owned: dict[str, int] = {}
        conservative_entries = 0
        available: dict[FilesystemIdentity, int] = {}
        try:
            check_deadline()
            now = self._monotonic()
            if self._heartbeat is not None and (
                self._last_heartbeat is None
                or now - self._last_heartbeat >= 60.0
            ):
                self._heartbeat()
                check_deadline()
                self._last_heartbeat = now
            identities: set[FileIdentity] = set()
            identity_bytes: dict[tuple[int, int], int] = {}
            root_owned_identities: dict[
                str, frozenset[tuple[int, int]]
            ] = {}
            for state in self._root_states:
                check_deadline()
                root = state.root
                capability = state.capability
                if capability is None:
                    raise DiskMeasurementError(
                        f"disk root capability unavailable for {root.enforcement}: "
                        f"{state.acquisition_error}"
                    )
                capacity_capability = capability
                transient: DirectoryCapability | None = None
                root_primary: BaseException | None = None
                try:
                    if root.exact_path is not None:
                        try:
                            opened = self._backend.open_root(
                                root.exact_path,
                                SharePolicy.SCAN,
                            )
                        except OSError as error:
                            if (
                                not isinstance(error, FileNotFoundError)
                                and error.errno != errno.ENOENT
                            ):
                                raise
                            check_deadline()
                            if state.exact_identity is not None:
                                raise DiskMeasurementError(
                                    "exact disk root disappeared for "
                                    f"{root.enforcement}"
                                )
                        else:
                            try:
                                transient = _validate_root_capability(
                                    opened,
                                    self._backend,
                                    root.exact_path,
                                )
                            except BaseException as validation_error:
                                if isinstance(
                                    opened,
                                    (DirectoryCapability, FileCapability),
                                ):
                                    _close_capability_after_error(
                                        opened,
                                        validation_error,
                                        check_deadline=check_deadline,
                                    )
                                raise
                            check_deadline()
                            exact_identity = transient.identity
                            expected_exact = state.exact_identity
                            if expected_exact is None:
                                state.exact_identity = exact_identity
                            elif exact_identity != expected_exact:
                                raise DiskMeasurementError(
                                    "exact disk root identity changed for "
                                    f"{root.enforcement}"
                                )
                            capacity_capability = transient
                    filesystem = capacity_capability.filesystem
                    if filesystem not in available:
                        free = self._backend.available_bytes(
                            capacity_capability
                        )
                        check_deadline()
                        if free < 0:
                            raise DiskMeasurementError(
                                "filesystem reported negative available bytes"
                            )
                        available[filesystem] = free
                    if root.charge_owned_bytes:
                        check_deadline()
                        before = owned
                        identities_before = set(identities)
                        measured, conservative = _measure_capability(
                            self._backend,
                            capability,
                            monotonic=self._monotonic,
                            started=started,
                            identities=identities,
                            identity_bytes=identity_bytes,
                        )
                        check_deadline()
                        owned += measured
                        root_owned[root.enforcement] = (
                            root_owned.get(root.enforcement, 0)
                            + owned
                            - before
                        )
                        root_owned_identities[root.enforcement] = frozenset(
                            set(root_owned_identities.get(root.enforcement, ()))
                            | {
                                (identity.volume, identity.file)
                                for identity in identities - identities_before
                            }
                        )
                        conservative_entries += conservative
                except BaseException as error:
                    root_primary = error
                finally:
                    if transient is not None and transient.is_open:
                        try:
                            transient.close()
                        except BaseException as close_error:
                            if root_primary is None:
                                root_primary = close_error
                            else:
                                _add_close_note(root_primary, close_error)
                            if transient.is_open:
                                try:
                                    transient.close()
                                except BaseException as retry_error:
                                    assert root_primary is not None
                                    _add_close_note(
                                        root_primary,
                                        retry_error,
                                    )
                                else:
                                    try:
                                        check_deadline()
                                    except BaseException as deadline_error:
                                        assert root_primary is not None
                                        _add_deadline_note(
                                            root_primary,
                                            deadline_error,
                                        )
                        else:
                            try:
                                check_deadline()
                            except BaseException as deadline_error:
                                if root_primary is None:
                                    root_primary = deadline_error
                                else:
                                    root_primary.add_note(
                                        _bounded_secondary(str(deadline_error))
                                    )
                if root_primary is not None:
                    raise _as_measurement_error(root_primary)
                check_deadline()
            if not available:
                raise DiskMeasurementError("disk guard has no capacity roots")
            observation = DiskObservation(
                owned_bytes=owned,
                available_bytes=min(available.values()),
                measured_in_seconds=self._monotonic() - started,
                conservative_entries=conservative_entries,
                filesystem_available_bytes={
                    (
                        f"{filesystem.volume}:"
                        f"{filesystem.discriminator_a}:"
                        f"{filesystem.discriminator_b}"
                    ): free
                    for filesystem, free in available.items()
                },
                root_owned_bytes=root_owned,
                identity_bytes=identity_bytes,
                root_owned_identities=root_owned_identities,
            )
            failure = evaluate_disk_policy(self.policy, observation)
        except Exception as error:
            observation = None
            notes = " ".join(
                f"[{_bounded_secondary(note)}]"
                for note in getattr(error, "__notes__", ())
            )
            failure = DiskFailure(
                code=DISK_MEASUREMENT_FAILED,
                reason=DiskStopReason.MEASUREMENT_FAILED,
                message=(f"{error} {notes}".rstrip()),
            )
        with self._lock:
            self.latest_failure = failure
            if observation is not None:
                if self.observations:
                    previous = self.observations[-1]
                    if previous.identity_bytes or previous.root_owned_identities:
                        self.observations[-1] = replace(
                            previous,
                            identity_bytes={},
                            root_owned_identities={},
                        )
                self.sample_count += 1
                self.peak_owned_bytes = max(
                    self.peak_owned_bytes, observation.owned_bytes
                )
                self.minimum_free_bytes = (
                    observation.available_bytes
                    if self.minimum_free_bytes is None
                    else min(self.minimum_free_bytes, observation.available_bytes)
                )
                self.maximum_measurement_seconds = max(
                    self.maximum_measurement_seconds,
                    observation.measured_in_seconds,
                )
                if not self.start_free_bytes:
                    self.start_free_bytes = dict(
                        observation.filesystem_available_bytes
                    )
                self.end_free_bytes = dict(
                    observation.filesystem_available_bytes
                )
                if len(self.observations) >= MAX_DISK_OBSERVATIONS:
                    del self.observations[1]
                self.observations.append(observation)
            if self.failure is None and failure is not None:
                failure_observation = failure.observation
                if failure_observation is not None:
                    failure_observation = replace(
                        failure_observation,
                        identity_bytes={},
                        root_owned_identities={},
                    )
                self.failure = replace(
                    failure,
                    observation=failure_observation,
                    secondary=tuple(
                        replace(
                            secondary,
                            observation=(
                                replace(
                                    secondary.observation,
                                    identity_bytes={},
                                    root_owned_identities={},
                                )
                                if secondary.observation is not None
                                else None
                            ),
                        )
                        for secondary in failure.secondary
                    ),
                )
            return self.failure

    def start(self) -> None:
        if self._thread is not None:
            raise RuntimeError("disk guard already started")
        self.sample()
        self._thread = threading.Thread(
            target=self._monitor,
            name="focused-mutation-disk-monitor",
            daemon=True,
        )
        try:
            self._thread.start()
        except BaseException:
            self._thread = None
            raise

    def _monitor(self) -> None:
        try:
            while not self._stop.wait(self.policy.sample_interval_seconds):
                self.sample()
                if self.failure is not None:
                    return
        finally:
            self._close_capabilities()

    def stop_and_join(self, timeout: float) -> bool:
        self._stop.set()
        thread = self._thread
        if thread is None:
            self.close()
            return True
        thread.join(timeout)
        joined = not thread.is_alive()
        if joined:
            self.close()
        return joined

    def probe_close(self) -> tuple[str, ...]:
        errors: list[str] = []
        for state in self._root_states:
            capability = state.capability
            if capability is None:
                continue
            duplicate: DirectoryCapability | None = None
            try:
                duplicate = self._backend.reopen_directory(capability)
                duplicate.close()
            except Exception as error:
                errors.append(
                    _bounded_secondary(
                        f"{state.root.enforcement} capability close probe failed: "
                        f"{type(error).__name__}: {error}"
                    )
                )
            finally:
                if duplicate is not None and duplicate.is_open:
                    try:
                        duplicate.close()
                    except Exception as retry_error:
                        errors.append(
                            _bounded_secondary(
                                f"{state.root.enforcement} capability close probe "
                                f"retry failed: {type(retry_error).__name__}: "
                                f"{retry_error}"
                            )
                        )
        return tuple(errors)

    def close(self) -> tuple[str, ...]:
        if self._thread is not None and self._thread.is_alive():
            raise RuntimeError("cannot close disk capabilities while monitor is active")
        return self._close_capabilities()

    def _close_capabilities(self) -> tuple[str, ...]:
        for state in self._root_states:
            capability = state.capability
            if capability is None:
                continue
            try:
                capability.close()
            except Exception as close_error:
                self.close_errors.append(
                    f"{state.root.enforcement} capability close failed: "
                    f"{type(close_error).__name__}: {close_error}"
                )
            else:
                state.capability = None
        return tuple(self.close_errors)
