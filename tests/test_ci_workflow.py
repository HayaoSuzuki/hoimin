from __future__ import annotations

import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import TypedDict, TypeGuard

import pytest
import yaml

ROOT = Path(__file__).resolve().parents[1]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
FUZZ_WORKFLOW = ROOT / ".github" / "workflows" / "fuzz.yml"
NON_LINUX_CI_WORKFLOW = ROOT / ".github" / "workflows" / "non-linux-ci.yml"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
DEVELOPMENT_GUIDE = ROOT / "docs" / "development.md"
CARGO_MANIFEST = ROOT / "Cargo.toml"
RUST_TOOLCHAIN = ROOT / "rust-toolchain.toml"
LEAN_ORACLE = ROOT / "formal" / "HoiminOracle"
LEAN_LAKEFILE = LEAN_ORACLE / "lakefile.toml"
LEAN_TOOLCHAIN = LEAN_ORACLE / "lean-toolchain"
STABLE_CANARY_WORKFLOW = ROOT / ".github" / "workflows" / "rust-stable-canary.yml"
REPOSITORY_RUST_JOBS = {
    "boundary-contracts",
    "quality",
    "rust",
    "contracts",
    "core-dependency-purity",
    "wheel-smoke",
    "linux-best-effort",
    "linux-cgroup-v2-hard",
}
COMPATIBILITY_RUST_JOBS = {"msrv", "rust-shuffle", "fuzz"}
LEAN_JOBS = {"lean-audit"}
AUTOMATIC_LINUX_MATRIX_JOBS = {
    "quality",
    "rust",
    "core-dependency-purity",
    "wheel-smoke",
}
MANUAL_NON_LINUX_MATRIX_JOBS = {
    "quality": ["windows-latest", "macos-14"],
    "rust": ["windows-latest", "macos-14"],
    "wheel-smoke": ["windows-latest", "macos-14"],
}
MANUAL_NON_LINUX_JOB_NAMES = {
    "quality": "Manual quality (${{ matrix.os }})",
    "rust": "Manual Rust (${{ matrix.os }})",
    "core-dependency-purity": ("Manual core dependency purity (windows-latest)"),
    "wheel-smoke": "Manual wheel smoke (${{ matrix.os }})",
}
CHECKOUT_ACTION = "actions/checkout"
SETUP_PYTHON_ACTION = "actions/setup-python"
SETUP_UV_ACTION = "astral-sh/setup-uv"
LEAN_CACHE_ACTION = "actions/cache"
CACHE_RESTORE_ACTION = "actions/cache/restore"
CACHE_SAVE_ACTION = "actions/cache/save"
LEAN_ELAN_VERSION = "v4.1.2"
LEAN_CORPUS_BY_EXECUTABLE = {
    "generate": "corpus/state-machine.jsonl",
    "generate_budget": "corpus/budget-cleanup.jsonl",
    "generate_resume_budget": "corpus/resume-budget.jsonl",
    "generate_session": "corpus/session-recovery.jsonl",
    "generate_source_order": "corpus/source-order.jsonl",
    "generate_shutdown": "corpus/shutdown-orchestration.jsonl",
    "generate_workspace": "corpus/workspace-lifecycle.jsonl",
    "generate_glob_selection": "corpus/glob-selection.jsonl",
    "generate_result_lifecycle": "corpus/result-lifecycle.jsonl",
    "generate_candidate_ranking": "corpus/candidate-ranking.jsonl",
    "generate_schema_migration": "corpus/schema-migration-concurrency.jsonl",
    "generate_binding_flow": "corpus/binding-flow-joins.jsonl",
    "generate_annotation_scope": "corpus/annotation-scope-correspondence.jsonl",
    "generate_exception_match_binding": (
        "corpus/exception-match-binding-correspondence.jsonl"
    ),
    "generate_top_budget_projection": "corpus/top-budget-projection.jsonl",
    "generate_progress_decision": "corpus/progress-decision.jsonl",
    "generate_progress_input": "corpus/progress-input.jsonl",
    "generate_nested_try_flow": "corpus/nested-try-flow.jsonl",
    "generate_nested_match_exits": "corpus/nested-match-exits.jsonl",
    "generate_multiple_handler_joins": "corpus/multiple-handler-joins.jsonl",
    "generate_report_sequence": "corpus/report-sequence.jsonl",
    "generate_compound_pattern_guards": "corpus/compound-pattern-guards.jsonl",
    "generate_except_star_flow": "corpus/except-star-flow.jsonl",
    "generate_bounded_candidate_discovery": (
        "corpus/bounded-candidate-discovery.jsonl"
    ),
    "generate_mutation_score_exit_policy": ("corpus/mutation-score-exit-policy.jsonl"),
    "generate_output_retention": "corpus/output-retention.jsonl",
    "generate_candidate_span": "corpus/candidate-span-preservation.jsonl",
    "generate_changed_target": "corpus/changed-target-composition.jsonl",
    "generate_changed_context": "corpus/changed-context.jsonl",
    "generate_changed_lines": "corpus/changed-lines.jsonl",
    "generate_process_output": "corpus/process-output-outcome.jsonl",
    "generate_timeout_limit": "corpus/timeout-limit.jsonl",
    "generate_disk_guard": "corpus/disk-guard-lifecycle.jsonl",
    "generate_cleanup_capability": "corpus/cleanup-capability.jsonl",
    "generate_empty_directory": "corpus/empty-directory.jsonl",
    "generate_comprehension_bindings": "corpus/comprehension-bindings.jsonl",
    "generate_performance_cost": "corpus/performance-cost.jsonl",
    "generate_valid_python": "corpus/valid-python.jsonl",
    "generate_collection_annotation": "corpus/collection-annotation.jsonl",
    "generate_implicit_finally": "corpus/implicit-finally.jsonl",
    "generate_with_suppression": "corpus/with-suppression.jsonl",
    "generate_deferred_annotation": "corpus/deferred-annotation.jsonl",
    "generate_evaluation_order": "corpus/evaluation-order.jsonl",
    "generate_declaration_only": "corpus/declaration-only.jsonl",
    "generate_resume_copy": "corpus/resume-copy.jsonl",
    "generate_nullable_gate": "corpus/nullable-gate.jsonl",
    "generate_prepared_namespace": "corpus/prepared-namespace.jsonl",
    "generate_paging": "corpus/paging.jsonl",
    "generate_line_diverse": "corpus/line-diverse.jsonl",
    "generate_method_call_remove": "corpus/method-call-remove.jsonl",
    "generate_function_return_constant": "corpus/function-return-constant.jsonl",
    "generate_prepared_annotation_import": "corpus/prepared-annotation-import.jsonl",
    "generate_private_annotation_import": "corpus/private-annotation-import.jsonl",
    "generate_environment_fingerprint": "corpus/environment-fingerprint.jsonl",
    "generate_resume_diagnostic": "corpus/resume-diagnostic.jsonl",
    "generate_exception_hierarchy": "corpus/exception-hierarchy.jsonl",
}
LEAN_SENSITIVITY_EXECUTABLES = {
    name
    for name in LEAN_CORPUS_BY_EXECUTABLE
    if name not in {"generate", "generate_budget", "generate_workspace"}
}
UPLOAD_ARTIFACT_ACTION = "actions/upload-artifact"
WHEEL_SMOKE_COMMAND = "uv run --frozen --no-sync python tests/wheel_smoke.py"
MANUAL_NON_LINUX_CI_COMMAND = "gh workflow run non-linux-ci.yml --ref <REF>"


def bash_executable() -> str:
    if os.name != "nt":
        return "bash"

    git = shutil.which("git")
    if git is not None:
        candidate = Path(git).resolve().parents[1] / "bin" / "bash.exe"
        if candidate.is_file():
            return str(candidate)

    bash = shutil.which("bash")
    assert bash is not None, "Git Bash is required to execute workflow fixtures"
    return bash


def git_bash_path(path: Path) -> str:
    resolved = path.resolve()
    if os.name != "nt":
        return str(resolved)

    drive, tail = os.path.splitdrive(resolved)
    assert drive, f"Git Bash fixture path must be on a drive: {resolved}"
    return f"/{drive[0].lower()}{tail.replace(os.sep, '/')}"


def job_block(workflow: str, job_name: str) -> str:
    marker = f"  {job_name}:\n"
    start = workflow.index(marker)
    following = workflow[start + len(marker) :]
    next_job = re.search(r"^  [a-z0-9-]+:\n", following, re.MULTILINE)
    end = len(workflow) if next_job is None else start + len(marker) + next_job.start()
    return workflow[start:end]


