from __future__ import annotations

from dataclasses import dataclass, replace
from enum import StrEnum
import json
from pathlib import Path
from typing import Any, Callable
import unittest

from tools.focused_mutation_support.disk import (
    CleanupOutcome,
    ComponentState,
    DISK_MEASUREMENT_FAILED,
    DiskLifecycle,
    DiskLifecycleEvent,
    DiskLifecycleSnapshot,
    DiskLifecycleTraceRunner,
    DiskFailure,
    DiskObservation,
    DiskPolicy,
    DiskRootId,
    DiskSecondary,
    DiskStopReason,
    EventKind,
    apply_disk_lifecycle_event,
    evaluate_disk_policy,
    snapshot_disk_lifecycle,
)


ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "formal/HoiminOracle/corpus/disk-guard-lifecycle.jsonl"
STATE_KEYS = {
    "stop",
    "secondary_stops",
    "active",
    "dispatched",
    "owned_roots",
    "delivery_roots",
    "cleanup_requested",
    "cleanup_clean",
    "cleanup_failed",
    "cleanup_deferred",
    "cleanup_retained",
    "process_drain",
    "output_drain",
    "monitor_join",
    "report",
    "finished",
}
CASE_KEYS = {
    "schema",
    "id",
    "mode",
    "layer",
    "implementation_targets",
    "initial",
    "events",
    "expected",
}
EVENT_KEYS = {
    "observe": {"kind", "owned", "max_owned", "free", "min_free"},
    "meter_failed": {"kind"},
    "dispatch": {"kind"},
    "process_drain_succeeded": {"kind"},
    "process_drain_failed": {"kind"},
    "output_drained": {"kind"},
    "output_drain_failed": {"kind"},
    "monitor_joined": {"kind"},
    "monitor_join_failed": {"kind"},
    "report_succeeded": {"kind"},
    "report_failed": {"kind"},
    "finish": {"kind"},
    "cleanup_requested": {"kind", "root"},
    "cleanup_succeeded": {"kind", "root"},
    "cleanup_failed": {"kind", "root"},
    "cleanup_deferred": {"kind", "root"},
    "cleanup_retained": {"kind", "root"},
}
STOP_NAMES = {
    "workspace_size_exceeded",
    "filesystem_reserve_reached",
    "measurement_failed",
    "process_failed",
}
ROOT_NAMES = {"execution", "delivery"}
COMPONENT_NAMES = {"pending", "succeeded", "failed"}


class OracleResultKind(StrEnum):
    MATCH = "match"
    MISMATCH = "mismatch"
    INFRASTRUCTURE = "infrastructure"
    NON_APPLICABLE = "non_applicable"


@dataclass(frozen=True)
class OracleResult:
    case_id: str
    mode: str
    kind: OracleResultKind
    detail: str = ""


class BrokenFamily(StrEnum):
    THRESHOLD = "threshold"
    PRECEDENCE = "precedence"
    DISPATCH = "dispatch"
    CLEANUP = "cleanup"


def _strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    value: dict[str, object] = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def _require_exact_keys(
    value: dict[str, object], expected: set[str], label: str
) -> None:
    if set(value) != expected:
        raise ValueError(
            f"{label} fields differ: missing={sorted(expected - set(value))}, "
            f"unknown={sorted(set(value) - expected)}"
        )


