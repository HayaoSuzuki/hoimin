"""Generate and prove every production budget-math obligation afresh.

The manifest names the source contracts and generated obligations. Only fresh
Coma output and a complete, no-cache Why3find goal report can pass this gate.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from types import FrameType
from typing import Any

TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
)
REQUIRED_FUNCTIONS = {
    "positive_constructor",
    "positive_accessor",
    "auto_timeout",
    "projection",
    "saturating_product",
    "reconstruction",
    "shortfall",
    "NonZeroDuration_equality",
    "MutantTimeout_equality",
    "TopBudgetProjection_equality",
}
REQUIRED_CONTRACTS = {
    "positive_constructor": {"positive_constructor_exact"},
    "positive_accessor": {"positive_accessor_exact"},
    "auto_timeout": {"auto_floor_and_saturation"},
    "projection": {
        "selected_preserved",
        "jobs_preserved",
        "planned_timeout_preserved",
        "baseline_preserved",
        "remaining_preserved",
        "mathematical_ceiling",
        "effective_timeout",
        "capacity",
    },
    "saturating_product": {"both_product_saturations"},
    "reconstruction": {"exact_reconstruction"},
    "shortfall": {"strict_shortfall_for_any_struct"},
    "NonZeroDuration_equality": {"structural_equality"},
    "MutantTimeout_equality": {"structural_equality"},
    "TopBudgetProjection_equality": {"structural_equality"},
}
REQUIRED_LOGICAL = {
    "duration_max_nanos",
    "auto_timeout_nanos",
    "positive_invariant",
    "duration_view",
    "duration_deep_model",
}
REQUIRED_MODELS = {"MutantTimeout", "TopBudgetProjection"}
# The approved eight-clause manifest is a reviewed proof boundary. A semantic
# change requires reviewing the new manifest and updating this digest together.
APPROVED_MANIFEST_SHA256 = (
    "82ea2df833b6e124a68b5c5bbb282a16e6352837e33493917d616407a46b3912"
)
GENERATED = Path("verif/hoimin_budget_math_rlib")
ENVIRONMENT = Path("/recipe/tools/creusot_environment.py")
TOOLCHAIN = Path("/recipe/formal/creusot/toolchain.json")
GOAL_RE = re.compile(r"^  Goal (\S+): (\S+) \((\d+)\)$")
THEORY_RE = re.compile(r"^Theory (\S+)\.Coma: (\S+) \((\d+)\)$")
FINAL_RE = re.compile(r"^Proved \((\d+) files\) ✔$")


class ProofError(RuntimeError):
    """A required environment, scope, freshness, or proof condition failed."""


def require(condition: bool, detail: str) -> None:  # noqa: FBT001 -- assertion-style guard
    if not condition:
        raise ProofError(detail)


def normalized(value: str) -> str:
    return re.sub(r"\s+", "", value)


def trailing_comment_start(source: str, cursor: int) -> int | None:
    """Skip comments separating Rust attributes without interpreting attributes."""
    if not source[:cursor].endswith("*/"):
        line_start = source.rfind("\n", 0, cursor) + 1
        comment = source[line_start:cursor].find("//")
        return line_start + comment if comment >= 0 else None
    depth = 1
    scan = cursor - 2
    while depth:
        opening = source.rfind("/*", 0, scan)
        closing = source.rfind("*/", 0, scan)
        require(opening >= 0, "malformed block comment before function")
        if closing > opening:
            depth += 1
            scan = closing
        else:
            depth -= 1
            scan = opening
    return scan


def attached_attributes(source: str, function: str, anchor: str | None) -> list[str]:
    start = source.find(anchor) if anchor is not None else 0
    require(start >= 0, f"source anchor missing: {anchor}")
    match = re.search(rf"\bfn\s+{re.escape(function)}\s*\(", source[start:])
    if match is None:
        detail = f"source function missing: {function}"
        raise ProofError(detail)
    function_start = start + match.start()
    cursor = source.rfind("\n", 0, function_start) + 1
    require(
        "#" not in source[cursor:function_start],
        f"unsupported inline attribute before function: {function}",
    )
    attributes = []
    while True:
        while cursor and source[cursor - 1].isspace():
            cursor -= 1
        if (comment_start := trailing_comment_start(source, cursor)) is not None:
            cursor = comment_start
            continue
        if not cursor or source[cursor - 1] != "]":
            break
        end = cursor
        depth = 0
        while cursor:
            cursor -= 1
            if source[cursor] == "]":
                depth += 1
            elif source[cursor] == "[":
                depth -= 1
                if depth == 0:
                    break
        require(
            cursor > 0 and source[cursor - 1] == "#", "malformed attached attribute"
        )
        attributes.append(source[cursor - 1 : end])
        cursor -= 1
    return attributes


def check_source_scope(root: Path, manifest: dict[str, Any]) -> None:
    """Compare whole contract sets on actual named production functions."""
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    require(
        hashlib.sha256(canonical).hexdigest() == APPROVED_MANIFEST_SHA256,
        "approved scope manifest changed without review",
    )
    require(manifest.get("schema_version") == 1, "invalid obligation schema")
    require(manifest.get("package") == "hoimin-budget-math", "wrong proof package")
    require(tuple(manifest.get("targets", ())) == TARGETS, "incomplete proof targets")
    require(
        set(manifest.get("clauses", {})) == {str(n) for n in range(1, 9)},
        "missing specification clause",
    )
    functions = manifest.get("functions", [])
    require(
        {item["id"] for item in functions} == REQUIRED_FUNCTIONS,
        "missing required function",
    )
    for item in functions:
        require(
            {contract["id"] for contract in item["contracts"]}
            == REQUIRED_CONTRACTS[item["id"]],
            f"required contract set missing: {item['id']}",
        )
        source = (root / item["source"]).read_text(encoding="utf-8")
        attributes = attached_attributes(source, item["function"], item.get("anchor"))
        # Any other attached attribute may change the effective Creusot
        # contract or availability of the function. Keep this reviewed boundary
        # closed instead of trying to interpret Rust's cfg_attr grammar here.
        actual = sorted(
            normalized(attribute)
            for attribute in attributes
            if normalized(attribute) != "#[must_use]"
        )
        expected = sorted(
            normalized(f"#[cfg_attr(creusot, {clause})]")
            for clause in [
                *(contract["source"] for contract in item["contracts"]),
                *item["requires"],
            ]
        )
        require(
            actual == expected,
            f"source contract mismatch: {item['id']}",
        )
    require(
        {item["id"] for item in manifest["logical_definitions"]} == REQUIRED_LOGICAL,
        "required logical definition set missing",
    )
    for item in manifest["logical_definitions"]:
        source = (root / item["source"]).read_text(encoding="utf-8")
        require(
            normalized(item["definition"]) in normalized(source),
            f"logical definition missing: {item['id']}",
        )
    require(
        {item["type"] for item in manifest["structural_models"]} == REQUIRED_MODELS,
        "required structural model set missing",
    )
    for item in manifest["structural_models"]:
        source = (root / item["source"]).read_text(encoding="utf-8")
        match = re.search(rf"\b(?:struct|enum)\s+{re.escape(item['type'])}\b", source)
        if match is None:
            detail = f"structural model type missing: {item['type']}"
            raise ProofError(detail)
        prefix = source[max(0, match.start() - 250) : match.start()]
        require(
            normalized(item["derive"]) in normalized(prefix),
            f"structural model derive missing: {item['type']}",
        )
        if item.get("forbid_user_invariant"):
            require(
                re.search(
                    rf"\bimpl\s+Invariant\s+for\s+{re.escape(item['type'])}\b", source
                )
                is None,
                f"public projection invariant forbidden: {item['type']}",
            )


def coma_files(root: Path) -> dict[str, Path]:
    directory = root / GENERATED
    require(
        directory.is_dir() and not directory.is_symlink(),
        "generated Coma directory missing",
    )
    files = {
        path.relative_to(root).as_posix(): path for path in directory.rglob("*.coma")
    }
    require(bool(files), "no generated Coma files")
    return files


def check_generated_scope(
    root: Path, manifest: dict[str, Any], generated_after_ns: int
) -> dict[str, int]:
    """Reject stale, missing, empty, or unnamed generated obligations."""
    files = coma_files(root)
    inventory = manifest["goal_inventory"]
    require(
        len(files) >= manifest["expected_coma_files"], "too few generated Coma files"
    )
    require(set(inventory) <= set(files), "required named Coma file missing")
    require(
        sum(len(goals) for goals in inventory.values())
        >= manifest["expected_named_goals"],
        "empty goal inventory",
    )
    for name, path in files.items():
        require(
            path.is_file() and not path.is_symlink(), f"invalid generated file: {name}"
        )
        require(path.stat().st_size > 0, f"empty generated Coma file: {name}")
        require(
            path.stat().st_mtime_ns >= generated_after_ns,
            f"old generated Coma file: {name}",
        )
    for function in manifest["functions"]:
        name = function["coma"]
        content = files[name].read_text(encoding="utf-8")
        for contract in function["contracts"]:
            marker = f"[@expl:{contract['generated_marker']}]"
            require(
                marker in content,
                f"generated contract marker missing: {name}: {marker}",
            )
        if refinement := function.get("refinement"):
            require(refinement["coma"] in files, f"refinement missing: {name}")
    return {
        "files": len(files),
        "expected_named_goals": manifest["expected_named_goals"],
    }


def check_proof_status(
    root: Path, manifest: dict[str, Any], log: str
) -> dict[str, int]:
    """Require one successful result for every goal in every generated file."""
    files = coma_files(root)
    theories: dict[str, dict[str, Any]] = {}
    current: str | None = None
    final = None
    for line in log.splitlines():
        if match := THEORY_RE.fullmatch(line):
            theory, status, count = match.groups()
            name = str(theory).replace(".", "/") + ".coma"
            require(name not in theories, f"duplicate theory: {name}")
            require(status == "✔", f"unproved theory: {name}: {status}")
            theories[name] = {"count": int(count), "goals": set()}
            current = name
        elif match := GOAL_RE.fullmatch(line):
            goal, status, count = match.groups()
            if current is None:
                detail = f"goal without theory: {goal}"
                raise ProofError(detail)
            require(
                status == "✔" and int(count) > 0, f"unproved goal: {current}: {goal}"
            )
            require(
                goal not in theories[current]["goals"],
                f"duplicate goal: {current}: {goal}",
            )
            theories[current]["goals"].add(goal)
        elif match := FINAL_RE.fullmatch(line):
            require(final is None, "duplicate final proof status")
            final = int(match.group(1))
    require(
        set(theories) == set(files), "proof did not cover every generated Coma file"
    )
    require(final == len(files), "missing or incomplete final proof status")
    for name, theory in theories.items():
        require(
            theory["count"] == len(theory["goals"]) and theory["count"] > 0,
            f"missing named goals: {name}",
        )
        require(
            set(manifest["goal_inventory"].get(name, [])) <= theory["goals"],
            f"required goal missing: {name}",
        )
    return {
        "files": len(files),
        "goals": sum(len(item["goals"]) for item in theories.values()),
    }


def owned_session_groups(session_id: int) -> set[int]:
    """Find Linux process groups in the session created for one runner command."""
    groups = set()
    for entry in Path("/proc").iterdir():
        if not entry.name.isdecimal():
            continue
        try:
            stat = (entry / "stat").read_text(encoding="utf-8")
            fields = stat.rsplit(") ", 1)[1].split()
            if int(fields[3]) == session_id and fields[0] != "Z":
                groups.add(int(fields[2]))
        except (OSError, IndexError, ValueError):
            continue  # A process may exit while /proc is scanned.
    return groups


def kill_group(group: int) -> None:
    with contextlib.suppress(ProcessLookupError):
        getattr(os, "killpg")(group, getattr(signal, "SIGKILL"))  # noqa: B009 -- Linux-only API


def terminate_session(session_id: int) -> None:
    """Kill only this command's session, including GNU timeout's child group."""
    if sys.platform != "linux":
        kill_group(session_id)
        return
    require(
        getattr(os, "getsid")(0) != session_id,  # noqa: B009 -- Linux-only API
        "refusing to terminate the runner session",
    )
    # Stop the command dispatcher before enumerating GNU timeout's child group.
    kill_group(session_id)
    for _ in range(3):
        groups = owned_session_groups(session_id) - {session_id}
        for group in sorted(groups):
            kill_group(group)
        if not groups:
            return


def run_command(
    command: list[str], *, cwd: Path, log_path: Path, deadline: float
) -> dict[str, Any]:
    """Run one step under the deadline, cleaning up its session on interruption."""
    remaining = deadline - time.monotonic()
    require(remaining > 0, "overall proof deadline exhausted")
    before = time.monotonic()
    time_file = log_path.with_suffix(".time")
    measured = sys.platform.startswith("linux") and Path("/usr/bin/time").is_file()
    actual_command = (
        ["/usr/bin/time", "-v", "-o", str(time_file), *command] if measured else command
    )
    stopping = False
    launching = True
    pending_stop = False

    def request_stop(_signum: int, _frame: FrameType | None) -> None:
        nonlocal stopping, pending_stop
        if stopping:
            return
        if launching:
            pending_stop = True
            return
        stopping = True
        detail = "proof runner interrupted by SIGTERM"
        raise ProofError(detail)

    old_handler = signal.getsignal(signal.SIGTERM)
    process = None
    try:
        signal.signal(signal.SIGTERM, request_stop)
        with log_path.open("wb") as stream:
            process = subprocess.Popen(  # noqa: S603 -- fixed runner commands, no shell
                actual_command,
                cwd=cwd,
                stdout=stream,
                stderr=subprocess.STDOUT,
                start_new_session=True,
                env=os.environ
                | {
                    "CARGO_BUILD_JOBS": "2",
                    "CARGO_TARGET_DIR": "/tmp/hoimin-issue999-target",  # noqa: S108 -- task-owned directory
                },
            )
            launching = False
            require(not pending_stop, "proof runner interrupted by SIGTERM")
            code = process.wait(timeout=remaining)
    except BaseException as error:
        stopping = True  # Repeated SIGTERM must not interrupt owned cleanup.
        if process is not None and process.returncode is None:
            terminate_session(process.pid)
            process.wait()
        if isinstance(error, subprocess.TimeoutExpired):
            detail = f"overall proof deadline exceeded: {command[0]}"
            raise ProofError(detail) from error
        raise
    finally:
        signal.signal(signal.SIGTERM, old_handler)
    elapsed = time.monotonic() - before
    require(code == 0, f"command failed ({code}): {' '.join(command)}; see {log_path}")
    peak_rss = None
    if measured:
        match = re.search(
            r"Maximum resident set size \(kbytes\):\s*(\d+)",
            time_file.read_text(encoding="utf-8"),
        )
        if match is None:
            detail = f"missing per-command memory measurement: {time_file}"
            raise ProofError(detail)
        peak_rss = int(match.group(1))
    return {
        "command": command,
        "elapsed_seconds": round(elapsed, 3),
        "peak_process_or_child_rss_kib": peak_rss,
        "log": str(log_path),
    }


def input_hashes(root: Path, manifest_path: Path) -> dict[str, str]:
    files = [
        root / "Cargo.toml",
        root / "Cargo.lock",
        root / "why3find.json",
        root / "crates/hoimin-budget-math/Cargo.toml",
        manifest_path,
    ]
    files += sorted((root / "crates/hoimin-budget-math/src").glob("*.rs"))
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in files}


def proof_preflight(
    root: Path, output: Path, deadline: float, label: str
) -> dict[str, Any]:
    return run_command(
        [
            sys.executable,
            str(ENVIRONMENT),
            "check",
            "--manifest",
            str(TOOLCHAIN),
            "--project-root",
            str(root),
        ],
        cwd=root,
        log_path=output / f"{label}-preflight.log",
        deadline=deadline,
    )


def install_strategies(root: Path, manifest: dict[str, Any], target: str) -> int:
    source = root / manifest["strategy_roots"][target] / GENERATED.name
    require(source.is_dir(), f"strategy directory missing: {target}")
    strategies = list(source.rglob("proof.json"))
    require(
        len(strategies) >= manifest["expected_coma_files"],
        f"strategies incomplete: {target}",
    )
    for strategy in strategies:
        destination = root / GENERATED / strategy.relative_to(source)
        coma = destination.parent.with_suffix(".coma")
        require(coma.is_file(), f"strategy scope missing: {coma}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(strategy, destination)
    return len(strategies)


def verify(
    root: Path, output: Path, manifest: dict[str, Any], deadline: float
) -> list[dict[str, Any]]:
    check_source_scope(root, manifest)
    original_hashes = input_hashes(root, root / "formal/creusot/obligations.json")
    require(
        ENVIRONMENT.is_file() and TOOLCHAIN.is_file(),
        "pinned proof environment missing",
    )
    results = []
    for target in TARGETS:
        target_output = output / target
        target_output.mkdir()
        steps = [proof_preflight(root, target_output, deadline, "generation")]
        steps.append(
            run_command(
                ["cargo", "clean", "-p", "hoimin-budget-math", "--target", target],
                cwd=root,
                log_path=target_output / "clean.log",
                deadline=deadline,
            )
        )
        generated = root / GENERATED
        require(not generated.is_symlink(), "generated output is a symlink")
        if generated.exists():
            shutil.rmtree(generated)
        generation_start_ns = time.time_ns()
        steps.append(
            run_command(
                [
                    "cargo",
                    "creusot",
                    "-p",
                    "hoimin-budget-math",
                    "--only=coma",
                    "--span-mode",
                    "absolute",
                    "-j",
                    "2",
                    "--",
                    "--lib",
                    "--locked",
                    # The proof compiler has its own verified nightly pin.
                    "--ignore-rust-version",
                    "--target",
                    target,
                ],
                cwd=root,
                log_path=target_output / "generation.log",
                deadline=deadline,
            )
        )
        generated_scope = check_generated_scope(root, manifest, generation_start_ns)
        require(
            input_hashes(root, root / "formal/creusot/obligations.json")
            == original_hashes,
            "proof inputs changed during generation",
        )
        strategy_count = install_strategies(root, manifest, target)
        steps.append(proof_preflight(root, target_output, deadline, "proof"))
        steps.append(
            run_command(
                [
                    "cargo",
                    "creusot",
                    "-p",
                    "hoimin-budget-math",
                    "--only=prove",
                    "--no-cache",
                    "--replay",
                    "-j",
                    "2",
                    "--why3find-arg=--goals",
                ],
                cwd=root,
                log_path=target_output / "proof.log",
                deadline=deadline,
            )
        )
        status = check_proof_status(
            root, manifest, (target_output / "proof.log").read_text(encoding="utf-8")
        )
        require(
            input_hashes(root, root / "formal/creusot/obligations.json")
            == original_hashes,
            "proof inputs changed during proof",
        )
        results.append(
            {
                "target": target,
                "generated": generated_scope,
                "proved": status,
                "strategies": strategy_count,
                "steps": steps,
            }
        )
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path.cwd().resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    report: dict[str, Any] = {
        "status": "failed",
        "targets": [],
        "deadline_seconds": 1800,
    }
    try:
        manifest = json.loads(
            (root / "formal/creusot/obligations.json").read_text(encoding="utf-8")
        )
        report["targets"] = verify(root, output, manifest, started + 1800)
        report["status"] = "passed"
    except (ProofError, OSError, KeyError, ValueError, TypeError) as error:
        report["error"] = str(error)
        sys.stderr.write(f"Creusot verification failed: {error}\n")
    finally:
        report["elapsed_seconds"] = round(time.monotonic() - started, 3)
        report["output_disk_bytes"] = sum(
            path.stat().st_size for path in output.rglob("*") if path.is_file()
        )
        (output / "summary.json").write_text(
            json.dumps(report, indent=2) + "\n", encoding="utf-8"
        )
    return int(report["status"] != "passed")


if __name__ == "__main__":
    raise SystemExit(main())