def is_string_mapping(value: object) -> TypeGuard[dict[str, object]]:
    return isinstance(value, dict) and all(isinstance(key, str) for key in value)


def mapping(value: object) -> dict[str, object]:
    assert is_string_mapping(value), "expected a mapping with string keys"
    return value


def sequence(value: object) -> list[object]:
    assert isinstance(value, list), "expected a list"
    return list(value)


def string(value: object) -> str:
    assert isinstance(value, str), "expected a string"
    return value


def string_list(value: object) -> list[str]:
    return [string(item) for item in sequence(value)]


def workflow_document(workflow: str) -> dict[object, object]:
    decoded: object = yaml.safe_load(workflow)
    assert isinstance(decoded, dict), "workflow must be a mapping"
    return dict(decoded.items())


def workflow_jobs(document: dict[object, object]) -> dict[str, dict[str, object]]:
    return {name: mapping(job) for name, job in mapping(document["jobs"]).items()}


def job_steps(job: dict[str, object]) -> list[dict[str, object]]:
    return [mapping(step) for step in sequence(job.get("steps", []))]


def named_step(job: dict[str, object], name: str) -> dict[str, object]:
    return next(step for step in job_steps(job) if step.get("name") == name)


class LeanCall(TypedDict):
    argv: list[str]
    cwd: str


def lean_call(line: str) -> LeanCall:
    call = mapping(json.loads(line))
    return {"argv": string_list(call["argv"]), "cwd": string(call["cwd"])}


def lean_gate_invocations(
    tmp_path: Path,
    script: str,
    *,
    fail_at: int | None = None,
) -> tuple[subprocess.CompletedProcess[str], list[LeanCall]]:
    temporary = tmp_path
    fake_bin = temporary / "bin"
    fake_bin.mkdir()
    call_log = temporary / "calls.jsonl"
    fake_python = fake_bin / "python3"
    fake_python.write_text(
        f"""#!{git_bash_path(Path(sys.executable))}
import json
import os
import sys

record = {{"argv": sys.argv[1:], "cwd": os.getcwd()}}
with open(os.environ["LEAN_CALL_LOG"], "a", encoding="utf-8") as stream:
    stream.write(json.dumps(record) + "\\n")
call_count = len(open(os.environ["LEAN_CALL_LOG"], encoding="utf-8").readlines())
if call_count == int(os.environ.get("LEAN_FAIL_AT", "0")):
    raise SystemExit(23)
""",
        encoding="utf-8",
        newline="\n",
    )
    fake_python.chmod(0o755)
    runner_temp = temporary / "runner"
    runner_temp.mkdir()
    environment = os.environ.copy()
    environment.update(
        {
            "LEAN_CALL_LOG": str(call_log),
            "LEAN_FAIL_AT": str(fail_at or 0),
            "PATH": f"{fake_bin}{os.pathsep}{environment['PATH']}",
            "RUNNER_TEMP": git_bash_path(runner_temp),
        }
    )
    # Execute the checked-in workflow under the test's controlled fake PATH.
    completed = subprocess.run(  # noqa: S603
        [bash_executable(), "-euo", "pipefail"],
        cwd=temporary,
        env=environment,
        input=script,
        check=False,
        capture_output=True,
        text=True,
        timeout=60 if os.name == "nt" else 10,
    )
    calls = (
        [lean_call(line) for line in call_log.read_text().splitlines()]
        if call_log.exists()
        else []
    )
    expected_cwd = str(temporary.resolve())
    assert all(call["cwd"] == expected_cwd for call in calls)
    return completed, calls


def lean_module_sources() -> dict[str, Path]:
    lakefile = mapping(tomllib.loads(LEAN_LAKEFILE.read_text(encoding="utf-8")))
    executable_roots = {
        string(mapping(executable)["root"])
        for executable in sequence(lakefile["lean_exe"])
    }
    sources = [LEAN_ORACLE / "HoiminOracle.lean"]
    sources.extend((LEAN_ORACLE / "HoiminOracle").glob("*.lean"))
    sources.extend(LEAN_ORACLE / f"{root}.lean" for root in executable_roots)
    return {
        ".".join(source.relative_to(LEAN_ORACLE).with_suffix("").parts): source
        for source in sources
    }


def trigger_events(workflow: str) -> set[str]:
    start = workflow.index("on:\n") + len("on:\n")
    end = workflow.index("\npermissions:", start)
    return set(re.findall(r"^  ([a-z_]+):", workflow[start:end], re.MULTILINE))


def job_event_conditions(workflow: str) -> set[str]:
    events: set[str] = set()
    jobs = workflow[workflow.index("jobs:\n") + len("jobs:\n") :]
    for job_name in re.findall(r"^  ([a-z0-9-]+):\n", jobs, re.MULTILINE):
        block = job_block(workflow, job_name)
        lines = block.splitlines()
        for index, line in enumerate(lines):
            if not line.startswith("    if:"):
                continue
            expression = line.partition("if:")[2].strip()
            if expression in {"|", "|-", ">", ">-"}:
                continuation = []
                for candidate in lines[index + 1 :]:
                    if candidate and len(candidate) - len(candidate.lstrip()) <= len(
                        "    "
                    ):
                        break
                    continuation.append(candidate.strip())
                expression = " ".join(continuation)
            events.update(
                match[1]
                for match in re.findall(
                    r"github\.event_name\s*==\s*(['\"])([^'\"]+)\1",
                    expression,
                )
            )
    return events


def workflow_contract(workflow: str) -> dict[object, object]:
    """Check immutable action pins, then compare behavior independently of SHA."""
    decoded = workflow_document(workflow)
    for job in workflow_jobs(decoded).values():
        for step in job_steps(job):
            if "uses" not in step:
                continue
            reference = step["uses"]
            match = (
                re.fullmatch(
                    r"([A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)+)@[0-9a-fA-F]{40}",
                    reference,
                )
                if isinstance(reference, str)
                else None
            )
            assert match is not None, (
                f"action must use a full commit SHA: {reference!r}"
            )
            step["uses"] = match[1]
    return decoded


def assert_standalone_build(job: dict[str, object], target: str, platform: str) -> None:
    native = named_step(job, "Build standalone executable")
    assert native["run"] == (
        "cargo build --release --locked --no-default-features --bin hoimin "
        f"--target {target} --target-dir target/standalone"
    )
    executable = f"target/standalone/{target}/release/hoimin"
    if platform.startswith("windows"):
        executable += ".exe"
    verify = named_step(job, "Verify standalone executable")
    assert mapping(verify["env"])["BINARY"] == executable
    package = named_step(job, "Package verified binaries and wheels")
    assert f"--binary {executable}" in string(package["run"])


def assert_wheel_build(job: dict[str, object], target: str, platform: str) -> str:
    build = named_step(job, "Build wheel")
    assert set(build) == {"name", "env", "shell", "run"}
    assert build["shell"] == "bash"
    assert set(mapping(build["env"])) == {"MATURIN_VERSION"}
    version = mapping(build["env"])["MATURIN_VERSION"]
    assert isinstance(version, str)
    assert re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version)
    command = string(build["run"])
    if platform == "linux-x86_64":
        expected = (
            'docker run --rm --volume "$PWD:/io" --workdir /io '
            "--env CARGO_TARGET_DIR=/tmp/hoimin-wheel-target "
            'ghcr.io/pyo3/maturin:v"$MATURIN_VERSION" '
            "build --release --locked --compatibility manylinux2014 "
            f"--no-default-features --target {target} --out target/wheels\n"
            'sudo chown -R "$(id -u):$(id -g)" target/wheels\n'
            'docker image rm ghcr.io/pyo3/maturin:v"$MATURIN_VERSION"\n'
            "df -h . /tmp"
        )
    else:
        expected = (
            'uvx --from "maturin==$MATURIN_VERSION" maturin build '
            "--release --locked --compatibility pypi --no-default-features "
            f"--target {target}"
        )
    assert shlex.split(command.replace("\\\n", ""), comments=True) == shlex.split(
        expected
    )
    return version


