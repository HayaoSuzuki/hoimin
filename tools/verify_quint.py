"""Bounded TLC verification and reproducible ITF oracles for stateful contracts.

The existing POSIX process-tree guard owns termination and resource accounting.
Only an explicit semantic violation counts as a successful negative control.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[1]
FORMAL = ROOT / "formal/quint"
GUARD = ROOT / "formal/HoiminOracle/tools/lean_resource_guard.py"
QUINT = FORMAL / "node_modules/.bin/quint"


@dataclass(frozen=True)
class Model:
    source: str
    main: str
    prefix: str
    mutant: str | None = None
    scenarios: tuple[str, ...] = ()
    scenario_steps: int = 12
    # TLC may omit input-history fields that do not affect transitions or Safety.
    verification_step: str = "step"


MODELS = (
    Model(
        "copy_preflight.qnt",
        "CopyPreflight",
        "copy_preflight",
        "CopyPreflightAcceptsWrongCount",
        ("ordinaryStep", "freshRetryStep", "resumeRetryStep", "emptyStep"),
    ),
    Model(
        "worker_grant.qnt",
        "WorkerGrant",
        "worker_grant",
        "WorkerGrantIgnoresReservation",
        (
            "reverseStep",
            "wrongReservationStep",
            "wrongWorkerStep",
            "failureStep",
            "deadlineStep",
        ),
    ),
    Model(
        "session_startup.qnt",
        "SessionStartup",
        "session_startup",
        "SessionStartupSkipsReportAck",
        (
            "disabledStep",
            "resumedStep",
            "fallbackStep",
            "missingFingerprintStep",
            "deadlineLoadStep",
            "failedBeginStep",
        ),
    ),
    Model(
        "session_finish_ack.qnt",
        "SessionFinishAck",
        "session_finish_ack",
        "SessionFinishAckAcceptsMismatch",
        (
            "cleanStep",
            "wrongFlagStep",
            "wrongRunStep",
            "sessionFailureStep",
            "reportFailureStep",
            "freezeStep",
        ),
    ),
    Model(
        "integrity_checkpoint.qnt",
        "IntegrityCheckpointProtocol",
        "integrity_checkpoint",
        "IntegrityCheckpointAcceptsWrongFinal",
        ("bothMismatchStep", "deadlineStep", "invalidFinalStep"),
    ),
    Model(
        "process_result.qnt",
        "ProcessResult",
        "process_result",
        "ProcessResultIgnoresOutputTimeout",
        (
            "outputTimeoutStep",
            "processTimeoutStep",
            "memoryStep",
            "identitiesStep",
            "cancelPersistStep",
            "cancelFinishStep",
        ),
    ),
    Model(
        "worker_failure.qnt",
        "WorkerFailure",
        "worker_failure",
        "WorkerFailureDropsResult",
        (
            "applyFailureStep",
            "startFailureStep",
            "runFailureStep",
            "finishFailureStep",
            "reportStartFailureStep",
            "reportFinishFailureStep",
        ),
    ),
    Model(
        "mutant_admission.qnt",
        "MutantAdmission",
        "mutant_admission",
        "MutantAdmissionChargesAtStart",
        ("earlyOverflowStep", "exactStep", "realFirstStep"),
    ),
    Model(
        "explicit_stream.qnt",
        "ExplicitStream",
        "explicit_stream",
        "ExplicitStreamReappliesDuplicate",
        ("filteredStep", "partialStep", "workerStep"),
    ),
    Model(
        "analysis_spool.qnt",
        "AnalysisSpool",
        "analysis_spool",
        "AnalysisSpoolReadsTruncation",
        (
            "ordinaryTruncatedStep",
            "explicitTruncatedStep",
            "ordinaryEmptyStep",
            "ordinaryNoSpoolStep",
            "explicitRetryStep",
            "deadlineStep",
        ),
    ),
    Model(
        "verify_budget_gate.qnt",
        "VerifyBudgetGate",
        "verify_budget_gate",
        "VerifyBudgetGateWarnsAtEquality",
        (
            "belowStep",
            "equalStep",
            "aboveStep",
            "cancelWarningStep",
            "deadlineObservationStep",
            "failWarningStep",
        ),
    ),
    Model(
        "analysis_diagnostics.qnt",
        "AnalysisDiagnostics",
        "analysis_diagnostics",
        "AnalysisDiagnosticsBypassesOutput",
        (
            "fifoStep",
            "repeatedStep",
            "truncatedStep",
            "noSpoolStep",
            "cancelStep",
            "failureStep",
        ),
    ),
    Model(
        "baseline_restoration.qnt",
        "BaselineRestoration",
        "baseline_restoration",
        "BaselineRestorationUsesOutput",
        (
            "outputFirstStep",
            "resetFirstStep",
            "rejectionsStep",
            "failureStep",
            "stopStep",
        ),
    ),
    Model(
        "ordered_discovery.qnt",
        "OrderedDiscovery",
        "ordered_discovery",
        "OrderedDiscoveryWrongOrder",
        ("orderedStep", "missingStep"),
    ),
    Model(
        "ordered_drain.qnt",
        "OrderedDrain",
        "ordered_drain",
        "OrderedDrainInventsCandidate",
        (
            "partialStopStep",
            "repeatedIndexedStopStep",
            "indexedFailureStep",
            "trailingFailureStep",
        ),
        scenario_steps=16,
    ),
    Model(
        "resume_candidate.qnt",
        "ResumeCandidate",
        "resume_candidate",
        "ResumeCandidateRerunsCached",
        (
            "reuseStep",
            "rerunStep",
            "cancelledLookupStep",
            "cancelledStartStep",
            "cancelledFinishStep",
            "failedStartStep",
            "failedFinishStep",
            "reportIoStep",
        ),
    ),
    Model(
        "disk_observation.qnt",
        "DiskObservation",
        "disk_observation",
        "DiskObservationReplace",
        ("priorityStep", "duplicateStep", "failureStep"),
    ),
    Model(
        "workers.qnt",
        "Workers",
        "workers",
        scenarios=(
            "reverseStep",
            "completedStopStep",
            "startStopStep",
            "runningStopStep",
            "timeoutStep",
            "timeoutStopStep",
        ),
        scenario_steps=24,
    ),
    Model(
        "workers.qnt",
        "WorkersSession",
        "workers_session",
        "WorkersDoubleCount",
        (
            "reverseStep",
            "completedStopStep",
            "startStopStep",
            "runningStopStep",
            "timeoutStep",
            "timeoutStopStep",
        ),
        scenario_steps=24,
    ),
    Model(
        "stop.qnt",
        "StopEarly",
        "stop_early",
        scenarios=(
            "cleanStep",
            "diskStep",
            "failureStep",
            "terminalStep",
            "finalFailureStep",
        ),
    ),
    Model(
        "stop.qnt",
        "StopCopy",
        "stop_copy",
        scenarios=(
            "cleanStep",
            "diskStep",
            "failureStep",
            "terminalStep",
            "finalFailureStep",
        ),
    ),
    Model(
        "stop.qnt",
        "StopSession",
        "stop_session",
        "StopPrematureSession",
        (
            "cleanStep",
            "diskStep",
            "failureStep",
            "terminalStep",
            "finalFailureStep",
            "reserveEvidenceStep",
            "measurementEvidenceStep",
            "diagnosticEvidenceStep",
        ),
    ),
    Model("session.qnt", "Session", "session", "SessionOverwrite", ("recoveryStep",)),
    Model(
        "session_selection.qnt",
        "SessionSelection",
        "session_selection",
        "SessionSelectionOldest",
        ("latestStep", "fallbackStep", "freshPriorityStep"),
    ),
    Model(
        "session_ownership.qnt",
        "SessionOwnership",
        "session_ownership",
        "SessionOwnershipShared",
        ("handoffStep",),
    ),
    Model("budget.qnt", "Budget", "trace", "BudgetOverdraft"),
    Model("budget.qnt", "BudgetExtended", "extended"),
    Model(
        "report.qnt",
        "Report",
        "report",
        "ReportEarlyFinish",
        ("interleavedStep", "gappedStep"),
    ),
    Model(
        "completion.qnt",
        "Completion",
        "completion",
        "CompletionResurrectRetired",
        ("gapStep", "wrongKindStep", "cleanupStep"),
    ),
    Model(
        "disk.qnt",
        "DiskLifecycle",
        "disk",
        "DiskBadDelivery",
        ("cleanStep", "deferredStep", "processEvidenceStep", "cleanupDiagnosticStep"),
        verification_step="verifyStep",
    ),
    Model(
        "disk.qnt",
        "DiskExecutionOnly",
        "disk_execution",
        scenarios=("cleanStep",),
        verification_step="verifyStep",
    ),
    Model(
        "disk.qnt",
        "DiskNoRoots",
        "disk_empty",
        scenarios=("cleanStep",),
        verification_step="verifyStep",
    ),
    Model(
        "ownership.qnt",
        "Ownership",
        "ownership",
        "OwnershipConflict",
        ("reacquireStep",),
    ),
    Model(
        "finalization.qnt",
        "Finalization",
        "finalization",
        "FinalizationUnjoined",
        (
            "cleanStep",
            "refusedStep",
            "unjoinedStep",
            "availableStep",
            "pairedAvailableStep",
            "missingAvailableStep",
        ),
    ),
)


def validate_result(code: int, output: str, *, counterexample: bool) -> None:
    if counterexample:
        if code != 1 or "[violation]" not in output or "counterexample" not in output:
            msg = f"Expected a semantic counterexample, got exit {code}:\n{output}"
            raise RuntimeError(msg)
    elif code != 0:
        msg = f"Quint verification failed with exit {code}:\n{output}"
        raise RuntimeError(msg)


def guarded(arguments: list[str], directory: Path, name: str) -> tuple[int, str]:
    env = {**os.environ, "NO_COLOR": "1", "JAVA_TOOL_OPTIONS": "-Xmx1G"}
    result = subprocess.run(  # noqa: S603 -- repository-owned executable and argv, no shell
        [
            sys.executable,
            str(GUARD),
            "--timeout-seconds",
            "20",
            "--rss-limit-mib",
            "2048",
            "--sample-ms",
            "50",
            "--stats",
            str(directory / f"{name}.stats.json"),
            "--",
            str(QUINT),
            *arguments,
        ],
        cwd=directory,
        env=env,
        capture_output=True,
        text=True,
        check=False,
        timeout=25,
    )
    output = result.stdout + result.stderr
    (directory / f"{name}.log").write_text(output, encoding="utf-8")
    print(output, end="")  # noqa: T201 -- developer command diagnostics
    return result.returncode, output


def canonical_trace(path: Path) -> str:
    """Remove generator timestamps/absolute paths, preserving all expected states."""
    trace = json.loads(path.read_text(encoding="utf-8"))
    trace["#meta"] = {"format": "ITF", "status": trace["#meta"]["status"]}
    return json.dumps(trace, sort_keys=True, indent=2) + "\n"


def verify_model(
    specification: Model, build: Path, *, update: bool, simulate_only: bool
) -> None:
    model = str(FORMAL / specification.source)
    code, output = guarded(
        ["typecheck", model], build, f"{specification.main}-typecheck"
    )
    validate_result(code, output, counterexample=False)
    if not simulate_only:
        modules = (specification.main,)
        if specification.mutant is not None:
            modules = (*modules, specification.mutant)
        for module in modules:
            code, output = guarded(
                [
                    "verify",
                    model,
                    "--backend=tlc",
                    "--apalache-version=0.62.4",
                    f"--tlc-config={FORMAL / 'tlc.json'}",
                    f"--main={module}",
                    f"--step={specification.verification_step}",
                    "--invariant=Safety",
                    "--verbosity=3",
                ],
                build,
                module,
            )
            validate_result(code, output, counterexample=module != specification.main)
    simulate_model(specification, build, update=update)
    for scenario in specification.scenarios:
        simulate_model(specification, build, update=update, step=scenario)


def simulate_model(
    specification: Model, build: Path, *, update: bool, step: str = "step"
) -> None:
    """Use model actions for both random and deliberately scheduled inputs."""
    model = str(FORMAL / specification.source)
    count = 4 if step == "step" else 1
    prefix = (
        specification.prefix if step == "step" else f"{specification.prefix}_{step}"
    )
    with TemporaryDirectory(dir=build) as temporary:
        directory = Path(temporary)
        code, output = guarded(
            [
                "run",
                model,
                "--backend=typescript",
                f"--main={specification.main}",
                "--invariant=Safety",
                "--seed=1172",
                f"--max-steps={24 if step == 'step' else specification.scenario_steps}",
                f"--max-samples={20 if step == 'step' else 1}",
                f"--n-traces={count}",
                f"--step={step}",
                f"--out-itf={directory}/trace_{{seq}}.itf.json",
                "--verbosity=1",
            ],
            build,
            f"{specification.main}-{step}-simulation",
        )
        validate_result(code, output, counterexample=False)
        expected_paths = [directory / f"trace_{i}.itf.json" for i in range(count)]
        for index, source in enumerate(expected_paths):
            content = canonical_trace(source)
            target = FORMAL / "traces" / f"{prefix}_{index}.itf.json"
            if update:
                target.parent.mkdir(exist_ok=True)
                target.write_text(content, encoding="utf-8")
            elif not target.is_file() or target.read_text(encoding="utf-8") != content:
                msg = (
                    f"Stale Quint oracle: {target}; run tools/verify_quint.py --update"
                )
                raise RuntimeError(msg)


def verify(*, update: bool, simulate_only: bool = False) -> None:
    if not QUINT.is_file():
        msg = (
            "Install the pinned tool first: "
            "npm ci --prefix formal/quint --ignore-scripts"
        )
        raise RuntimeError(msg)
    build = FORMAL / "_build"
    build.mkdir(exist_ok=True)
    for specification in MODELS:
        verify_model(specification, build, update=update, simulate_only=simulate_only)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--update", action="store_true", help="replace committed ITF oracles"
    )
    parser.add_argument(
        "--simulate-only",
        action="store_true",
        help="skip TLC for local oracle development",
    )
    arguments = parser.parse_args()
    verify(update=arguments.update, simulate_only=arguments.simulate_only)


if __name__ == "__main__":
    main()
