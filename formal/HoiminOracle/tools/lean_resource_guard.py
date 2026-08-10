"""Run one Lean command with wall-clock and process-tree RSS limits."""

import argparse
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

TIMEOUT_EXIT = 124
RSS_LIMIT_EXIT = 125
MONITOR_ERROR_EXIT = 126
MONITOR_EXCEPTIONS = (OSError, RuntimeError, ValueError, subprocess.SubprocessError)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout-seconds", type=float, required=True)
    parser.add_argument("--rss-limit-mib", type=int, required=True)
    parser.add_argument("--sample-ms", type=int, required=True)
    parser.add_argument("--stats", type=Path, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    arguments = parser.parse_args()
    if arguments.command[:1] == ["--"]:
        arguments.command = arguments.command[1:]
    if not arguments.command:
        parser.error("a command is required after --")
    for name in ("timeout_seconds", "rss_limit_mib", "sample_ms"):
        if getattr(arguments, name) <= 0:
            parser.error(f"--{name.replace('_', '-')} must be positive")
    return arguments


def process_table() -> dict[int, tuple[int, int, int]]:
    completed = subprocess.run(
        ["ps", "-axo", "pid=,ppid=,pgid=,rss="],
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"ps failed with exit code {completed.returncode}")

    rows: dict[int, tuple[int, int, int]] = {}
    for line in completed.stdout.splitlines():
        fields = line.split()
        if len(fields) != 4:
            continue
        pid, parent_pid, process_group_id, rss_kib = map(int, fields)
        rows[pid] = (parent_pid, process_group_id, rss_kib)
    return rows


def process_tree_rss_kib(root_pid: int, rows: dict[int, tuple[int, int, int]]) -> int:
    children: dict[int, list[int]] = {}
    for pid, (parent_pid, _, _) in rows.items():
        children.setdefault(parent_pid, []).append(pid)

    pending = [root_pid]
    process_ids: set[int] = set()
    while pending:
        process_id = pending.pop()
        if process_id in process_ids:
            continue
        process_ids.add(process_id)
        pending.extend(children.get(process_id, ()))
    return sum(rows[process_id][2] for process_id in process_ids if process_id in rows)


def process_group_members(
    process_group_id: int, rows: dict[int, tuple[int, int, int]]
) -> list[int]:
    return [pid for pid, (_, group, _) in rows.items() if group == process_group_id]


def process_group_exists(process_group_id: int) -> bool:
    try:
        os.killpg(process_group_id, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def wait_for_process_group_exit(
    process: subprocess.Popen[bytes], timeout: float
) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        process.poll()
        if not process_group_exists(process.pid):
            return True
        time.sleep(0.025)
    return not process_group_exists(process.pid)


def terminate_process_group(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass

    if not wait_for_process_group_exit(process, 1.0):
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        wait_for_process_group_exit(process, 1.0)
    process.wait()


def write_stats(path: Path, stats: dict[str, int | str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, delete=False
    ) as temporary_file:
        json.dump(stats, temporary_file, sort_keys=True)
        temporary_file.write("\n")
        temporary_path = Path(temporary_file.name)
    temporary_path.replace(path)


def run(arguments: argparse.Namespace) -> int:
    started_at = time.monotonic()
    peak_rss_kib = 0
    timeout_ms = round(arguments.timeout_seconds * 1000)
    rss_limit_kib = arguments.rss_limit_mib * 1024
    if os.name != "posix" or not hasattr(os, "killpg"):
        try:
            write_stats(
                arguments.stats,
                {
                    "schema": 1,
                    "reason": "monitor_error",
                    "exit_code": MONITOR_ERROR_EXIT,
                    "elapsed_ms": 0,
                    "peak_rss_kib": 0,
                    "rss_limit_kib": rss_limit_kib,
                    "timeout_ms": timeout_ms,
                },
            )
        except OSError as error:
            print(f"failed to write resource stats: {error}", file=sys.stderr)
        return MONITOR_ERROR_EXIT
    reason = "monitor_error"
    public_exit_code = MONITOR_ERROR_EXIT
    process: subprocess.Popen[bytes] | None = None

    try:
        process = subprocess.Popen(arguments.command, start_new_session=True)
        while True:
            child_exit_code = process.poll()
            elapsed_seconds = time.monotonic() - started_at
            if child_exit_code is not None:
                rows = process_table()
                if process_group_members(process.pid, rows):
                    terminate_process_group(process)
                    reason = "monitor_error"
                    public_exit_code = MONITOR_ERROR_EXIT
                else:
                    reason = "child_exit"
                    public_exit_code = child_exit_code
                break
            if elapsed_seconds >= arguments.timeout_seconds:
                reason = "timeout"
                public_exit_code = TIMEOUT_EXIT
                terminate_process_group(process)
                break

            rows = process_table()
            rss_kib = process_tree_rss_kib(process.pid, rows)
            peak_rss_kib = max(peak_rss_kib, rss_kib)
            if rss_kib >= rss_limit_kib:
                reason = "rss_limit"
                public_exit_code = RSS_LIMIT_EXIT
                terminate_process_group(process)
                break
            time.sleep(arguments.sample_ms / 1000)
    except MONITOR_EXCEPTIONS:
        if process is not None:
            terminate_process_group(process)

    elapsed_ms = round((time.monotonic() - started_at) * 1000)
    stats: dict[str, int | str] = {
        "schema": 1,
        "reason": reason,
        "exit_code": public_exit_code,
        "elapsed_ms": elapsed_ms,
        "peak_rss_kib": peak_rss_kib,
        "rss_limit_kib": rss_limit_kib,
        "timeout_ms": timeout_ms,
    }
    try:
        write_stats(arguments.stats, stats)
    except OSError as error:
        print(f"failed to write resource stats: {error}", file=sys.stderr)
        return MONITOR_ERROR_EXIT
    return public_exit_code


def main() -> int:
    return run(parse_arguments())


if __name__ == "__main__":
    raise SystemExit(main())
