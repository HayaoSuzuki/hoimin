from collections.abc import Sequence
import io
import json
from pathlib import Path
import re
import os
import stat
from typing import cast

from .filesystem import (
    CreateDisposition,
    EntryKind,
    FileAccess,
    SharePolicy,
    SecurityDomain,
    default_filesystem_backend,
)
from .model import Candidate, CandidateState, CommandRecord, RunState
from .lease import (
    _OwnedDescriptor,
    _close_capability_retry,
    validate_reported_path,
)


SUPPORTED_CARGO_MUTANTS_VERSION = "27.1.0"
MAX_INVENTORY_ENTRIES = 10_000
MAX_JSON_NODES = 100_000
MAX_JSON_DEPTH = 64
MAX_JSON_STRING_BYTES = 16 * 1024
_OUTER_GUARD_DEPTH_FIXTURES = (
    "workspace::disk::tests::exact_depth_bound_uses_at_most_one_hundred_twenty_nine_directory_handles",
    "workspace::disk::tests::directory_handle_bound_child",
    "workspace::disk::tests::fresh_process_observes_the_real_directory_descriptor_bound",
    "workspace::disk::tests::rejects_a_tree_deeper_than_the_bound",
    "workspace::owned::tests::cleanup_removes_a_tree_deeper_than_the_meter_limit",
    "workspace::root::tests::post_order_removal_handles_a_tree_at_the_supported_depth",
    "workspace::root::tests::post_order_removal_reports_the_shared_depth_limit",
    "cleanup_releases_state_when_the_temporary_wrapper_was_already_removed",
    "reset_preserves_depth_error_while_discard_cleanup_is_pending",
    "reset_handles_a_tree_at_the_supported_depth",
    "reset_reports_a_depth_error_beyond_the_supported_depth",
    "cleanup_reports_the_same_depth_error_as_reset",
)


def _raise_bounded_descriptor_close(
    descriptor: _OwnedDescriptor,
    label: str,
    primary: BaseException | None = None,
) -> None:
    details = descriptor.close_retry(label)
    if primary is not None:
        for detail in details:
            primary.add_note(detail)
        return
    if details:
        raise OSError("; ".join(details))


def _open_windows_bounded_regular(path: Path) -> _OwnedDescriptor:
    target = Path(os.path.abspath(path))
    descriptor = _OwnedDescriptor()
    backend = default_filesystem_backend()
    parent = None
    regular = None
    try:
        parent = backend.open_root(
            target.parent,
            SharePolicy.PINNED,
            SecurityDomain.CALLER,
        )
        regular = backend.open_file(
            parent,
            target.name,
            access=FileAccess.READ,
            disposition=CreateDisposition.OPEN_EXISTING,
            share_policy=SharePolicy.PINNED,
        )
        if (
            regular.kind is not EntryKind.REGULAR
            or regular.filesystem != parent.filesystem
        ):
            raise OSError(f"bounded input identity changed: {path}")

        parent_details = _close_capability_retry(parent, "bounded input parent")
        if parent_details:
            raise OSError("; ".join(parent_details))
        if parent.is_open:
            raise OSError("bounded input parent close failed")

        descriptor.adopt(
            regular.detach_to_fd(
                os.O_RDONLY | getattr(os, "O_BINARY", 0)
            )
        )
        return descriptor
    except BaseException as primary:
        details: list[str] = []
        if regular is not None and regular.is_open:
            details.extend(_close_capability_retry(regular, "bounded input file"))
        if parent is not None and parent.is_open:
            details.extend(_close_capability_retry(parent, "bounded input parent"))
        details.extend(descriptor.close_retry("bounded input descriptor"))
        for detail in details:
            primary.add_note(detail)
        raise