def assert_github_release(workflow: str) -> None:
    decoded = workflow_contract(workflow)
    assert set(decoded) == {"name", True, "permissions", "concurrency", "jobs"}
    assert decoded["permissions"] == {"contents": "read"}
    assert decoded[True] == {
        "pull_request": {"branches": ["main"]},
        "pull_request_target": {"branches": ["main"], "types": ["closed"]},
        "workflow_dispatch": None,
    }
    assert decoded["concurrency"] == {
        "group": (
            "release-${{ github.event_name }}-"
            "${{ github.event.pull_request.merge_commit_sha || github.sha }}"
        ),
        "cancel-in-progress": False,
    }
    jobs = workflow_jobs(decoded)
    assert set(jobs) == {
        "prepare",
        "windows-wheel",
        "linux-wheel",
        "macos-wheel",
        "publish",
    }
    # GitHub Releases does not need PyPI credentials or publishing commands.
    assert "secrets" not in workflow
    assert "id-token" not in workflow
    assert "uv publish" not in workflow
    maturin_versions = set()
    for name, runner, target, platform in (
        ("windows-wheel", "windows-latest", "x86_64-pc-windows-msvc", "windows-x86_64"),
        ("linux-wheel", "ubuntu-22.04", "x86_64-unknown-linux-gnu", "linux-x86_64"),
        ("macos-wheel", "macos-14", "aarch64-apple-darwin", "macos-aarch64"),
    ):
        job = jobs[name]
        assert set(job) == {"if", "needs", "runs-on", "timeout-minutes", "steps"}
        assert job["if"] == "needs.prepare.outputs.version != ''"
        assert job["needs"] == "prepare"
        assert job["runs-on"] == runner
        steps = job_steps(job)
        assert_standalone_build(job, target, platform)
        assert [step["uses"] for step in steps if "uses" in step] == [
            CHECKOUT_ACTION,
            SETUP_PYTHON_ACTION,
            SETUP_UV_ACTION,
            UPLOAD_ARTIFACT_ACTION,
        ]
        checkout = steps[0]
        assert checkout["with"] == {
            "ref": "${{ needs.prepare.outputs.commit }}",
            "persist-credentials": False,
        }
        maturin_versions.add(assert_wheel_build(job, target, platform))
        build = named_step(job, "Build wheel")
        smoke = next(step for step in steps if step.get("run") == WHEEL_SMOKE_COMMAND)
        assert smoke == {"run": WHEEL_SMOKE_COMMAND}
        assert steps[steps.index(smoke) - 1] == {
            "run": "uv sync --frozen --no-install-project"
        }
        assert steps.index(build) < steps.index(smoke) < len(steps) - 1
        assert steps[-1] == {
            "uses": UPLOAD_ARTIFACT_ACTION,
            "with": {
                "name": "release-" + platform,
                "path": "dist/*",
                "if-no-files-found": "error",
            },
        }
    assert len(maturin_versions) == 1
    prepare = jobs["prepare"]
    assert prepare["permissions"] == {"contents": "write"}
    assert (
        prepare["if"] == "github.event_name != 'pull_request_target' || "
        "github.event.pull_request.merged == true"
    )
    publish = jobs["publish"]
    assert publish["permissions"] == {"contents": "write"}
    assert publish["needs"] == [
        "prepare",
        "windows-wheel",
        "linux-wheel",
        "macos-wheel",
    ]
    assert publish["if"] == "needs.prepare.outputs.publish == 'true'"
    assert all("environment" not in job and "env" not in job for job in jobs.values())
    for job in (prepare, publish):
        assert set(job) <= {
            "if",
            "needs",
            "runs-on",
            "permissions",
            "outputs",
            "steps",
            "timeout-minutes",
        }
        for step in job_steps(job):
            if "uses" in step:
                assert step["uses"] in {
                    CHECKOUT_ACTION,
                    SETUP_PYTHON_ACTION,
                    "actions/download-artifact",
                }


def assert_repository_rust_toolchain(toolchain: dict[str, object]) -> None:
    assert set(toolchain) == {"toolchain"}
    declaration = mapping(toolchain["toolchain"])
    assert set(declaration) == {"channel", "profile", "components"}
    # The manifest owns the version; this contract checks reproducible pinning.
    assert (
        re.search(
            r"\A(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z",
            string(declaration["channel"]),
        )
        is not None
    )
    assert declaration["profile"] == "minimal"
    assert sorted(string_list(declaration["components"])) == sorted(
        ["clippy", "rustfmt"]
    )


def workflow_paths() -> list[Path]:
    paths = sorted(
        path
        for path in (ROOT / ".github" / "workflows").iterdir()
        if path.suffix in {".yml", ".yaml"}
    )
    assert paths
    return paths


# Workflow Action Pin contracts.
@pytest.mark.parametrize("path", workflow_paths(), ids=lambda path: path.name)
def test_every_workflow_uses_known_actions_with_full_commit_pins(path: Path) -> None:
    allowed = {
        CHECKOUT_ACTION,
        SETUP_PYTHON_ACTION,
        SETUP_UV_ACTION,
        LEAN_CACHE_ACTION,
        CACHE_RESTORE_ACTION,
        CACHE_SAVE_ACTION,
        UPLOAD_ARTIFACT_ACTION,
        "actions/download-artifact",
    }
    decoded = workflow_contract(path.read_text(encoding="utf-8"))
    actions = [
        step["uses"]
        for job in workflow_jobs(decoded).values()
        for step in job_steps(job)
        if "uses" in step
    ]
    assert actions
    for action in actions:
        assert action in allowed


def test_scheduled_fuzz_runs_daily_with_selectable_per_target_time() -> None:
    assert FUZZ_WORKFLOW.exists(), "scheduled fuzz workflow is missing"
    workflow = workflow_contract(FUZZ_WORKFLOW.read_text(encoding="utf-8"))
    assert workflow[True] == {
        "schedule": [{"cron": "17 18 * * *"}],
        "workflow_dispatch": {
            "inputs": {
                "seconds": {
                    "description": "Fuzzing seconds per target",
                    "type": "choice",
                    "options": ["30", "60", "300"],
                    "default": "60",
                }
            }
        },
    }
    assert workflow["permissions"] == {"contents": "read"}
    assert workflow["concurrency"] == {
        "group": "fuzz-${{ github.ref }}",
        "cancel-in-progress": False,
    }

    job = workflow_jobs(workflow)["fuzz"]
    assert job["runs-on"] == "ubuntu-latest"
    assert job["timeout-minutes"] == 60
    restore = named_step(job, "Restore discovered corpus")
    save = named_step(job, "Save discovered corpus")
    artifacts = named_step(job, "Preserve fuzz diagnostics")
    assert restore["uses"] == CACHE_RESTORE_ACTION
    assert save["uses"] == CACHE_SAVE_ACTION
    assert save["if"] == "always()"
    cache_key = (
        "fuzz-corpus-v1-${{ runner.os }}-${{ github.ref_name }}-"
        "${{ github.run_id }}-${{ github.run_attempt }}"
    )
    restore_inputs = mapping(restore["with"])
    save_inputs = mapping(save["with"])
    assert restore_inputs == {
        "path": "fuzz/corpus",
        "key": cache_key,
        "restore-keys": ("fuzz-corpus-v1-${{ runner.os }}-${{ github.ref_name }}-\n"),
    }
    assert save_inputs == {"path": "fuzz/corpus", "key": cache_key}
    assert artifacts["uses"] == UPLOAD_ARTIFACT_ACTION
    assert artifacts["if"] == "always()"
    artifact_inputs = mapping(artifacts["with"])
    assert string(artifact_inputs["path"]).splitlines() == [
        "${{ runner.temp }}/fuzz-report",
        "fuzz/artifacts",
    ]
    assert artifact_inputs["if-no-files-found"] == "ignore"
    assert artifact_inputs["retention-days"] == 14
    assert "+ 3300" in string(
        named_step(job, "Start fuzz budget including setup")["run"]
    )
    assert (
        named_step(job, "Install pinned fuzz toolchain")["run"]
        == "rustup toolchain install nightly-2026-07-27 --profile minimal"
    )
    install = string(
        named_step(job, "Install cargo-fuzz within remaining budget")["run"]
    )
    assert "cargo +nightly-2026-07-27 install cargo-fuzz" in install
    assert "--version 0.13.2 --locked" in install
    command = string(named_step(job, "Fuzz all targets")["run"])
    assert "--seconds-per-target \"${{ inputs.seconds || '60' }}\"" in command


@pytest.mark.parametrize("commit", ["1" * 40, "abcdef0123" * 4, "ABCDEF0123" * 4])
def test_accepts_updated_pins_without_changing_release_contract(commit: str) -> None:
    workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
    updated = re.sub(r"(?<=@)[0-9a-f]{40}", commit, workflow)
    assert updated != workflow
    assert_github_release(updated)