def _validate_state(value: dict[str, object], label: str) -> None:
    if value["stop"] is not None and value["stop"] not in STOP_NAMES:
        raise ValueError(f"{label} has invalid stop")
    secondary = value["secondary_stops"]
    if (
        not isinstance(secondary, list)
        or any(item not in STOP_NAMES for item in secondary)
        or len(set(secondary)) != len(secondary)
    ):
        raise ValueError(f"{label} has invalid secondary stops")
    for field in ("active", "dispatched"):
        if type(value[field]) is not int or value[field] < 0:
            raise ValueError(f"{label} has invalid {field}")
    for field in (
        "owned_roots",
        "delivery_roots",
        "cleanup_requested",
        "cleanup_clean",
        "cleanup_failed",
        "cleanup_deferred",
        "cleanup_retained",
    ):
        roots = value[field]
        if (
            not isinstance(roots, list)
            or any(root not in ROOT_NAMES for root in roots)
            or len(set(roots)) != len(roots)
        ):
            raise ValueError(f"{label} has invalid {field}")
    for field in ("process_drain", "output_drain", "monitor_join", "report"):
        if value[field] not in COMPONENT_NAMES:
            raise ValueError(f"{label} has invalid {field}")
    if type(value["finished"]) is not bool:
        raise ValueError(f"{label} has invalid finished flag")


def _parse_cases(source: str) -> list[dict[str, Any]]:
    cases: list[dict[str, Any]] = []
    ids: set[str] = set()
    for line_number, line in enumerate(source.splitlines(), 1):
        if not line:
            raise ValueError(f"empty corpus line {line_number}")
        case = json.loads(line, object_pairs_hook=_strict_object)
        if not isinstance(case, dict):
            raise ValueError(f"case {line_number} is not an object")
        _require_exact_keys(case, CASE_KEYS, f"case {line_number}")
        if type(case["schema"]) is not int or case["schema"] != 1:
            raise ValueError(f"case {line_number} has unsupported schema")
        case_id = case["id"]
        if not isinstance(case_id, str) or not case_id or case_id in ids:
            raise ValueError(f"case {line_number} has invalid or duplicate id")
        ids.add(case_id)
        if case["layer"] not in {"policy", "runtime"}:
            raise ValueError(f"{case_id} has unknown layer")
        if case["mode"] not in {
            "strict",
            "internal-fixture",
            "model-only",
            "infrastructure-error",
        }:
            raise ValueError(f"{case_id} has unknown mode")
        targets = case["implementation_targets"]
        if (
            not isinstance(targets, list)
            or not targets
            or any(target not in {"rust", "python"} for target in targets)
            or len(set(targets)) != len(targets)
        ):
            raise ValueError(f"{case_id} has invalid implementation targets")
        initial = case["initial"]
        expected = case["expected"]
        if not isinstance(initial, dict) or not isinstance(expected, dict):
            raise ValueError(f"{case_id} state is not an object")
        _require_exact_keys(initial, STATE_KEYS, f"{case_id} initial")
        _validate_state(initial, f"{case_id} initial")
        _require_exact_keys(
            expected,
            STATE_KEYS | {"accepted", "rejected_at"},
            f"{case_id} expected",
        )
        _validate_state(expected, f"{case_id} expected")
        if type(expected["accepted"]) is not bool or (
            expected["rejected_at"] is not None
            and (
                type(expected["rejected_at"]) is not int
                or expected["rejected_at"] < 0
            )
        ):
            raise ValueError(f"{case_id} has invalid acceptance result")
        if expected["accepted"] != (expected["rejected_at"] is None):
            raise ValueError(f"{case_id} has inconsistent acceptance result")
        events = case["events"]
        if not isinstance(events, list):
            raise ValueError(f"{case_id} events are not a list")
        for index, event in enumerate(events):
            if not isinstance(event, dict) or not isinstance(event.get("kind"), str):
                raise ValueError(f"{case_id} event {index} is invalid")
            kind = event["kind"]
            keys = EVENT_KEYS.get(kind)
            if keys is None:
                raise ValueError(f"{case_id} event {index} has unknown kind")
            _require_exact_keys(event, keys, f"{case_id} event {index}")
            if "root" in event and event["root"] not in ROOT_NAMES:
                raise ValueError(f"{case_id} event {index} has unknown root")
            if kind == "observe":
                for field in ("owned", "max_owned", "free", "min_free"):
                    if type(event[field]) is not int or event[field] < 0:
                        raise ValueError(
                            f"{case_id} event {index} has invalid {field}"
                        )
                if event["max_owned"] == 0 or event["min_free"] == 0:
                    raise ValueError(
                        f"{case_id} event {index} has non-positive policy"
                    )
        cases.append(case)
    if not cases:
        raise ValueError("disk oracle corpus is empty")
    return cases