def read_bounded_regular(path: Path, capacity: int) -> bytes:
    if capacity <= 0:
        raise ValueError("bounded read capacity must be positive")
    if os.name == "nt":
        descriptor = _open_windows_bounded_regular(path)
        try:
            metadata = os.fstat(descriptor.fd)
            if not stat.S_ISREG(metadata.st_mode):
                raise ValueError(f"bounded input is not a regular file: {path}")
            if metadata.st_size > capacity:
                raise ValueError(f"bounded input exceeds {capacity} bytes: {path}")
            value = bytearray()
            while len(value) <= capacity:
                chunk = os.read(
                    descriptor.fd,
                    min(64 * 1024, capacity + 1 - len(value)),
                )
                if not chunk:
                    break
                value.extend(chunk)
            if len(value) > capacity:
                raise ValueError(f"bounded input exceeds {capacity} bytes: {path}")
            result = bytes(value)
        except BaseException as primary:
            _raise_bounded_descriptor_close(
                descriptor,
                "bounded input descriptor",
                primary,
            )
            raise
        _raise_bounded_descriptor_close(descriptor, "bounded input descriptor")
        return result

    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    fd = os.open(path, flags)
    try:
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError(f"bounded input is not a regular file: {path}")
        if metadata.st_size > capacity:
            raise ValueError(f"bounded input exceeds {capacity} bytes: {path}")
        value = bytearray()
        while len(value) <= capacity:
            chunk = os.read(fd, min(64 * 1024, capacity + 1 - len(value)))
            if not chunk:
                break
            value.extend(chunk)
        if len(value) > capacity:
            raise ValueError(f"bounded input exceeds {capacity} bytes: {path}")
        return bytes(value)
    finally:
        os.close(fd)


def read_bounded_regular_json(path: Path, capacity: int) -> object:
    try:
        text = read_bounded_regular(path, capacity).decode("utf-8", errors="strict")
    except UnicodeError as error:
        raise ValueError(f"bounded JSON is not UTF-8: {path}") from error
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid bounded JSON {path}: {error}") from error
    _validate_json_shape(value)
    return value


def _validate_json_shape(value: object) -> None:
    nodes = 0
    stack: list[tuple[object, int]] = [(value, 0)]
    while stack:
        item, depth = stack.pop()
        nodes += 1
        if nodes > MAX_JSON_NODES:
            raise ValueError(f"bounded JSON exceeds {MAX_JSON_NODES} nodes")
        if depth > MAX_JSON_DEPTH:
            raise ValueError(f"bounded JSON exceeds depth {MAX_JSON_DEPTH}")
        if isinstance(item, str):
            if len(item.encode("utf-8")) > MAX_JSON_STRING_BYTES:
                raise ValueError(
                    f"bounded JSON string exceeds {MAX_JSON_STRING_BYTES} bytes"
                )
        elif isinstance(item, list):
            stack.extend((child, depth + 1) for child in item)
        elif isinstance(item, dict):
            for key, child in item.items():
                if not isinstance(key, str):
                    raise ValueError("bounded JSON object key is not a string")
                if len(key.encode("utf-8")) > 256:
                    raise ValueError("bounded JSON object key exceeds 256 bytes")
                stack.append((child, depth + 1))
        elif item is not None and not isinstance(item, (bool, int, float)):
            raise ValueError("bounded JSON contains an unsupported value")


def read_bounded_regular_tail(path: Path, capacity: int) -> tuple[bytes, int]:
    if capacity < 0:
        raise ValueError("tail capacity cannot be negative")
    if os.name == "nt":
        descriptor = _open_windows_bounded_regular(path)
        try:
            metadata = os.fstat(descriptor.fd)
            if not stat.S_ISREG(metadata.st_mode):
                raise ValueError(f"diagnostic input is not a regular file: {path}")
            observed = metadata.st_size
            if capacity == 0:
                result = b""
            else:
                os.lseek(descriptor.fd, max(0, observed - capacity), os.SEEK_SET)
                value = bytearray()
                while len(value) < min(observed, capacity):
                    chunk = os.read(
                        descriptor.fd,
                        min(64 * 1024, capacity - len(value)),
                    )
                    if not chunk:
                        break
                    value.extend(chunk)
                result = bytes(value)
        except BaseException as primary:
            _raise_bounded_descriptor_close(
                descriptor,
                "bounded diagnostic descriptor",
                primary,
            )
            raise
        _raise_bounded_descriptor_close(
            descriptor,
            "bounded diagnostic descriptor",
        )
        return result, observed

    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    fd = os.open(path, flags)
    try:
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError(f"diagnostic input is not a regular file: {path}")
        observed = metadata.st_size
        if capacity == 0:
            return b"", observed
        os.lseek(fd, max(0, observed - capacity), os.SEEK_SET)
        value = bytearray()
        while len(value) < min(observed, capacity):
            chunk = os.read(fd, min(64 * 1024, capacity - len(value)))
            if not chunk:
                break
            value.extend(chunk)
        return bytes(value), observed
    finally:
        os.close(fd)