@pytest.mark.parametrize(
    ("original", "replacement"),
    [
        (CHECKOUT_ACTION, "attacker/checkout"),
        (SETUP_UV_ACTION, "astral-sh/setup-uv-fork"),
        (UPLOAD_ARTIFACT_ACTION, LEAN_CACHE_ACTION),
    ],
)
def test_rejects_different_actions_even_with_full_pins(
    original: str, replacement: str
) -> None:
    workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
    changed = workflow.replace(original + "@", replacement + "@", 1)
    assert changed != workflow
    workflow_contract(changed)  # Pin syntax is valid; identity must fail.
    with pytest.raises(AssertionError):
        assert_github_release(changed)


@pytest.mark.parametrize(
    "reference",
    [
        "actions/checkout",
        "actions/checkout@v6",
        "actions/checkout@main",
        "actions/checkout@" + "1" * 39,
        "actions/checkout@" + "1" * 41,
        "actions/checkout@" + "g" * 40,
        "actions/checkout@" + "1" * 40 + "\n",
        "actions/checkout@" + "1" * 40 + "@main",
        "${{ inputs.action }}",
        "./local-action",
        None,
        True,
        ["actions/checkout"],
    ],
)
def test_rejects_unpinned_or_malformed_action_references(reference: object) -> None:
    workflow = yaml.safe_dump({"jobs": {"test": {"steps": [{"uses": reference}]}}})
    with pytest.raises(AssertionError, match="full commit SHA"):
        workflow_contract(workflow)


def test_normalizes_only_step_action_references() -> None:
    action = "actions/checkout@" + "1" * 40
    document = {
        "jobs": {
            "test": {
                "steps": [
                    {"uses": action, "with": {"ref": action}},
                    {"run": "echo " + action, "env": {"USES": action}},
                ]
            }
        }
    }
    observed = workflow_contract(yaml.safe_dump(document))
    document["jobs"]["test"]["steps"][0]["uses"] = CHECKOUT_ACTION
    assert observed == document