def _root(value: object) -> DiskRootId:
    if value == "execution":
        return DiskRootId.EXECUTION
    if value == "delivery":
        return DiskRootId.DELIVERY
    raise ValueError(f"unknown root: {value!r}")


def _event(value: dict[str, Any]) -> DiskLifecycleEvent:
    kind = value["kind"]
    if kind == "observe":
        return DiskLifecycleEvent.observation(
            DiskPolicy(
                max_disk_bytes=value["max_owned"],
                min_free_bytes=value["min_free"],
            ),
            DiskObservation(
                owned_bytes=value["owned"],
                available_bytes=value["free"],
            ),
        )
    simple = {
        "meter_failed": DiskLifecycleEvent.measurement_failed("oracle fixture"),
        "dispatch": DiskLifecycleEvent.dispatch_requested(),
        "process_drain_succeeded": DiskLifecycleEvent.process_drain_succeeded(),
        "process_drain_failed": DiskLifecycleEvent.process_drain_failed(),
        "output_drained": DiskLifecycleEvent.output_drain_succeeded(),
        "output_drain_failed": DiskLifecycleEvent.output_drain_failed(),
        "monitor_joined": DiskLifecycleEvent.monitor_join_succeeded(),
        "monitor_join_failed": DiskLifecycleEvent.monitor_join_failed(),
        "report_succeeded": DiskLifecycleEvent.report_succeeded(),
        "report_failed": DiskLifecycleEvent.report_failed(),
        "finish": DiskLifecycleEvent.finish_requested(),
    }
    if kind in simple:
        return simple[kind]
    root = _root(value["root"])
    if kind == "cleanup_requested":
        return DiskLifecycleEvent.cleanup_requested(root)
    outcome = {
        "cleanup_succeeded": CleanupOutcome.CLEAN,
        "cleanup_failed": CleanupOutcome.FAILED,
        "cleanup_deferred": CleanupOutcome.DEFERRED,
        "cleanup_retained": CleanupOutcome.RETAINED,
    }.get(kind)
    if outcome is None:
        raise ValueError(f"unsupported event: {kind}")
    return DiskLifecycleEvent.cleanup_completed(root, outcome, "oracle fixture")


def _apply_runtime_step(
    runner: DiskLifecycleTraceRunner, value: dict[str, Any]
) -> bool:
    kind = value["kind"]
    if kind == "dispatch":
        return runner.dispatch()
    if kind == "process_drain_succeeded":
        return runner.record_process_drain(True)
    if kind == "process_drain_failed":
        return runner.record_process_drain(False)
    if kind == "output_drained":
        return runner.record_output_drain(True)
    if kind == "output_drain_failed":
        return runner.record_output_drain(False)
    if kind == "monitor_joined":
        return runner.record_monitor_join(True)
    if kind == "monitor_join_failed":
        return runner.record_monitor_join(False)
    if kind == "cleanup_requested":
        return runner.request_cleanup(_root(value["root"]))
    cleanup = {
        "cleanup_succeeded": CleanupOutcome.CLEAN,
        "cleanup_failed": CleanupOutcome.FAILED,
        "cleanup_deferred": CleanupOutcome.DEFERRED,
        "cleanup_retained": CleanupOutcome.RETAINED,
    }.get(kind)
    if cleanup is not None:
        return runner.complete_cleanup(
            _root(value["root"]), cleanup, "oracle fixture"
        )
    if kind == "report_succeeded":
        return runner.record_report(True)
    if kind == "report_failed":
        return runner.record_report(False)
    if kind == "finish":
        return runner.finish()
    if kind in {"observe", "meter_failed"}:
        return runner.apply(_event(value))
    raise ValueError(f"unsupported runtime event: {kind}")