def build_list_command(repository: Path, files: Sequence[str]) -> list[str]:
    command = [
        "cargo",
        "mutants",
        "--workspace",
        "--list",
        "--json",
        "--manifest-path",
        str(repository / "Cargo.toml"),
    ]
    for path in files:
        command.extend(["--file", path])
    return command


def parse_list_json(text: str) -> list[Candidate]:
    try:
        entries = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid cargo-mutants list JSON: {error}") from error
    if not isinstance(entries, list):
        raise ValueError("cargo-mutants list JSON must be an array")
    if len(entries) > MAX_INVENTORY_ENTRIES:
        raise ValueError(
            f"cargo-mutants inventory exceeds {MAX_INVENTORY_ENTRIES} entries"
        )

    candidates: list[Candidate] = []
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError("cargo-mutants list entry must be an object")
        path = _required_string(entry, "file")
        mutant_name = _required_string(entry, "name")
        if "function" not in entry:
            raise ValueError("cargo-mutants list entry requires function")
        function = entry["function"]
        if function is None:
            continue
        if not isinstance(function, dict):
            raise ValueError("cargo-mutants list entry requires function")
        symbol = _required_string(function, "function_name")
        candidates.append(Candidate(path, symbol, mutant_name))
    return candidates


def validate_cargo_mutants_version(
    output: str,
) -> tuple[RunState | None, str | None]:
    expected = f"cargo-mutants {SUPPORTED_CARGO_MUTANTS_VERSION}"
    if output.strip() == expected:
        return None, None
    return (
        RunState.TOOL_UNAVAILABLE,
        (
            f"unsupported cargo-mutants version {output.strip()!r}; "
            f"focused mutation evidence requires {expected}"
        ),
    )


def _append_outer_guard_skips(
    command: list[str], *, through_cargo_mutants: bool
) -> None:
    # These tests deliberately create physical trees at or beyond the
    # production limit. Normal test runs retain them; a monitored mutation
    # run uses injected boundary tests for the same contract instead.
    if through_cargo_mutants:
        cargo_test_args = ["--"]
        for fixture in _OUTER_GUARD_DEPTH_FIXTURES:
            cargo_test_args.extend(["--skip", fixture])
        command.extend(
            f"--cargo-test-arg={argument}" for argument in cargo_test_args
        )
    else:
        command.append("--")
        for fixture in _OUTER_GUARD_DEPTH_FIXTURES:
            command.extend(["--skip", fixture])


def build_baseline_command(candidate: Candidate) -> list[str]:
    parts = Path(candidate.path).parts
    if len(parts) < 3 or parts[0] != "crates":
        raise ValueError(
            f"candidate is outside a workspace member: {candidate.path}"
        )
    command = ["cargo", "test", "-p", parts[1]]
    if parts[1] == "hoimin-cli":
        _append_outer_guard_skips(command, through_cargo_mutants=False)
    return command


def build_mutation_command(
    repository: Path,
    output_directory: Path,
    candidate: Candidate,
    iterate: bool,
    jobs: int,
) -> list[str]:
    if candidate.mutant_name is None:
        raise ValueError("candidate has no mutant name from inventory")
    parts = Path(candidate.path).parts
    if len(parts) < 3 or parts[0] != "crates":
        raise ValueError(
            f"candidate is outside a workspace member: {candidate.path}"
        )
    package = parts[1]
    command = [
        "cargo",
        "mutants",
        "--package",
        package,
        "--test-workspace=false",
        "--manifest-path",
        str(repository / "Cargo.toml"),
        "--output",
        str(output_directory),
        "--file",
        candidate.path,
        "--re",
        f"^{re.escape(candidate.mutant_name)}$",
        "--jobs",
        str(jobs),
    ]
    if iterate:
        command.append("--iterate")
    if package == "hoimin-cli":
        _append_outer_guard_skips(command, through_cargo_mutants=True)
    return command