# Repository Rust Toolchain contracts.
def test_repository_toolchain_is_exact_and_complete() -> None:
    toolchain = mapping(tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8")))
    assert_repository_rust_toolchain(toolchain)


@pytest.mark.parametrize("channel", ["1.98.0", "1.98.1", "1.98.10", "1.99.0", "2.0.0"])
def test_accepts_updated_exact_stable_versions(channel: str) -> None:
    toolchain = mapping(tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8")))
    mapping(toolchain["toolchain"])["channel"] = channel
    assert_repository_rust_toolchain(toolchain)


@pytest.mark.parametrize(
    "channel",
    [
        "stable",
        "beta",
        "nightly",
        "nightly-2026-07-27",
        "1",
        "1.98",
        "1.98.*",
        "1.98.1-beta.1",
        "1.98.1+build",
        "v1.98.1",
        "1.98.1-x86_64-unknown-linux-gnu",
        "01.98.1",
        "1.098.1",
        "1.98.01",
        " 1.98.1",
        "1.98.1 ",
        "1.98.1\n",
        "",
        "１.98.1",
    ],
)
def test_rejects_floating_incomplete_and_nonstable_versions(channel: str) -> None:
    toolchain = mapping(tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8")))
    mapping(toolchain["toolchain"])["channel"] = channel
    with pytest.raises(AssertionError):
        assert_repository_rust_toolchain(toolchain)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("profile", "default"),
        ("components", ["clippy"]),
        ("components", ["rustfmt"]),
        ("components", ["clippy", "rustfmt", "rust-src"]),
        ("components", ["clippy", "rustfmt", "rustfmt"]),
        ("targets", ["x86_64-unknown-linux-gnu"]),
    ],
)
def test_retains_profile_components_and_declaration_checks(
    field: str, value: object
) -> None:
    toolchain = mapping(tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8")))
    mapping(toolchain["toolchain"])[field] = value
    with pytest.raises(AssertionError):
        assert_repository_rust_toolchain(toolchain)


# Python Quality Workflow contracts.
def test_type_checks_keep_all_diagnostics_and_strict_analysis_enabled() -> None:
    project = mapping(
        tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    )
    settings = mapping(mapping(project["tool"])["ty"])

    assert settings["rules"] == {"all": "error"}
    assert settings["analysis"] == {
        "strict-equality-semantics": True,
        "strict-generic-narrowing": True,
        "respect-type-ignore-comments": False,
    }
    environment = mapping(settings["environment"])
    assert environment["python-version"] == "3.14"
    assert environment["python-platform"] == "all"
    assert mapping(settings["terminal"])["error-on-warning"] is True
    assert "overrides" not in settings


@pytest.mark.parametrize(
    "path", [CI_WORKFLOW, NON_LINUX_CI_WORKFLOW], ids=lambda path: path.name
)
def test_quality_checks_use_frozen_dev_tools_without_editing_sources(
    path: Path,
) -> None:
    jobs = workflow_jobs(workflow_contract(path.read_text(encoding="utf-8")))
    steps = job_steps(jobs["quality"])
    python_setup = next(
        step for step in steps if step.get("uses") == SETUP_PYTHON_ACTION
    )
    assert mapping(python_setup["with"])["python-version"] == "3.14"
    assert any(step.get("uses") == SETUP_UV_ACTION for step in steps)
    commands = [string(step["run"]) for step in steps if "run" in step]
    sync = "uv sync --frozen --group fuzz --no-install-project"
    checks = [
        "uv run --frozen --no-sync ruff format --check .",
        "uv run --frozen --no-sync ruff check --no-fix .",
        "uv run --frozen --no-sync ty check",
    ]
    for command in checks:
        assert command in commands
        assert commands.index(sync) < commands.index(command)
    assert commands.index(checks[-1]) == commands.index(checks[-2]) + 1
    wheel_commands = [
        string(step["run"]) for step in job_steps(jobs["wheel-smoke"]) if "run" in step
    ]
    assert "uv run --frozen pytest" in wheel_commands
    assert not any("unittest discover" in cmd for cmd in wheel_commands)


def test_boundary_contracts_run_pytest_after_frozen_environment_sync() -> None:
    jobs = workflow_jobs(workflow_contract(CI_WORKFLOW.read_text(encoding="utf-8")))
    job = jobs["boundary-contracts"]
    adapter_step = named_step(job, "Adapter shape and runner classification")
    pytest_command = (
        "uv run --frozen --no-sync pytest tests/test_boundary_contracts.py -v"
    )

    assert pytest_command in string(adapter_step["run"]).splitlines()
    steps = job_steps(job)
    sync_step = next(step for step in steps if step.get("run") == "uv sync --frozen")
    assert steps.index(sync_step) < steps.index(adapter_step)


# CI Rust job contracts.
def test_rust_jobs_install_only_their_classified_toolchain() -> None:
    workflow = CI_WORKFLOW.read_text(encoding="utf-8")
    decoded = workflow_document(workflow)

    assert (
        set(workflow_jobs(decoded))
        == REPOSITORY_RUST_JOBS | COMPATIBILITY_RUST_JOBS | LEAN_JOBS
    )
    assert "RUSTUP_TOOLCHAIN" not in workflow
    assert "rustup override" not in workflow
    assert "rustup default" not in workflow
    assert "rustup run" not in workflow
    assert "rustup update" not in workflow
    all_steps = [
        step for job in workflow_jobs(decoded).values() for step in job_steps(job)
    ]
    assert not any(
        "toolchain" in string(step.get("uses", "")).lower() for step in all_steps
    )
    install_commands = [
        line.strip()
        for step in all_steps
        for line in string(step.get("run", "")).splitlines()
        if line.strip().startswith("rustup toolchain install")
    ]
    assert sorted(install_commands) == sorted(
        ["rustup toolchain install"] * len(REPOSITORY_RUST_JOBS)
        + [
            "rustup toolchain install 1.88 --profile minimal",
            "rustup toolchain install nightly-2026-07-27 --profile minimal",
            "rustup toolchain install nightly-2026-07-27 --profile minimal",
        ]
    )
    assert sorted(
        re.findall(r"(?m)\b(?:cargo|rustc|rustdoc) \+([^\s]+)", workflow)
    ) == sorted(["1.88", "nightly-2026-07-27", "nightly-2026-07-27"])
    for job_name in REPOSITORY_RUST_JOBS:
        job = job_block(workflow, job_name)
        steps = job_steps(workflow_jobs(decoded)[job_name])
        install_indexes = [
            index
            for index, step in enumerate(steps)
            if step.get("run") == "rustup toolchain install"
        ]
        rust_command_indexes = [
            index
            for index, step in enumerate(steps)
            if re.search(
                r"(?m)^(?:cargo|rustc|rustdoc|uvx maturin)\b",
                string(step.get("run", "")),
            )
        ]
        assert len(install_indexes) == 1, job_name
        assert rust_command_indexes, job_name
        assert install_indexes[0] < min(rust_command_indexes), job_name
        assert re.search(r"(?:cargo|rustc|rustdoc) \+[^\s]+", job) is None, job_name

    msrv = job_block(workflow, "msrv")
    shuffle = job_block(workflow, "rust-shuffle")
    assert "cargo +1.88 check" in msrv
    assert "cargo +nightly-2026-07-27 test" in shuffle


# Lean Audit Workflow contracts.
def run_toolchain_setup(
    tmp_path: Path, *, cached: bool, install_fails: bool = False
) -> subprocess.CompletedProcess[str]:
    workflow = workflow_document(CI_WORKFLOW.read_text(encoding="utf-8"))
    script = string(
        named_step(
            workflow_jobs(workflow)["lean-audit"], "Install pinned Lean toolchain"
        )["run"]
    )
    temporary = tmp_path
    elan_bin = temporary / ".elan" / "bin"
    elan_bin.mkdir(parents=True)
    elan = elan_bin / "elan"
    elan.write_text(
        f"""#!{git_bash_path(Path(sys.executable))}
import os
import sys
from pathlib import Path

args = sys.argv[1:]
cached = Path(os.environ["TEST_TOOLCHAIN_CACHE"])
toolchain = "leanprover/lean4:v4.32.2"
if args == ["toolchain", "install", toolchain]:
    if cached.exists():
        sys.exit("error: toolchain is already installed")
elif args != ["run", "--install", toolchain, "lean", "--version"]:
    sys.exit("unexpected toolchain selection or command")
if not cached.exists():
    if os.environ["TEST_INSTALL_FAILS"] == "1":
        sys.exit(23)
    cached.touch()
if args[0] == "run":
    print("Lean (version 4.32.2)")
""",
        encoding="utf-8",
        newline="\n",
    )
    elan.chmod(0o755)
    cached_toolchain = temporary / "cached-toolchain"
    if cached:
        cached_toolchain.touch()
    # The workflow script and fake elan executable are controlled fixtures.
    return subprocess.run(  # noqa: S603
        [bash_executable(), "-euo", "pipefail"],
        cwd=ROOT,
        env={
            **os.environ,
            "HOME": git_bash_path(temporary),
            "ELAN_HOME": str(temporary / ".elan"),
            "GITHUB_PATH": git_bash_path(temporary / "github-path"),
            "RUNNER_TEMP": git_bash_path(temporary),
            "TEST_TOOLCHAIN_CACHE": str(cached_toolchain),
            "TEST_INSTALL_FAILS": str(int(install_fails)),
        },
        input=script,
        check=False,
        capture_output=True,
        text=True,
        timeout=10,
    )


def test_toolchain_setup_executes_pinned_lean_without_cache(tmp_path: Path) -> None:
    completed = run_toolchain_setup(tmp_path, cached=False)
    assert completed.returncode == 0, completed.stderr
    assert "Lean (version 4.32.2)" in completed.stdout


def test_toolchain_setup_reuses_cache_without_installing(tmp_path: Path) -> None:
    completed = run_toolchain_setup(tmp_path, cached=True, install_fails=True)
    assert completed.returncode == 0, completed.stderr
    assert "Lean (version 4.32.2)" in completed.stdout


def test_toolchain_setup_propagates_install_failure(tmp_path: Path) -> None:
    completed = run_toolchain_setup(tmp_path, cached=False, install_fails=True)
    assert completed.returncode == 23, completed.stderr


def test_job_uses_pinned_tools_repository_toolchain_and_cache() -> None:
    workflow = workflow_contract(CI_WORKFLOW.read_text(encoding="utf-8"))
    job = workflow_jobs(workflow)["lean-audit"]

    assert job["needs"] == "quality"
    assert job["runs-on"] == "ubuntu-latest"
    assert job["timeout-minutes"] == 60
    assert "if" not in job
    assert job_steps(job)[0] == {"uses": CHECKOUT_ACTION}
    assert job_steps(job)[1] == {
        "uses": SETUP_PYTHON_ACTION,
        "with": {"python-version": "3.14"},
    }
    cache = job_steps(job)[2]
    assert cache["uses"] == LEAN_CACHE_ACTION
    assert set(string(mapping(cache["with"])["path"]).splitlines()) == {
        "~/.elan/toolchains",
        "formal/HoiminOracle/.lake",
    }
    assert "formal/HoiminOracle/lean-toolchain" in string(mapping(cache["with"])["key"])
    assert "formal/HoiminOracle/lakefile.toml" in string(mapping(cache["with"])["key"])
    assert "formal/HoiminOracle/**/*.lean" in string(mapping(cache["with"])["key"])

    install = string(named_step(job, "Install pinned Lean toolchain")["run"])
    assert f"releases/download/{LEAN_ELAN_VERSION}/" in install
    assert 'echo "$HOME/.elan/bin" >> "$GITHUB_PATH"' in install
    assert (
        LEAN_TOOLCHAIN.read_text(encoding="utf-8").strip() == "leanprover/lean4:v4.32.2"
    )
    lakefile = mapping(tomllib.loads(LEAN_LAKEFILE.read_text(encoding="utf-8")))
    assert lakefile["moreLeanArgs"] == ["-j1", "-DElab.async=false"]
    artifact = job_steps(job)[-1]
    assert artifact["if"] == "always()"
    assert artifact["uses"] == UPLOAD_ARTIFACT_ACTION
    assert mapping(artifact["with"]) == {
        "name": "lean-audit-stats",
        "path": "${{ runner.temp }}/lean-audit",
        "if-no-files-found": "warn",
        "retention-days": 7,
    }


def test_bounded_audit_covers_every_module_and_generator(tmp_path: Path) -> None:
    workflow = workflow_document(CI_WORKFLOW.read_text(encoding="utf-8"))
    step = named_step(workflow_jobs(workflow)["lean-audit"], "Run bounded Lean audit")

    assert step["working-directory"] == "formal/HoiminOracle"
    assert step["shell"] == "bash"
    completed, calls = lean_gate_invocations(tmp_path, string(step["run"]))
    assert completed.returncode == 0, completed.stderr

    guarded_commands: list[list[str]] = []
    stats_paths: list[str] = []
    for call in calls:
        arguments = call["argv"]
        assert arguments[0] == "tools/lean_resource_guard.py"
        assert arguments[1:7] == [
            "--timeout-seconds",
            "30",
            "--rss-limit-mib",
            "2048",
            "--sample-ms",
            "250",
        ]
        assert arguments[7] == "--stats"
        stats_paths.append(arguments[8])
        assert arguments[9] == "--"
        guarded_commands.append(arguments[10:])
    assert len(stats_paths) == len(set(stats_paths))
    assert all("/runner/lean-audit/" in path for path in stats_paths)

    sources = lean_module_sources()
    module_count = len(sources)
    assert sources
    module_commands = guarded_commands[:module_count]
    modules = [command[2][1:-2] for command in module_commands]
    assert all(
        command[:2] == ["lake", "build"]
        and command[2].startswith("+")
        and command[2].endswith(":o")
        for command in module_commands
    )
    assert set(modules) == set(sources)
    assert len(modules) == len(set(modules))
    positions = {module: index for index, module in enumerate(modules)}
    for module, source in sources.items():
        imports = re.findall(
            r"(?m)^import ([A-Za-z0-9_.]+)$",
            source.read_text(encoding="utf-8"),
        )
        for dependency in imports:
            if dependency in positions:
                assert positions[dependency] < positions[module], (
                    f"{dependency} must be built before {module}"
                )

    remaining = guarded_commands[module_count:]
    assert remaining.pop(0) == ["lake", "build", "HoiminOracle"]
    assert remaining.pop(0) == [
        "lake",
        "env",
        "lean",
        "-j1",
        "-DElab.async=false",
        "--run",
        "ResourceCleanupAuditMain.lean",
        "4",
    ]
    lakefile = mapping(tomllib.loads(LEAN_LAKEFILE.read_text(encoding="utf-8")))
    executable_names = [
        string(mapping(item)["name"]) for item in sequence(lakefile["lean_exe"])
    ]
    assert executable_names == list(LEAN_CORPUS_BY_EXECUTABLE)
    expected_gates: list[list[str]] = []
    for executable, corpus in LEAN_CORPUS_BY_EXECUTABLE.items():
        expected_gates.append(["lake", "exe", executable, "--", "--check", corpus])
        if executable in LEAN_SENSITIVITY_EXECUTABLES:
            expected_gates.append(["lake", "exe", executable, "--", "--sensitivity"])
    assert remaining == expected_gates
    assert set(LEAN_CORPUS_BY_EXECUTABLE.values()) == {
        path.relative_to(LEAN_ORACLE).as_posix()
        for path in (LEAN_ORACLE / "corpus").glob("*.jsonl")
    }


def test_bounded_audit_stops_after_the_first_failed_gate(tmp_path: Path) -> None:
    workflow = workflow_document(CI_WORKFLOW.read_text(encoding="utf-8"))
    script = string(
        named_step(
            workflow_jobs(workflow)["lean-audit"],
            "Run bounded Lean audit",
        )["run"]
    )

    completed, calls = lean_gate_invocations(tmp_path, script, fail_at=4)

    assert completed.returncode == 23
    assert len(calls) == 4


def test_resource_cleanup_failure_stops_before_corpus_checks(tmp_path: Path) -> None:
    workflow = workflow_document(CI_WORKFLOW.read_text(encoding="utf-8"))
    script = string(
        named_step(workflow_jobs(workflow)["lean-audit"], "Run bounded Lean audit")[
            "run"
        ]
    )
    resource_gate = len(lean_module_sources()) + 2

    completed, calls = lean_gate_invocations(tmp_path, script, fail_at=resource_gate)

    assert completed.returncode == 23
    assert len(calls) == resource_gate
    assert calls[-1]["argv"][10:] == [
        "lake",
        "env",
        "lean",
        "-j1",
        "-DElab.async=false",
        "--run",
        "ResourceCleanupAuditMain.lean",
        "4",
    ]


# Platform Execution Policy contracts.
def test_automatic_ci_hosted_matrices_are_linux_only() -> None:
    workflow = CI_WORKFLOW.read_text(encoding="utf-8")
    decoded = workflow_document(workflow)
    jobs = workflow_jobs(decoded)

    assert trigger_events(workflow) == {
        "pull_request",
        "push",
        "merge_group",
        "workflow_dispatch",
    }
    assert "windows-latest" not in workflow
    assert "macos-14" not in workflow
    for job_name in AUTOMATIC_LINUX_MATRIX_JOBS:
        matrix = mapping(jobs[job_name]["strategy"])["matrix"]
        assert matrix == {"os": ["ubuntu-latest"]}, job_name

    matrix_runner_jobs = {
        job_name
        for job_name, job in jobs.items()
        if job["runs-on"] == "${{ matrix.os }}"
    }
    assert matrix_runner_jobs == AUTOMATIC_LINUX_MATRIX_JOBS
    for job_name, job in jobs.items():
        runner = job["runs-on"]
        if job_name in matrix_runner_jobs:
            continue
        if isinstance(runner, list):
            assert "linux" in runner, job_name
            assert not {"windows", "macos"}.intersection(runner), job_name
        else:
            assert runner == "ubuntu-latest", job_name


def test_non_linux_ci_has_only_a_manual_trigger() -> None:
    workflow = NON_LINUX_CI_WORKFLOW.read_text(encoding="utf-8")
    decoded = workflow_document(workflow)

    assert set(decoded) == {"name", True, "permissions", "jobs"}
    assert trigger_events(workflow) == {"workflow_dispatch"}
    assert decoded[True] == {"workflow_dispatch": None}
    assert decoded["permissions"] == {"contents": "read"}


def test_manual_non_linux_jobs_are_complete_and_independent() -> None:
    automatic = workflow_document(CI_WORKFLOW.read_text(encoding="utf-8"))
    workflow = NON_LINUX_CI_WORKFLOW.read_text(encoding="utf-8")
    manual = workflow_document(workflow)
    jobs = workflow_jobs(manual)

    assert set(jobs) == set(MANUAL_NON_LINUX_JOB_NAMES) | {
        "windows-resource-scope",
        "windows-metrics-destinations",
    }
    assert "ubuntu-latest" not in workflow
    for job_name, job in jobs.items():
        assert "if" not in job, job_name
        for step in job_steps(job):
            if string(step.get("uses", "")).startswith("actions/upload-artifact@"):
                assert step.get("if") == "always()", job_name
            else:
                assert "if" not in step, job_name
    for job_name, expected_name in MANUAL_NON_LINUX_JOB_NAMES.items():
        job = jobs[job_name]
        assert job["name"] == expected_name, job_name
        assert "needs" not in job, job_name
        assert "outputs" not in job, job_name
        assert job_steps(job) == job_steps(workflow_jobs(automatic)[job_name]), job_name

    for job_name, expected_os in MANUAL_NON_LINUX_MATRIX_JOBS.items():
        job = jobs[job_name]
        assert mapping(job["strategy"]) == {
            "fail-fast": False,
            "matrix": {"os": expected_os},
        }, job_name
        assert job["runs-on"] == "${{ matrix.os }}", job_name

    purity = jobs["core-dependency-purity"]
    assert "strategy" not in purity
    assert purity["runs-on"] == "windows-latest"


def test_windows_resource_scope_runs_native_acceptance_independently() -> None:
    jobs = workflow_jobs(
        workflow_contract(NON_LINUX_CI_WORKFLOW.read_text(encoding="utf-8"))
    )
    assert "windows-resource-scope" in jobs
    assert jobs["windows-resource-scope"] == {
        "name": "Manual Windows resource scope",
        "runs-on": "windows-latest",
        "timeout-minutes": 20,
        "steps": [
            {"uses": CHECKOUT_ACTION},
            {"uses": SETUP_PYTHON_ACTION, "with": {"python-version": "3.14"}},
            {"uses": SETUP_UV_ACTION, "with": {"enable-cache": True}},
            {
                "name": "Install repository Rust toolchain",
                "run": "rustup toolchain install",
            },
            {"run": "uv sync --frozen"},
            {
                "run": (
                    "cargo clippy -p hoimin-cli --lib --test process_handler "
                    "--test windows_resource_scope "
                    "--all-features -- -D warnings"
                )
            },
            {
                "run": (
                    "cargo test -p hoimin-cli --all-features "
                    "--lib resource::windows::tests -- --nocapture"
                )
            },
            {
                "run": (
                    "cargo test -p hoimin-cli --all-features "
                    "--test process_handler job_object -- --nocapture"
                )
            },
            {
                "run": (
                    "cargo test -p hoimin-cli --all-features "
                    "--test windows_resource_scope -- --nocapture"
                )
            },
        ],
    }


def test_windows_metrics_destinations_runs_native_acceptance_independently() -> None:
    jobs = workflow_jobs(
        workflow_contract(NON_LINUX_CI_WORKFLOW.read_text(encoding="utf-8"))
    )

    assert "windows-metrics-destinations" in jobs
    assert jobs["windows-metrics-destinations"] == {
        "name": "Manual Windows metrics destinations",
        "runs-on": "windows-latest",
        "timeout-minutes": 25,
        "env": {"HOIMIN_REQUIRE_WINDOWS_SYMLINKS": "1"},
        "steps": [
            {"uses": CHECKOUT_ACTION},
            {"uses": SETUP_PYTHON_ACTION, "with": {"python-version": "3.14"}},
            {"uses": SETUP_UV_ACTION, "with": {"enable-cache": True}},
            {
                "name": "Install repository Rust toolchain",
                "run": "rustup toolchain install",
            },
            {"run": "uv sync --frozen"},
            {
                "run": (
                    "cargo clippy -p hoimin-cli --lib "
                    "--test metrics_destinations --all-features "
                    "-- -D warnings"
                )
            },
            {
                "run": (
                    "cargo test -p hoimin-cli --all-features "
                    "--lib metrics_destination::tests -- --nocapture"
                )
            },
            {
                "run": (
                    "cargo test -p hoimin-cli --all-features "
                    "--test metrics_destinations -- --nocapture"
                )
            },
        ],
    }


# Latest Stable Canary contracts.
def test_latest_stable_canary_is_isolated_and_environment_complete() -> None:
    workflow = STABLE_CANARY_WORKFLOW.read_text(encoding="utf-8")
    decoded = workflow_contract(workflow)

    assert set(decoded) == {"name", True, "permissions", "jobs"}
    assert decoded["name"] == "Latest stable Rust canary"
    assert trigger_events(workflow) == {"schedule", "workflow_dispatch"}
    assert decoded["permissions"] == {"contents": "read"}
    assert mapping(decoded[True])["schedule"] == [{"cron": "0 3 * * 1"}]
    assert mapping(decoded[True])["workflow_dispatch"] is None
    assert set(workflow_jobs(decoded)) == {"stable"}
    job = workflow_jobs(decoded)["stable"]
    assert set(job) == {"runs-on", "env", "steps"}
    assert job["runs-on"] == "ubuntu-latest"
    assert job["env"] == {
        "CARGO_PROFILE_DEV_DEBUG": "0",
        "CARGO_PROFILE_TEST_DEBUG": "0",
        "CARGO_INCREMENTAL": "0",
        "CARGO_BUILD_JOBS": "2",
    }
    assert job_steps(job) == [
        {"uses": CHECKOUT_ACTION},
        {"uses": SETUP_PYTHON_ACTION, "with": {"python-version": "3.14"}},
        {"uses": SETUP_UV_ACTION, "with": {"enable-cache": True}},
        {
            "name": "Install latest stable Rust tooling",
            "run": (
                "rustup toolchain install stable --profile minimal "
                "--component rustfmt --component clippy"
            ),
        },
        {"run": "uv sync --frozen"},
        {"run": "cargo +stable fmt --all -- --check"},
        {
            "run": (
                "cargo +stable clippy --workspace --all-targets "
                "--all-features -- -D warnings"
            )
        },
        {
            "name": "Remove Clippy build artifacts",
            "run": "cargo +stable clean",
        },
        {"run": "cargo +stable test --workspace"},
    ]
    assert "rust-stable-canary" not in CI_WORKFLOW.read_text(encoding="utf-8")


# Toolchain Release Documentation contracts.
def test_release_has_no_toolchain_override() -> None:
    release = RELEASE_WORKFLOW.read_text(encoding="utf-8")

    assert "RUSTUP_TOOLCHAIN" not in release
    assert "rust-toolchain:" not in release


def test_development_guide_separates_pin_updates_from_msrv_updates() -> None:
    guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")

    assert "## Pinned Rust toolchain" in guide
    assert "rust-toolchain.toml" in guide
    assert "major.minor.patch" in guide
    assert "does not raise the minimum supported Rust version" in guide
    expected_commands = [
        "uv sync --frozen --group fuzz --no-install-project",
        "uv run --frozen --no-sync ruff format --check .",
        "uv run --frozen --no-sync ruff check --no-fix .",
        "uv run --frozen --no-sync ty check",
        "cargo fmt --all -- --check",
        "cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check",
        "cargo clippy --workspace --all-targets --all-features -- -D warnings",
        (
            "cargo clippy --locked -p littrs-ruff-python-parser "
            "--lib --no-deps -- -D warnings"
        ),
        "cargo test --workspace",
        "cargo test -p hoimin-cli --test run_e2e",
        "cargo test -p hoimin-core --features contracts",
        "cargo test -p hoimin-cli --features contracts",
        "uv run --frozen pytest",
        "uvx maturin build --release",
        "uv sync --frozen --no-install-project",
        "uv run --frozen --no-sync python tests/wheel_smoke.py",
    ]
    fence = chr(96) * 3
    prefix = (
        "Run the quality gates locally with the same commands used in CI:"
        f"\n\n{fence}console\n"
    )
    start = guide.index(prefix) + len(prefix)
    end = guide.index(f"\n{fence}", start)
    assert guide[start:end].splitlines() == expected_commands
    assert "uv run maturin build --release" not in guide


def test_development_guide_documents_one_shot_non_linux_ci() -> None:
    guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")
    normalized = " ".join(guide.split())

    assert "## CI platform execution policy" in guide
    assert "Linux CI consumes runner capacity" in normalized
    assert "not a dependency or merge condition" in normalized
    assert "once against the final ref" in normalized
    assert guide.count(MANUAL_NON_LINUX_CI_COMMAND) == 1


# Shuffle Workflow contracts.
def test_msrv_job_matches_the_manifest_and_checks_the_locked_workspace() -> None:
    workflow = CI_WORKFLOW.read_text(encoding="utf-8")
    manifest = mapping(tomllib.loads(CARGO_MANIFEST.read_text(encoding="utf-8")))
    msrv = string(mapping(mapping(manifest["workspace"])["package"])["rust-version"])
    job = job_block(workflow, "msrv")

    assert re.search(r"(?m)^    needs: quality$", job) is not None
    assert re.search(r"(?m)^    runs-on: ubuntu-latest$", job) is not None
    assert f"rustup toolchain install {msrv} --profile minimal" in job
    assert (
        re.search(
            rf"(?m)^      - run: cargo \+{re.escape(msrv)} check "
            r"--workspace --all-targets --all-features --locked$",
            job,
        )
        is not None
    )


def test_wheel_smoke_build_starts_from_an_empty_artifact_directory() -> None:
    workflow = CI_WORKFLOW.read_text(encoding="utf-8")
    wheel_smoke = job_block(workflow, "wheel-smoke")

    unit_tests = "uv run --frozen pytest"
    reset = (
        'python -c "import shutil; '
        "shutil.rmtree('target/wheels', ignore_errors=True)\""
    )
    build = "uvx maturin build --release"
    smoke = "uv run --frozen --no-sync python tests/wheel_smoke.py"

    assert wheel_smoke.index(unit_tests) < wheel_smoke.index(reset)
    assert wheel_smoke.index(reset) < wheel_smoke.index(build)
    assert wheel_smoke.index(build) < wheel_smoke.index(smoke)


def test_shuffle_job_is_pinned_isolated_and_complete() -> None:
    workflow = CI_WORKFLOW.read_text(encoding="utf-8")

    stable = job_block(workflow, "rust")
    assert "matrix:\n        os: [ubuntu-latest]" in stable
    assert re.search(r"(?m)^      - run: cargo test --workspace$", stable) is not None
    assert "  rust-shuffle:\n" in workflow
    shuffle = job_block(workflow, "rust-shuffle")
    assert re.search(r"(?m)^    needs: quality$", shuffle) is not None
    assert re.search(r"(?m)^    runs-on: ubuntu-latest$", shuffle) is not None
    assert "python-version: '3.14'" in shuffle
    assert (
        re.search(
            (
                r"(?m)^        run: rustup toolchain install "
                r"nightly-2026-07-27 --profile minimal$"
            ),
            shuffle,
        )
        is not None
    )
    assert re.search(r"(?m)^      - run: uv sync --frozen$", shuffle) is not None
    assert (
        re.search(
            (
                r"(?m)^        run: cargo \+nightly-2026-07-27 test --workspace -- "
                r"-Z unstable-options --shuffle$"
            ),
            shuffle,
        )
        is not None
    )


def test_stable_quality_matrix_and_release_workflow_remain_nightly_free() -> None:
    workflow = CI_WORKFLOW.read_text(encoding="utf-8")
    quality = job_block(workflow, "quality")
    release = RELEASE_WORKFLOW.read_text(encoding="utf-8")

    assert "matrix:\n        os: [ubuntu-latest]" in quality
    assert "nightly-2026-07-27" not in release
    assert "--shuffle" not in release


def test_development_guide_documents_seed_replay() -> None:
    guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")

    assert (
        "cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle\n"
    ) in guide
    assert (
        "cargo +nightly-2026-07-27 test --workspace -- \\\n"
        "  -Z unstable-options --shuffle-seed <SEED>\n"
    ) in guide


# Trigger Reachability contracts.
def test_job_event_extraction_covers_multiline_expressions_and_quote_styles() -> None:
    workflow = """\
name: fixture
on:
  pull_request:
permissions:
  contents: read
jobs:
  guarded:
    if: >-
      ${{ github.event_name == "push" ||
          github.event_name == 'workflow_dispatch' }}
    runs-on: ubuntu-latest
"""

    assert job_event_conditions(workflow) == {"push", "workflow_dispatch"}


def test_every_job_event_condition_is_reachable_from_a_workflow_trigger() -> None:
    workflow = CI_WORKFLOW.read_text(encoding="utf-8")

    assert job_event_conditions(workflow) <= trigger_events(workflow)


def test_delegated_cgroup_job_remains_main_only_opted_in_and_fail_closed() -> None:
    workflow = CI_WORKFLOW.read_text(encoding="utf-8")
    delegated = job_block(workflow, "linux-cgroup-v2-hard")

    assert "github.event_name == 'push'" in delegated
    assert "github.ref == 'refs/heads/main'" in delegated
    assert "vars.HOIMIN_CGROUP_V2_DELEGATED == 'true'" in delegated
    assert "runs-on: [self-hosted, linux, x64, cgroup-v2-delegated]" in delegated
    assert "! grep -Fq 'SKIP:' cgroup-v2.log" in delegated


# Release Workflow contracts.
@pytest.mark.parametrize("version", ["1.99.0", "2.0.1"])
def test_accepts_updated_matching_maturin_pins(version: str) -> None:
    workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
    updated, count = re.subn(
        r'MATURIN_VERSION: "[0-9]+\.[0-9]+\.[0-9]+"',
        f'MATURIN_VERSION: "{version}"',
        workflow,
    )
    assert count == 3
    assert_github_release(updated)


@pytest.mark.parametrize(
    "version",
    [
        "latest",
        "v1",
        "v1.15",
        "v1.15.0",
        "01.15.0",
        "1.015.0",
        "1.15.00",
        "1.15.0rc1",
        "1.15.0\n",
        " 1.15.0",
        "${{ inputs.maturin }}",
        "",
        None,
        True,
        1,
        ["v1.15.0"],
    ],
)
def test_rejects_invalid_maturin_pins(version: object) -> None:
    document = workflow_document(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
    for job in workflow_jobs(document).values():
        for step in job_steps(job):
            if step.get("name") == "Build wheel":
                mapping(step["env"])["MATURIN_VERSION"] = version
    with pytest.raises(AssertionError):
        assert_github_release(yaml.safe_dump(document))


def test_rejects_missing_maturin_pins() -> None:
    document = workflow_document(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
    for job in workflow_jobs(document).values():
        for step in job_steps(job):
            if step.get("name") == "Build wheel":
                del mapping(step["env"])["MATURIN_VERSION"]
    with pytest.raises(AssertionError):
        assert_github_release(yaml.safe_dump(document))


@pytest.mark.parametrize("change", ["version", "args", "extra input"])
def test_rejects_mismatched_maturin_pins_and_other_input_changes(change: str) -> None:
    document = workflow_document(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
    step = next(
        step
        for step in job_steps(workflow_jobs(document)["windows-wheel"])
        if step.get("name") == "Build wheel"
    )
    if change == "version":
        mapping(step["env"])["MATURIN_VERSION"] = "9.99.0"
    elif change == "args":
        step["run"] = string(step["run"]) + " --features unexpected"
    else:
        mapping(step["env"])["UNEXPECTED_VERSION"] = "1.15.0"
    with pytest.raises(AssertionError):
        assert_github_release(yaml.safe_dump(document))


@pytest.mark.parametrize(
    "required",
    [
        "--compatibility manylinux2014",
        "--env CARGO_TARGET_DIR=/tmp/hoimin-wheel-target",
        '--volume "$PWD:/io"',
        "--workdir /io",
        "--out target/wheels",
        'sudo chown -R "$(id -u):$(id -g)" target/wheels',
        'docker image rm ghcr.io/pyo3/maturin:v"$MATURIN_VERSION"',
    ],
)
def test_linux_wheel_requires_portable_isolated_container_build(required: str) -> None:
    document = workflow_document(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
    build = named_step(workflow_jobs(document)["linux-wheel"], "Build wheel")
    original = string(build["run"])
    assert required in original
    build["run"] = original.replace(required, "")
    with pytest.raises(AssertionError):
        assert_github_release(yaml.safe_dump(document))


@pytest.mark.parametrize("job_name", ["windows-wheel", "linux-wheel", "macos-wheel"])
@pytest.mark.parametrize("option", ["--all-features", "--skip-auditwheel"])
def test_wheel_build_rejects_extra_options(job_name: str, option: str) -> None:
    document = workflow_document(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
    build = named_step(workflow_jobs(document)[job_name], "Build wheel")
    build["run"] = string(build["run"]).replace("--locked", f"--locked {option}")
    with pytest.raises(AssertionError):
        assert_github_release(yaml.safe_dump(document))


def test_merged_prs_publish_only_to_github_releases() -> None:
    workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")

    assert_github_release(workflow)


def test_release_version_preparation_uses_only_merged_base_commit() -> None:
    workflow = workflow_document(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
    jobs = workflow_jobs(workflow)
    prepare = jobs["prepare"]
    trusted_ref = "${{ github.event.pull_request.merge_commit_sha || github.sha }}"
    publish_condition = (
        "${{ github.event_name == 'pull_request_target' && "
        "github.event.pull_request.merged == true }}"
    )
    assert mapping(job_steps(prepare)[0]["with"]) == {
        "ref": trusted_ref,
        "fetch-depth": 0,
        "persist-credentials": publish_condition,
    }
    version = named_step(prepare, "Reserve release tag or choose preview version")
    assert version["env"] == {"PUBLISH": publish_condition, "COMMIT": trusted_ref}
    assert version["id"] == "version"
    assert (
        mapping(prepare["outputs"])["publish"] == "${{ steps.version.outputs.publish }}"
    )
    publisher = jobs["publish"]
    assert mapping(job_steps(publisher)[0]["with"]) == {
        "ref": "${{ needs.prepare.outputs.commit }}",
        "persist-credentials": False,
    }
    download = next(
        step
        for step in job_steps(publisher)
        if string(step.get("uses", "")).startswith("actions/download-artifact@")
    )
    # No other run or repository can supply the release artifacts.
    assert download["with"] == {
        "pattern": "release-*",
        "merge-multiple": True,
        "path": "dist",
    }


def hostile_release_workflows() -> dict[str, str]:
    workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
    return {
        "aliased publisher job": (
            workflow
            + """\

  upload_pypi:
    runs-on: ubuntu-latest
    steps:
      - run: uv publish
"""
        ),
        "unexpected action": workflow.replace(
            UPLOAD_ARTIFACT_ACTION,
            "attacker/publish",
            1,
        ),
        "job environment": workflow.replace(
            "    runs-on: ubuntu-latest\n",
            "    runs-on: ubuntu-latest\n    environment: pypi\n",
            1,
        ),
        "unexpected artifact name": workflow.replace(
            "          name: release-windows-x86_64",
            "          name: pypi-distribution",
            1,
        ),
        "unexpected artifact path": workflow.replace(
            "          path: dist/*",
            "          path: target/wheels/*.whl",
            1,
        ),
        "publication secret": workflow.replace(
            "    runs-on: ubuntu-latest\n",
            "    runs-on: ubuntu-latest\n"
            "    env:\n"
            "      PYPI_TOKEN: ${{ secrets.PYPI_TOKEN }}\n",
            1,
        ),
        "OIDC publication permission": workflow.replace(
            "permissions:\n  contents: read",
            "permissions:\n  contents: read\n  id-token: write",
        ),
        "publisher run command": workflow.replace(
            "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n",
            "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n"
            "      - run: uv publish\n",
            1,
        ),
        "bracketed publication secret": workflow.replace(
            "    runs-on: ubuntu-latest\n",
            "    runs-on: ubuntu-latest\n"
            "    env:\n"
            "      PYPI_TOKEN: ${{ secrets['PYPI_TOKEN'] }}\n",
            1,
        ),
        "flow OIDC permission": workflow.replace(
            "permissions:\n  contents: read",
            "permissions: {contents: read, id-token: write}",
        ),
        "quoted uses key": workflow.replace(
            "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n",
            "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n"
            '      - "uses": attacker/publish@'
            "0123456789abcdef0123456789abcdef01234567\n",
            1,
        ),
        "explicit mapping uses key": workflow.replace(
            "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n",
            "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n"
            "      - ? uses\n"
            "        : attacker/publish@0123456789abcdef0123456789abcdef01234567\n",
            1,
        ),
        "escaped OIDC permission key": workflow.replace(
            "permissions:\n  contents: read",
            'permissions:\n  contents: read\n  "id\\u002dtoken": write',
        ),
        "publisher shell on expected smoke command": workflow.replace(
            "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n",
            "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n"
            "        shell: bash -c 'uv publish && bash \"$1\"' -- {0}\n",
            1,
        ),
        "literal publication token on expected smoke command": (
            workflow.replace(
                "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n",
                "      - run: uv run --frozen --no-sync python tests/wheel_smoke.py\n"
                "        env:\n"
                "          UV_PUBLISH_TOKEN: pypi-hostile-token\n",
                1,
            )
        ),
        "Maturin publish command": workflow.replace(
            "maturin build",
            "maturin publish",
            1,
        ),
        "top-level environment and default publishing shell": workflow.replace(
            "\npermissions:\n",
            "\nenv:\n"
            "  UV_PUBLISH_TOKEN: pypi-hostile-token\n"
            "defaults:\n"
            "  run:\n"
            "    shell: bash -c 'uv publish && bash \"$1\"' -- {0}\n"
            "\npermissions:\n",
            1,
        ),
    }


@pytest.mark.parametrize(
    "hostile_workflow",
    [
        pytest.param(workflow, id=case)
        for case, workflow in hostile_release_workflows().items()
    ],
)
def test_github_release_policy_rejects_disguised_publication_paths(
    hostile_workflow: str,
) -> None:
    with pytest.raises(AssertionError):
        assert_github_release(hostile_workflow)