def _reason(value: DiskStopReason) -> str:
    return value.value


def _component(value: ComponentState) -> str:
    return value.value


def _snapshot(
    lifecycle: DiskLifecycleSnapshot,
    *,
    accepted: bool | None = None,
    rejected_at: int | None = None,
) -> dict[str, object]:
    stop = (
        _reason(lifecycle.stop.reason) if lifecycle.stop is not None else None
    )
    secondary: list[str] = []
    values: list[DiskSecondary] = []
    if lifecycle.stop is not None:
        values.extend(lifecycle.stop.secondary)
    values.extend(lifecycle.secondary)
    for value in values:
        if value.reason is not None:
            name = _reason(value.reason)
            if name not in secondary:
                secondary.append(name)
    snapshot: dict[str, object] = {
        "stop": stop,
        "secondary_stops": secondary,
        "active": lifecycle.active,
        "dispatched": lifecycle.dispatched,
        "owned_roots": [root.value for root in lifecycle.owned_roots],
        "delivery_roots": [root.value for root in lifecycle.delivery_roots],
        "cleanup_requested": [root.value for root in lifecycle.cleanup_requested],
        "cleanup_clean": [root.value for root in lifecycle.cleanup_clean],
        "cleanup_failed": [root.value for root in lifecycle.cleanup_failed],
        "cleanup_deferred": [root.value for root in lifecycle.cleanup_deferred],
        "cleanup_retained": [root.value for root in lifecycle.cleanup_retained],
        "process_drain": _component(lifecycle.process_drain),
        "output_drain": _component(lifecycle.output_drain),
        "monitor_join": _component(lifecycle.monitor_join),
        "report": _component(lifecycle.report),
        "finished": lifecycle.finished,
    }
    if accepted is not None:
        snapshot["accepted"] = accepted
        snapshot["rejected_at"] = rejected_at
    return snapshot


def _execute(case: dict[str, Any]) -> dict[str, object]:
    roots = [_root(root) for root in case["initial"]["owned_roots"]]
    runner = DiskLifecycleTraceRunner(roots)
    initial = _snapshot(runner.snapshot())
    if initial != case["initial"]:
        raise ValueError(f"{case['id']} initial state is not constructible")
    if case["layer"] == "runtime":
        accepted = True
        rejected_at: int | None = None
        for index, value in enumerate(case["events"]):
            if not _apply_runtime_step(runner, value):
                accepted = False
                rejected_at = index
                break
        return _snapshot(
            runner.snapshot(),
            accepted=accepted,
            rejected_at=rejected_at,
        )
    events = [_event(event) for event in case["events"]]
    lifecycle = DiskLifecycle(roots)
    accepted = True
    rejected_at: int | None = None
    for index, event in enumerate(events):
        if not apply_disk_lifecycle_event(lifecycle, event):
            accepted = False
            rejected_at = index
            break
    return _snapshot(
        snapshot_disk_lifecycle(lifecycle),
        accepted=accepted,
        rejected_at=rejected_at,
    )


def _result(case: dict[str, Any]) -> OracleResult:
    if "python" not in case["implementation_targets"]:
        return OracleResult(
            case["id"], case["mode"], OracleResultKind.NON_APPLICABLE
        )
    if case["mode"] == "model-only":
        return OracleResult(
            case["id"], case["mode"], OracleResultKind.NON_APPLICABLE
        )
    if case["mode"] == "infrastructure-error":
        return OracleResult(
            case["id"],
            case["mode"],
            OracleResultKind.INFRASTRUCTURE,
            "corpus declares infrastructure-only evidence",
        )
    try:
        actual = _execute(case)
    except Exception as error:  # infrastructure is distinct from a semantic mismatch
        return OracleResult(
            case["id"], case["mode"], OracleResultKind.INFRASTRUCTURE, str(error)
        )
    if actual == case["expected"]:
        return OracleResult(case["id"], case["mode"], OracleResultKind.MATCH)
    return OracleResult(
        case["id"],
        case["mode"],
        OracleResultKind.MISMATCH,
        f"actual={actual!r} expected={case['expected']!r}",
    )


