from __future__ import annotations

from dataclasses import dataclass, field, replace
from enum import StrEnum
import os
from pathlib import Path
import re
import tempfile
import stat
import threading
import time
import sys
from collections.abc import Callable, Iterable


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


def canonical_scratch_root(value: Path | None) -> Path:
    root = Path(tempfile.gettempdir()) if value is None else value
    try:
        canonical = root.resolve(strict=True)
    except OSError as error:
        raise ValueError(f"scratch root cannot be canonicalized: {root}: {error}") from error
    if not canonical.is_dir():
        raise ValueError(f"scratch root is not a directory: {canonical}")
    try:
        os.statvfs(canonical)
    except OSError as error:
        raise ValueError(f"scratch root free space cannot be queried: {canonical}: {error}") from error
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


class DiskMeasurementError(RuntimeError):
    pass


def _open_directory(path: Path) -> int:
    flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
    flags |= getattr(os, "O_NOFOLLOW", 0)
    return os.open(path, flags)


def _open_child_directory(parent_fd: int, name: str) -> int:
    flags = (
        os.O_RDONLY
        | getattr(os, "O_DIRECTORY", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )
    if not sys.platform.startswith("linux"):
        return os.open(name, flags, dir_fd=parent_fd)
    import ctypes

    class OpenHow(ctypes.Structure):
        _fields_ = [
            ("flags", ctypes.c_uint64),
            ("mode", ctypes.c_uint64),
            ("resolve", ctypes.c_uint64),
        ]

    encoded = os.fsencode(name)
    if b"/" in encoded or encoded in {b"", b".", b".."}:
        raise DiskMeasurementError("meter component is not a direct child")
    how = OpenHow(
        flags=flags | getattr(os, "O_CLOEXEC", 0),
        mode=0,
        resolve=0x01 | 0x02 | 0x04 | 0x08,
    )
    libc = ctypes.CDLL(None, use_errno=True)
    result = libc.syscall(
        437,
        parent_fd,
        ctypes.c_char_p(encoded),
        ctypes.byref(how),
        ctypes.sizeof(how),
    )
    if result < 0:
        error_number = ctypes.get_errno()
        raise OSError(error_number, os.strerror(error_number), name)
    return int(result)


def _open_child_regular(parent_fd: int, name: str) -> int:
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    if not sys.platform.startswith("linux"):
        return os.open(name, flags, dir_fd=parent_fd)
    import ctypes

    class OpenHow(ctypes.Structure):
        _fields_ = [
            ("flags", ctypes.c_uint64),
            ("mode", ctypes.c_uint64),
            ("resolve", ctypes.c_uint64),
        ]

    encoded = os.fsencode(name)
    if b"/" in encoded or encoded in {b"", b".", b".."}:
        raise DiskMeasurementError("meter component is not a direct child")
    how = OpenHow(
        flags=flags | getattr(os, "O_CLOEXEC", 0),
        mode=0,
        resolve=0x01 | 0x02 | 0x04 | 0x08,
    )
    libc = ctypes.CDLL(None, use_errno=True)
    result = libc.syscall(
        437,
        parent_fd,
        ctypes.c_char_p(encoded),
        ctypes.byref(how),
        ctypes.sizeof(how),
    )
    if result < 0:
        error_number = ctypes.get_errno()
        raise OSError(error_number, os.strerror(error_number), name)
    return int(result)


def _reopen_same_directory(fd: int) -> int:
    flags = (
        os.O_RDONLY
        | getattr(os, "O_DIRECTORY", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )
    reopened = os.open(".", flags, dir_fd=fd)
    original = os.fstat(fd)
    current = os.fstat(reopened)
    if (original.st_dev, original.st_ino) != (current.st_dev, current.st_ino):
        os.close(reopened)
        raise DiskMeasurementError("owned root identity changed while reopening")
    return reopened


def _filesystem_identity(fd: int) -> tuple[int, int, int]:
    metadata = os.fstat(fd)
    if sys.platform != "darwin":
        return metadata.st_dev, 0, 0
    import ctypes
    import struct

    value = ctypes.create_string_buffer(4_096)
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.fstatfs(fd, ctypes.byref(value)) != 0:
        error_number = ctypes.get_errno()
        raise OSError(error_number, os.strerror(error_number))
    first, second = struct.unpack_from("=ii", value.raw, 48)
    return metadata.st_dev, first, second


class _DirectoryStream:
    """One-FD, handle-relative directory stream for bounded DFS."""

    def __init__(self, fd: int) -> None:
        import ctypes

        self._ctypes = ctypes
        self._libc = ctypes.CDLL(None, use_errno=True)
        self._libc.fdopendir.argtypes = (ctypes.c_int,)
        self._libc.fdopendir.restype = ctypes.c_void_p
        self._libc.readdir.argtypes = (ctypes.c_void_p,)
        self._libc.readdir.restype = ctypes.c_void_p
        self._libc.dirfd.argtypes = (ctypes.c_void_p,)
        self._libc.dirfd.restype = ctypes.c_int
        self._libc.closedir.argtypes = (ctypes.c_void_p,)
        self._libc.closedir.restype = ctypes.c_int
        pointer = self._libc.fdopendir(fd)
        if not pointer:
            error_number = ctypes.get_errno()
            os.close(fd)
            raise OSError(error_number, os.strerror(error_number))
        self._pointer = pointer

    @property
    def fd(self) -> int:
        value = self._libc.dirfd(self._pointer)
        if value < 0:
            error_number = self._ctypes.get_errno()
            raise OSError(error_number, os.strerror(error_number))
        return int(value)

    def next_name(self) -> str | None:
        while True:
            self._ctypes.set_errno(0)
            pointer = self._libc.readdir(self._pointer)
            if not pointer:
                error_number = self._ctypes.get_errno()
                if error_number:
                    raise OSError(error_number, os.strerror(error_number))
                return None
            name_offset = 21 if sys.platform == "darwin" else 19
            encoded = self._ctypes.string_at(pointer + name_offset)
            name = os.fsdecode(encoded)
            if name not in {".", ".."}:
                return name

    def close(self) -> None:
        pointer = self._pointer
        if not pointer:
            return
        self._pointer = None
        if self._libc.closedir(pointer) != 0:
            error_number = self._ctypes.get_errno()
            raise OSError(error_number, os.strerror(error_number))


def _measure_fd(
    root_fd: int,
    *,
    monotonic: Callable[[], float],
    started: float,
    identities: set[tuple[int, int]],
    identity_bytes: dict[tuple[int, int], int],
) -> tuple[int, int]:
    root_stat = os.fstat(root_fd)
    root_filesystem = _filesystem_identity(root_fd)
    entries = 0
    owned_bytes = 0
    conservative_entries = 0

    def check_deadline() -> None:
        if monotonic() - started >= MAX_SCAN_SECONDS:
            raise DiskMeasurementError("owned scratch scan exceeded five seconds")

    stack: list[tuple[_DirectoryStream, tuple[str, ...]]] = [
        (_DirectoryStream(_reopen_same_directory(root_fd)), ())
    ]
    try:
        while stack:
            check_deadline()
            current, path_prefix = stack[-1]
            selected = current.next_name()
            if selected is None:
                current.close()
                stack.pop()
                continue
            entries += 1
            if entries > MAX_TREE_ENTRIES:
                raise DiskMeasurementError(
                    f"owned scratch entries exceed {MAX_TREE_ENTRIES}"
                )
            if entries % 256 == 0:
                check_deadline()
            try:
                metadata = os.stat(
                    selected,
                    dir_fd=current.fd,
                    follow_symlinks=False,
                )
            except FileNotFoundError:
                continue
            mode = metadata.st_mode
            if stat.S_ISLNK(mode):
                continue
            if stat.S_ISDIR(mode):
                child_depth = len(stack)
                if child_depth > MAX_TREE_DEPTH:
                    relative = "/".join((*path_prefix, selected))
                    bounded = (
                        relative.encode("utf-8", errors="backslashreplace")[:768]
                        .decode("utf-8", errors="ignore")
                    )
                    raise DiskMeasurementError(
                        f"owned scratch depth exceeds {MAX_TREE_DEPTH} below {bounded}"
                    )
                if metadata.st_dev != root_stat.st_dev:
                    raise DiskMeasurementError(
                        "owned scratch scan refuses to cross a filesystem boundary"
                    )
                try:
                    child_fd = _open_child_directory(current.fd, selected)
                except FileNotFoundError:
                    continue
                opened = os.fstat(child_fd)
                if (opened.st_dev, opened.st_ino) != (
                    metadata.st_dev,
                    metadata.st_ino,
                ):
                    os.close(child_fd)
                    raise DiskMeasurementError(
                        "owned scratch directory identity changed while opening"
                    )
                if _filesystem_identity(child_fd) != root_filesystem:
                    os.close(child_fd)
                    raise DiskMeasurementError(
                        "owned scratch scan refuses to cross a filesystem boundary"
                    )
                stack.append(
                    (
                        _DirectoryStream(child_fd),
                        (*path_prefix, selected)[:8],
                    )
                )
                continue
            if stat.S_ISREG(mode):
                try:
                    file_fd = _open_child_regular(current.fd, selected)
                except FileNotFoundError:
                    continue
                try:
                    opened = os.fstat(file_fd)
                finally:
                    os.close(file_fd)
                if not stat.S_ISREG(opened.st_mode) or (
                    opened.st_dev,
                    opened.st_ino,
                ) != (metadata.st_dev, metadata.st_ino):
                    raise DiskMeasurementError(
                        "owned scratch file identity changed while opening"
                    )
                identity = (opened.st_dev, opened.st_ino)
                if opened.st_dev and opened.st_ino and identity in identities:
                    continue
                if opened.st_dev and opened.st_ino:
                    if len(identities) >= MAX_TREE_ENTRIES:
                        raise DiskMeasurementError(
                            "owned hard-link identity set exceeds entry limit"
                        )
                    identities.add(identity)
                    identity_bytes[identity] = opened.st_size
                else:
                    conservative_entries += 1
                owned_bytes += opened.st_size
                if owned_bytes < 0:
                    raise DiskMeasurementError("owned byte count overflow")
    finally:
        while stack:
            try:
                stack.pop()[0].close()
            except OSError:
                pass
    return owned_bytes, conservative_entries


class DiskGuard:
    def __init__(
        self,
        policy: DiskPolicy,
        roots: Iterable[MeterRoot],
        *,
        monotonic: Callable[[], float] = time.monotonic,
        heartbeat: Callable[[], None] | None = None,
    ) -> None:
        self.policy = policy
        self.roots = list(roots)
        self._monotonic = monotonic
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
        self._root_capabilities: list[tuple[MeterRoot, int | None, str | None]] = []
        self._root_identities: list[tuple[int, int] | None] = []
        self._exact_identities: list[tuple[int, int] | None] = []
        for root in self.roots:
            try:
                descriptor = _open_directory(root.path)
            except OSError as error:
                self._root_capabilities.append(
                    (root, None, f"{type(error).__name__}: {error}")
                )
                self._root_identities.append(None)
                self._exact_identities.append(None)
            else:
                self._root_capabilities.append((root, descriptor, None))
                metadata = os.fstat(descriptor)
                self._root_identities.append((metadata.st_dev, metadata.st_ino))
                self._exact_identities.append(
                    (metadata.st_dev, metadata.st_ino)
                    if root.exact_path == root.path
                    else None
                )

    def sample(self) -> DiskFailure | None:
        with self._sample_lock:
            return self._sample_locked()

    def reserve_additional_bytes(
        self,
        additional_bytes: int,
        *,
        filesystem_fd: int,
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
                try:
                    filesystem = _filesystem_identity(filesystem_fd)
                    values = os.fstatvfs(filesystem_fd)
                    fragment_size = values.f_frsize
                    if fragment_size <= 0 or values.f_bavail < 0:
                        raise DiskMeasurementError(
                            "command spool filesystem reported invalid capacity"
                        )
                    actual_available = values.f_bavail * fragment_size
                except (OSError, DiskMeasurementError) as error:
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
                    filesystem_key = ":".join(str(item) for item in filesystem)
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
        owned = 0
        root_owned: dict[str, int] = {}
        conservative_entries = 0
        available: dict[tuple[int, int, int], int] = {}
        try:
            now = self._monotonic()
            if self._heartbeat is not None and (
                self._last_heartbeat is None
                or now - self._last_heartbeat >= 60.0
            ):
                self._heartbeat()
                self._last_heartbeat = now
            identities: set[tuple[int, int]] = set()
            identity_bytes: dict[tuple[int, int], int] = {}
            root_owned_identities: dict[
                str, frozenset[tuple[int, int]]
            ] = {}
            for index, (root, fd, open_error) in enumerate(self._root_capabilities):
                if fd is None:
                    raise DiskMeasurementError(
                        f"disk root capability unavailable for {root.enforcement}: "
                        f"{open_error}"
                    )
                capacity_fd = fd
                transient_fd = -1
                if root.exact_path is not None:
                    verification_fd = _open_directory(root.path)
                    try:
                        verification = os.fstat(verification_fd)
                        if (verification.st_dev, verification.st_ino) != (
                            self._root_identities[index]
                        ):
                            raise DiskMeasurementError(
                                f"disk root path identity changed for {root.enforcement}"
                            )
                    finally:
                        os.close(verification_fd)
                    try:
                        transient_fd = _open_directory(root.exact_path)
                    except FileNotFoundError:
                        if self._exact_identities[index] is not None:
                            raise DiskMeasurementError(
                                f"exact disk root disappeared for {root.enforcement}"
                            )
                        transient_fd = -1
                    else:
                        exact_metadata = os.fstat(transient_fd)
                        exact_identity = (
                            exact_metadata.st_dev,
                            exact_metadata.st_ino,
                        )
                        expected_exact = self._exact_identities[index]
                        if expected_exact is None:
                            self._exact_identities[index] = exact_identity
                        elif exact_identity != expected_exact:
                            os.close(transient_fd)
                            transient_fd = -1
                            raise DiskMeasurementError(
                                f"exact disk root identity changed for {root.enforcement}"
                            )
                        capacity_fd = transient_fd
                try:
                    filesystem = _filesystem_identity(capacity_fd)
                    if filesystem not in available:
                        values = os.fstatvfs(capacity_fd)
                        available[filesystem] = values.f_bavail * values.f_frsize
                    if root.charge_owned_bytes:
                        before = owned
                        identities_before = set(identities)
                        measured, conservative = _measure_fd(
                            fd,
                            monotonic=self._monotonic,
                            started=started,
                            identities=identities,
                            identity_bytes=identity_bytes,
                        )
                        owned += measured
                        root_owned[root.enforcement] = (
                            root_owned.get(root.enforcement, 0)
                            + owned
                            - before
                        )
                        root_owned_identities[root.enforcement] = frozenset(
                            set(root_owned_identities.get(root.enforcement, ()))
                            | (identities - identities_before)
                        )
                        conservative_entries += conservative
                finally:
                    if transient_fd >= 0:
                        os.close(transient_fd)
            if not available:
                raise DiskMeasurementError("disk guard has no capacity roots")
            observation = DiskObservation(
                owned_bytes=owned,
                available_bytes=min(available.values()),
                measured_in_seconds=self._monotonic() - started,
                conservative_entries=conservative_entries,
                filesystem_available_bytes={
                    f"{device}:{first}:{second}": free
                    for (device, first, second), free in available.items()
                },
                root_owned_bytes=root_owned,
                identity_bytes=identity_bytes,
                root_owned_identities=root_owned_identities,
            )
            failure = evaluate_disk_policy(self.policy, observation)
        except (OSError, UnicodeError, DiskMeasurementError) as error:
            observation = None
            failure = DiskFailure(
                code=DISK_MEASUREMENT_FAILED,
                reason=DiskStopReason.MEASUREMENT_FAILED,
                message=str(error),
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
        for root, fd, _error in self._root_capabilities:
            if fd is None:
                continue
            duplicate = -1
            try:
                duplicate = os.dup(fd)
                os.close(duplicate)
                duplicate = -1
            except OSError as error:
                errors.append(
                    f"{root.enforcement} capability close probe failed: "
                    f"{type(error).__name__}: {error}"
                )
            finally:
                if duplicate >= 0:
                    try:
                        os.close(duplicate)
                    except OSError:
                        pass
        return tuple(errors)

    def close(self) -> tuple[str, ...]:
        if self._thread is not None and self._thread.is_alive():
            raise RuntimeError("cannot close disk capabilities while monitor is active")
        return self._close_capabilities()

    def _close_capabilities(self) -> tuple[str, ...]:
        for index, (root, fd, error) in enumerate(self._root_capabilities):
            if fd is not None:
                try:
                    os.close(fd)
                except OSError as close_error:
                    self.close_errors.append(
                        f"{root.enforcement} capability close failed: "
                        f"{type(close_error).__name__}: {close_error}"
                    )
                self._root_capabilities[index] = (root, None, error or "closed")
        return tuple(self.close_errors)