def classify_mutation_output(
    run_directory: Path,
    record: CommandRecord,
    candidate: Candidate,
) -> CandidateState:
    if (
        candidate.mutant_name is None
        or record.exit_code is None
        or record.timed_out
        or record.interrupted
    ):
        return CandidateState.ERROR
    output_directory = run_directory / "mutants.out"
    categories = (
        ("timeout.txt", CandidateState.TIMEOUT),
        ("unviable.txt", CandidateState.UNVIABLE),
        ("missed.txt", CandidateState.SURVIVED),
        ("caught.txt", CandidateState.KILLED),
    )
    matches: list[CandidateState] = []
    expected_line = candidate.mutant_name.encode("utf-8", errors="strict")
    for filename, state in categories:
        path = output_directory / filename
        if not path.is_file():
            continue
        data = read_bounded_regular(path, 8 * 1024**2)
        line_count = 0
        matched = 0
        for line in io.BytesIO(data):
            line_count += 1
            if line_count > MAX_INVENTORY_ENTRIES:
                return CandidateState.ERROR
            try:
                normalized = line.rstrip(b"\r\n").decode(
                    "utf-8", errors="strict"
                ).encode("utf-8")
            except UnicodeError:
                return CandidateState.ERROR
            if normalized and normalized != expected_line:
                return CandidateState.ERROR
            if normalized == expected_line:
                matched += 1
        if matched > 1:
            return CandidateState.ERROR
        if matched == 1:
            matches.append(state)
    if len(matches) != 1:
        return CandidateState.ERROR
    state = matches[0]
    outcomes_path = output_directory / "outcomes.json"
    if not outcomes_path.is_file():
        return CandidateState.ERROR
    try:
        outcomes = read_bounded_regular_json(outcomes_path, 8 * 1024**2)
    except (OSError, ValueError):
        return CandidateState.ERROR
    if not isinstance(outcomes, dict):
        return CandidateState.ERROR
    total_mutants = outcomes.get("total_mutants")
    if type(total_mutants) is not int or total_mutants != 1:
        return CandidateState.ERROR
    expected = {
        CandidateState.KILLED: "CaughtMutant",
        CandidateState.SURVIVED: "MissedMutant",
        CandidateState.TIMEOUT: "Timeout",
        CandidateState.UNVIABLE: "Unviable",
    }[state]

    def matches_mutant(item: object) -> bool:
        if not isinstance(item, dict):
            return False
        scenario = item.get("scenario")
        if not isinstance(scenario, dict):
            return False
        mutant = scenario.get("Mutant")
        return (
            isinstance(mutant, dict)
            and mutant.get("name") == candidate.mutant_name
            and item.get("summary") == expected
        )

    outcome_items = outcomes.get("outcomes")
    if not isinstance(outcome_items, list) or len(outcome_items) != 2:
        return CandidateState.ERROR
    baseline_count = sum(
        1
        for item in outcome_items
        if isinstance(item, dict)
        and item.get("scenario") == "Baseline"
        and item.get("summary") == "Success"
    )
    mutant_matches = [
        item
        for item in outcome_items
        if matches_mutant(item)
    ]
    if baseline_count != 1 or len(mutant_matches) != 1:
        return CandidateState.ERROR
    if any(
        not isinstance(item, dict)
        or not (
            (
                item.get("scenario") == "Baseline"
                and item.get("summary") == "Success"
            )
            or matches_mutant(item)
        )
        for item in outcome_items
    ):
        return CandidateState.ERROR
    return state


def _required_string(value: dict[object, object], key: str) -> str:
    item = value.get(key)
    if not isinstance(item, str) or not item:
        raise ValueError(f"cargo-mutants list entry requires string {key}")
    try:
        encoded = item.encode("utf-8", errors="strict")
    except UnicodeError as error:
        raise ValueError(
            f"cargo-mutants list entry {key} is not strict UTF-8"
        ) from error
    if len(encoded) > 16 * 1024:
        raise ValueError(
            f"cargo-mutants list entry {key} exceeds 16384 bytes"
        )
    if key == "file":
        validate_reported_path(Path(item))
    return cast(str, item)