Transition = Callable[[DiskLifecycle, DiskLifecycleEvent], bool]


def _broken_transition(family: BrokenFamily) -> Transition:
    def transition(
        lifecycle: DiskLifecycle, event: DiskLifecycleEvent
    ) -> bool:
        if family is BrokenFamily.THRESHOLD and event.kind is EventKind.OBSERVATION:
            assert event.policy is not None
            event = replace(
                event,
                policy=replace(
                    event.policy,
                    max_disk_bytes=event.policy.max_disk_bytes + 1,
                ),
            )
        if (
            family is BrokenFamily.DISPATCH
            and event.kind is EventKind.DISPATCH_REQUESTED
            and lifecycle.stop is not None
        ):
            lifecycle.active += 1
            lifecycle.dispatched += 1
            return True
        if (
            family is BrokenFamily.CLEANUP
            and event.kind is EventKind.CLEANUP_COMPLETED
            and event.outcome is CleanupOutcome.DEFERRED
        ):
            event = replace(event, outcome=CleanupOutcome.RETAINED)
        accepted = apply_disk_lifecycle_event(lifecycle, event)
        if accepted and family is BrokenFamily.PRECEDENCE:
            latest: DiskFailure | None = None
            if event.kind is EventKind.OBSERVATION:
                assert event.policy is not None and event.value is not None
                latest = evaluate_disk_policy(event.policy, event.value)
            elif event.kind is EventKind.MEASUREMENT_FAILED:
                latest = DiskFailure(
                    code=DISK_MEASUREMENT_FAILED,
                    reason=DiskStopReason.MEASUREMENT_FAILED,
                    message=event.message,
                )
            if latest is not None:
                lifecycle.stop = latest
                lifecycle.secondary.clear()
        return accepted

    return transition


def _execute_broken(
    case: dict[str, Any], family: BrokenFamily
) -> dict[str, object]:
    lifecycle = DiskLifecycle(
        [_root(root) for root in case["initial"]["owned_roots"]]
    )
    transition = _broken_transition(family)
    accepted = True
    rejected_at: int | None = None
    for index, value in enumerate(case["events"]):
        if not transition(lifecycle, _event(value)):
            accepted = False
            rejected_at = index
            break
    return _snapshot(
        snapshot_disk_lifecycle(lifecycle),
        accepted=accepted,
        rejected_at=rejected_at,
    )


def _broken_result(
    case: dict[str, Any], family: BrokenFamily
) -> OracleResult:
    try:
        actual = _execute_broken(case, family)
    except Exception as error:
        return OracleResult(
            case["id"], case["mode"], OracleResultKind.INFRASTRUCTURE, str(error)
        )
    if actual == case["expected"]:
        return OracleResult(case["id"], case["mode"], OracleResultKind.MATCH)
    return OracleResult(
        case["id"], case["mode"], OracleResultKind.MISMATCH
    )


