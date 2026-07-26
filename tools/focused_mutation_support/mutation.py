from collections.abc import Sequence
import json
from pathlib import Path
import re
from typing import cast

from .model import Candidate, CandidateState, CommandRecord, RunState


SUPPORTED_CARGO_MUTANTS_VERSION = "27.1.0"


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

    candidates: list[Candidate] = []
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError("cargo-mutants list entry must be an object")
        path = _required_string(entry, "file")
        mutant_name = _required_string(entry, "name")
        function = entry.get("function")
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


def build_baseline_command(candidate: Candidate) -> list[str]:
    parts = Path(candidate.path).parts
    if len(parts) < 3 or parts[0] != "crates":
        raise ValueError(
            f"candidate is outside a workspace member: {candidate.path}"
        )
    return ["cargo", "test", "-p", parts[1]]


def build_mutation_command(
    repository: Path,
    candidate: Candidate,
    iterate: bool,
) -> list[str]:
    if candidate.mutant_name is None:
        raise ValueError("candidate has no mutant name from inventory")
    command = [
        "cargo",
        "mutants",
        "--workspace",
        "--manifest-path",
        str(repository / "Cargo.toml"),
        "--file",
        candidate.path,
        "--re",
        f"^{re.escape(candidate.mutant_name)}$",
    ]
    if iterate:
        command.append("--iterate")
    return command


def classify_mutation_output(
    run_directory: Path,
    record: CommandRecord,
    candidate: Candidate,
) -> CandidateState:
    if (
        candidate.mutant_name is None
        or record.exit_code is None
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
    for filename, state in categories:
        path = output_directory / filename
        if not path.is_file():
            continue
        names = path.read_text(encoding="utf-8").splitlines()
        if candidate.mutant_name in names:
            matches.append(state)
    if len(matches) != 1:
        return CandidateState.ERROR
    return matches[0]


def _required_string(value: dict[object, object], key: str) -> str:
    item = value.get(key)
    if not isinstance(item, str) or not item:
        raise ValueError(f"cargo-mutants list entry requires string {key}")
    return cast(str, item)