class FocusedMutationDiskOracleTests(unittest.TestCase):
    def test_every_python_case_matches_the_generated_oracle_once(self) -> None:
        cases = _parse_cases(CORPUS.read_text(encoding="utf-8"))
        expected_ids = {
            case["id"] for case in cases if "python" in case["implementation_targets"]
        }
        results = [_result(case) for case in cases]
        executed_ids = {
            result.case_id
            for result in results
            if result.kind is not OracleResultKind.NON_APPLICABLE
        }

        self.assertEqual(executed_ids, expected_ids)
        self.assertEqual(len(executed_ids), 21)
        failures = [
            result
            for result in results
            if result.kind
            not in {OracleResultKind.MATCH, OracleResultKind.NON_APPLICABLE}
        ]
        self.assertEqual(failures, [])
        by_mode = {
            mode: [result for result in results if result.mode == mode]
            for mode in {result.mode for result in results}
        }
        self.assertEqual(
            set(by_mode), {"strict", "internal-fixture", "model-only"}
        )
        self.assertTrue(all(by_mode.values()))
        rust_only = {
            case["id"]
            for case in cases
            if "rust" in case["implementation_targets"]
            and "python" not in case["implementation_targets"]
        }
        self.assertTrue(rust_only)
        non_applicable_ids = {
            result.case_id
            for result in results
            if result.kind is OracleResultKind.NON_APPLICABLE
        }
        self.assertEqual(non_applicable_ids, rust_only)

    def test_corpus_contract_rejects_unknown_duplicate_and_empty_metadata(self) -> None:
        first = CORPUS.read_text(encoding="utf-8").splitlines()[0]
        unknown = first.replace('"schema":1', '"schema":1,"extra":true', 1)
        with self.assertRaisesRegex(ValueError, "fields differ"):
            _parse_cases(unknown)
        duplicate_key = first.replace('"schema":1', '"schema":1,"schema":1', 1)
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            _parse_cases(duplicate_key)
        case = json.loads(first)
        case["implementation_targets"] = []
        with self.assertRaisesRegex(ValueError, "implementation targets"):
            _parse_cases(json.dumps(case, sort_keys=True))
        case["implementation_targets"] = ["python", "python"]
        with self.assertRaisesRegex(ValueError, "implementation targets"):
            _parse_cases(json.dumps(case, sort_keys=True))
        case["implementation_targets"] = ["native"]
        with self.assertRaisesRegex(ValueError, "implementation targets"):
            _parse_cases(json.dumps(case, sort_keys=True))
        case = json.loads(first)
        case["events"][0]["owned"] = True
        with self.assertRaisesRegex(ValueError, "invalid owned"):
            _parse_cases(json.dumps(case, sort_keys=True))
        case = json.loads(first)
        case["expected"]["accepted"] = False
        with self.assertRaisesRegex(ValueError, "inconsistent acceptance"):
            _parse_cases(json.dumps(case, sort_keys=True))

    def test_each_reviewed_broken_family_has_a_python_witness(self) -> None:
        cases = [
            case
            for case in _parse_cases(CORPUS.read_text(encoding="utf-8"))
            if "python" in case["implementation_targets"]
        ]
        for family in BrokenFamily:
            with self.subTest(family=family):
                results = [
                    _broken_result(case, family) for case in cases
                ]
                self.assertTrue(
                    any(result.kind is OracleResultKind.MISMATCH for result in results)
                )
                self.assertFalse(
                    any(
                        result.kind is OracleResultKind.INFRASTRUCTURE
                        for result in results
                    )
                )

    def test_process_failure_participates_in_first_stop_precedence(self) -> None:
        policy = DiskPolicy(max_disk_bytes=10, min_free_bytes=10)
        size = DiskLifecycleEvent.observation(
            policy,
            DiskObservation(owned_bytes=10, available_bytes=11),
        )
        process = DiskLifecycleEvent.process_drain_failed()

        process_first = DiskLifecycleTraceRunner([]).run([process, size]).snapshot
        self.assertEqual(
            process_first.stop.reason,
            DiskStopReason.PROCESS_FAILED,
        )
        self.assertEqual(
            [item.reason for item in process_first.secondary],
            [DiskStopReason.WORKSPACE_SIZE_EXCEEDED],
        )

        disk_first = DiskLifecycleTraceRunner([]).run([size, process]).snapshot
        self.assertEqual(
            disk_first.stop.reason,
            DiskStopReason.WORKSPACE_SIZE_EXCEEDED,
        )
        self.assertEqual(
            [item.reason for item in disk_first.secondary],
            [DiskStopReason.PROCESS_FAILED],
        )


if __name__ == "__main__":
    unittest.main()
