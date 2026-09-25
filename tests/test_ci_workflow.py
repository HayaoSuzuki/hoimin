from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
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
COMPATIBILITY_RUST_JOBS = {"msrv", "rust-shuffle"}
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
MATURIN_ACTION = "PyO3/maturin-action"
PINNED_MATURIN_VERSION = "<pinned-maturin-version>"
LEAN_ELAN_VERSION = "v4.1.2"
LEAN_CORPUS_BY_EXECUTABLE = {
    "generate": "corpus/state-machine.jsonl",
    "generate_budget": "corpus/budget-cleanup.jsonl",
    "generate_session": "corpus/session-recovery.jsonl",
    "generate_shutdown": "corpus/shutdown-orchestration.jsonl",
    "generate_workspace": "corpus/workspace-lifecycle.jsonl",
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
    "generate_process_output": "corpus/process-output-outcome.jsonl",
    "generate_timeout_limit": "corpus/timeout-limit.jsonl",
    "generate_disk_guard": "corpus/disk-guard-lifecycle.jsonl",
    "generate_cleanup_capability": "corpus/cleanup-capability.jsonl",
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
    "generate_source_order": "corpus/source-order.jsonl",
}
LEAN_SENSITIVITY_EXECUTABLES = {
    name
    for name in LEAN_CORPUS_BY_EXECUTABLE
    if name not in {"generate", "generate_budget", "generate_workspace"}
}
UPLOAD_ARTIFACT_ACTION = "actions/upload-artifact"
TAG_VALIDATION_COMMAND = (
    'python -c "import os, pathlib, tomllib; '
    "py=tomllib.loads(pathlib.Path('pyproject.toml').read_text())"
    "['project']['version']; "
    "cargo=tomllib.loads(pathlib.Path('Cargo.toml').read_text())"
    "['workspace']['package']['version']; "
    "tag=os.environ['TAG']; "
    "assert tag == f'v{py}' == f'v{cargo}', (tag, py, cargo)\""
)
WHEEL_SMOKE_COMMAND = "uv run --frozen python tests/wheel_smoke.py"
MANUAL_NON_LINUX_CI_COMMAND = "gh workflow run non-linux-ci.yml --ref <REF>"
EXPECTED_RELEASE_JOBS = {
    "validate-tag": {
        "runs-on": "ubuntu-latest",
        "steps": [
            {"uses": CHECKOUT_ACTION},
            {
                "uses": SETUP_PYTHON_ACTION,
                "with": {"python-version": "3.14"},
            },
            {
                "name": "Require the tag to match package metadata",
                "env": {"TAG": "${{ github.ref_name }}"},
                "run": f"{TAG_VALIDATION_COMMAND}\n",
            },
        ],
    },
    "windows-wheel": {
        "needs": "validate-tag",
        "runs-on": "windows-latest",
        "steps": [
            {"uses": CHECKOUT_ACTION},
            {
                "uses": SETUP_PYTHON_ACTION,
                "with": {"python-version": "3.14"},
            },
            {
                "uses": SETUP_UV_ACTION,
                "with": {"enable-cache": True},
            },
            {
                "uses": MATURIN_ACTION,
                "with": {
                    "command": "build",
                    "args": (
                        "--release --locked --compatibility pypi --no-default-features"
                    ),
                    "maturin-version": PINNED_MATURIN_VERSION,
                    "target": "x86_64-pc-windows-msvc",
                },
            },
            {"run": WHEEL_SMOKE_COMMAND},
            {
                "uses": UPLOAD_ARTIFACT_ACTION,
                "with": {
                    "name": "wheels-windows-x86_64",
                    "path": "target/wheels/*.whl",
                },
            },
        ],
    },
    "linux-wheel": {
        "needs": "validate-tag",
        "runs-on": "ubuntu-latest",
        "steps": [
            {"uses": CHECKOUT_ACTION},
            {
                "uses": SETUP_PYTHON_ACTION,
                "with": {"python-version": "3.14"},
            },
            {
                "uses": SETUP_UV_ACTION,
                "with": {"enable-cache": True},
            },
            {
                "uses": MATURIN_ACTION,
                "with": {
                    "command": "build",
                    "args": (
                        "--release --locked --compatibility pypi --no-default-features"
                    ),
                    "maturin-version": PINNED_MATURIN_VERSION,
                    "target": "x86_64-unknown-linux-gnu",
                    "manylinux": "2014",
                },
            },
            {"run": WHEEL_SMOKE_COMMAND},
            {
                "uses": UPLOAD_ARTIFACT_ACTION,
                "with": {
                    "name": "wheels-linux-x86_64",
                    "path": "target/wheels/*.whl",
                },
            },
        ],
    },
}
EXPECTED_RELEASE_WORKFLOW = {
    "name": "Release wheels",
    # PyYAML's YAML 1.1 resolver decodes the unquoted `on` key as `True`.
    True: {"push": {"tags": ["v*"]}},
    "permissions": {"contents": "read"},
    "jobs": EXPECTED_RELEASE_JOBS,
}


def job_block(workflow: str, job_name: str) -> str:
    marker = f"  {job_name}:\n"
    start = workflow.index(marker)
    following = workflow[start + len(marker) :]
    next_job = re.search(r"^  [a-z0-9-]+:\n", following, re.MULTILINE)
    end = len(workflow) if next_job is None else start + len(marker) + next_job.start()
    return workflow[start:end]


def named_step(job: dict[str, object], name: str) -> dict[str, object]:
    steps = job["steps"]
    assert isinstance(steps, list)
    return next(step for step in steps if step.get("name") == name)


def lean_gate_invocations(
    test: unittest.TestCase,
    script: str,
    *,
    fail_at: int | None = None,
) -> tuple[subprocess.CompletedProcess[str], list[dict[str, object]]]:
    with tempfile.TemporaryDirectory() as temporary_directory:
        temporary = Path(temporary_directory)
        fake_bin = temporary / "bin"
        fake_bin.mkdir()
        call_log = temporary / "calls.jsonl"
        fake_python = fake_bin / "python3"
        fake_python.write_text(
            f"""#!{sys.executable}
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
                "RUNNER_TEMP": str(runner_temp),
            }
        )
        # Execute the checked-in workflow under the test's controlled fake PATH.
        completed = subprocess.run(  # noqa: S603
            ["bash", "-euo", "pipefail", "-c", script],  # noqa: S607
            cwd=temporary,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
        calls = (
            [json.loads(line) for line in call_log.read_text().splitlines()]
            if call_log.exists()
            else []
        )
        expected_cwd = str(temporary.resolve())
        test.assertTrue(all(call["cwd"] == expected_cwd for call in calls))
        return completed, calls


def lean_module_sources() -> dict[str, Path]:
    lakefile = tomllib.loads(LEAN_LAKEFILE.read_text(encoding="utf-8"))
    executable_roots = {executable["root"] for executable in lakefile["lean_exe"]}
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


def workflow_contract(workflow: str) -> dict:
    """Check immutable action pins, then compare behavior independently of SHA."""
    decoded = yaml.safe_load(workflow)
    assert isinstance(decoded, dict), "workflow must be a mapping"
    for job in decoded["jobs"].values():
        for step in job.get("steps", []):
            if "uses" not in step:
                continue
            reference = step["uses"]
            match = (
                re.fullmatch(
                    r"([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)@[0-9a-fA-F]{40}", reference
                )
                if isinstance(reference, str)
                else None
            )
            assert match is not None, (
                f"action must use a full commit SHA: {reference!r}"
            )
            step["uses"] = match[1]
    return decoded


def assert_artifact_only_release(test: unittest.TestCase, workflow: str) -> None:
    decoded = workflow_contract(workflow)
    maturin_versions = set()
    for job in decoded["jobs"].values():
        for step in job.get("steps", []):
            if step.get("uses") != MATURIN_ACTION:
                continue
            inputs = step.get("with", {})
            test.assertIsInstance(inputs, dict)
            version = inputs.get("maturin-version")
            test.assertIsInstance(version, str)
            test.assertRegex(
                version,
                r"\Av(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z",
            )
            maturin_versions.add(version)
            inputs["maturin-version"] = PINNED_MATURIN_VERSION
    test.assertEqual(
        len(maturin_versions), 1, "release builds must share one maturin pin"
    )
    test.assertEqual(decoded, EXPECTED_RELEASE_WORKFLOW)


def assert_repository_rust_toolchain(test: unittest.TestCase, toolchain: dict) -> None:
    test.assertEqual(set(toolchain), {"toolchain"})
    declaration = toolchain["toolchain"]
    test.assertEqual(set(declaration), {"channel", "profile", "components"})
    # The manifest owns the version; this contract checks reproducible pinning.
    test.assertRegex(
        declaration["channel"],
        r"\A(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z",
    )
    test.assertEqual(declaration["profile"], "minimal")
    test.assertCountEqual(declaration["components"], ["clippy", "rustfmt"])


class WorkflowActionPinContractTests(unittest.TestCase):
    def test_every_workflow_uses_known_actions_with_full_commit_pins(self) -> None:
        allowed = {
            CHECKOUT_ACTION,
            SETUP_PYTHON_ACTION,
            SETUP_UV_ACTION,
            LEAN_CACHE_ACTION,
            UPLOAD_ARTIFACT_ACTION,
            MATURIN_ACTION,
        }
        paths = sorted(
            path
            for path in (ROOT / ".github" / "workflows").iterdir()
            if path.suffix in {".yml", ".yaml"}
        )
        self.assertTrue(paths)
        for path in paths:
            with self.subTest(workflow=path.name):
                decoded = workflow_contract(path.read_text(encoding="utf-8"))
                actions = [
                    step["uses"]
                    for job in decoded["jobs"].values()
                    for step in job.get("steps", [])
                    if "uses" in step
                ]
                self.assertTrue(actions)
                for action in actions:
                    self.assertIn(action, allowed)

    def test_accepts_updated_pins_without_changing_release_contract(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        for commit in ("1" * 40, "abcdef0123" * 4, "ABCDEF0123" * 4):
            with self.subTest(commit=commit):
                updated = re.sub(r"(?<=@)[0-9a-f]{40}", commit, workflow)
                self.assertNotEqual(updated, workflow)
                assert_artifact_only_release(self, updated)

    def test_rejects_different_actions_even_with_full_pins(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        for original, replacement in (
            (CHECKOUT_ACTION, "attacker/checkout"),
            (SETUP_UV_ACTION, "astral-sh/setup-uv-fork"),
            (UPLOAD_ARTIFACT_ACTION, LEAN_CACHE_ACTION),
        ):
            with self.subTest(action=original, replacement=replacement):
                changed = workflow.replace(original + "@", replacement + "@", 1)
                self.assertNotEqual(changed, workflow)
                workflow_contract(changed)  # Pin syntax is valid; identity must fail.
                with self.assertRaises(AssertionError):
                    assert_artifact_only_release(self, changed)

    def test_rejects_unpinned_or_malformed_action_references(self) -> None:
        for reference in (
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
        ):
            with self.subTest(reference=reference):
                workflow = yaml.safe_dump(
                    {"jobs": {"test": {"steps": [{"uses": reference}]}}}
                )
                with self.assertRaisesRegex(AssertionError, "full commit SHA"):
                    workflow_contract(workflow)

    def test_normalizes_only_step_action_references(self) -> None:
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
        self.assertEqual(observed, document)


class RepositoryRustToolchainContractTests(unittest.TestCase):
    def test_repository_toolchain_is_exact_and_complete(self) -> None:
        toolchain = tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8"))
        assert_repository_rust_toolchain(self, toolchain)

    def test_accepts_updated_exact_stable_versions(self) -> None:
        for channel in ("1.98.0", "1.98.1", "1.98.10", "1.99.0", "2.0.0"):
            with self.subTest(channel=channel):
                toolchain = tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8"))
                toolchain["toolchain"]["channel"] = channel
                assert_repository_rust_toolchain(self, toolchain)

    def test_rejects_floating_incomplete_and_nonstable_versions(self) -> None:
        for channel in (
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
        ):
            with self.subTest(channel=channel):
                toolchain = tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8"))
                toolchain["toolchain"]["channel"] = channel
                with self.assertRaises(AssertionError):
                    assert_repository_rust_toolchain(self, toolchain)

    def test_retains_profile_components_and_declaration_checks(self) -> None:
        for field, value in (
            ("profile", "default"),
            ("components", ["clippy"]),
            ("components", ["rustfmt"]),
            ("components", ["clippy", "rustfmt", "rust-src"]),
            ("components", ["clippy", "rustfmt", "rustfmt"]),
            ("targets", ["x86_64-unknown-linux-gnu"]),
        ):
            with self.subTest(field=field, value=value):
                toolchain = tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8"))
                toolchain["toolchain"][field] = value
                with self.assertRaises(AssertionError):
                    assert_repository_rust_toolchain(self, toolchain)


class PythonQualityWorkflowContractTests(unittest.TestCase):
    def test_quality_checks_use_frozen_dev_tools_without_editing_sources(self) -> None:
        for path in (CI_WORKFLOW, NON_LINUX_CI_WORKFLOW):
            with self.subTest(workflow=path.name):
                jobs = workflow_contract(path.read_text(encoding="utf-8"))["jobs"]
                steps = jobs["quality"]["steps"]
                python_setup = next(
                    step for step in steps if step.get("uses") == SETUP_PYTHON_ACTION
                )
                self.assertEqual(python_setup["with"]["python-version"], "3.14")
                self.assertTrue(
                    any(step.get("uses") == SETUP_UV_ACTION for step in steps)
                )
                commands = [step["run"] for step in steps if "run" in step]
                sync = "uv sync --frozen --no-install-project"
                checks = [
                    "uv run --frozen --no-sync ruff format --check .",
                    "uv run --frozen --no-sync ruff check --no-fix .",
                ]
                for command in checks:
                    self.assertIn(command, commands)
                    self.assertLess(commands.index(sync), commands.index(command))
                wheel_commands = [
                    step["run"]
                    for step in jobs["wheel-smoke"]["steps"]
                    if "run" in step
                ]
                self.assertIn("uv run --frozen pytest", wheel_commands)
                self.assertFalse(
                    any("unittest discover" in cmd for cmd in wheel_commands)
                )


class CiRustJobContractTests(unittest.TestCase):
    def test_rust_jobs_install_only_their_classified_toolchain(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        decoded = yaml.safe_load(workflow)

        self.assertEqual(
            set(decoded["jobs"]),
            REPOSITORY_RUST_JOBS | COMPATIBILITY_RUST_JOBS | LEAN_JOBS,
        )
        self.assertNotIn("RUSTUP_TOOLCHAIN", workflow)
        self.assertNotIn("rustup override", workflow)
        self.assertNotIn("rustup default", workflow)
        self.assertNotIn("rustup run", workflow)
        self.assertNotIn("rustup update", workflow)
        all_steps = [step for job in decoded["jobs"].values() for step in job["steps"]]
        self.assertFalse(
            any("toolchain" in step.get("uses", "").lower() for step in all_steps)
        )
        install_commands = [
            line.strip()
            for step in all_steps
            for line in step.get("run", "").splitlines()
            if line.strip().startswith("rustup toolchain install")
        ]
        self.assertCountEqual(
            install_commands,
            ["rustup toolchain install"] * len(REPOSITORY_RUST_JOBS)
            + [
                "rustup toolchain install 1.88 --profile minimal",
                ("rustup toolchain install nightly-2026-07-27 --profile minimal"),
            ],
        )
        self.assertCountEqual(
            re.findall(
                r"(?m)\b(?:cargo|rustc|rustdoc) \+([^\s]+)",
                workflow,
            ),
            ["1.88", "nightly-2026-07-27"],
        )
        for job_name in REPOSITORY_RUST_JOBS:
            job = job_block(workflow, job_name)
            steps = decoded["jobs"][job_name]["steps"]
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
                    step.get("run", ""),
                )
            ]
            self.assertEqual(len(install_indexes), 1, job_name)
            self.assertTrue(rust_command_indexes, job_name)
            self.assertLess(
                install_indexes[0],
                min(rust_command_indexes),
                job_name,
            )
            self.assertNotRegex(
                job,
                r"(?:cargo|rustc|rustdoc) \+[^\s]+",
                job_name,
            )

        msrv = job_block(workflow, "msrv")
        shuffle = job_block(workflow, "rust-shuffle")
        self.assertIn("cargo +1.88 check", msrv)
        self.assertIn("cargo +nightly-2026-07-27 test", shuffle)


class LeanAuditWorkflowContractTests(unittest.TestCase):
    def run_toolchain_setup(
        self, *, cached: bool, install_fails: bool = False
    ) -> subprocess.CompletedProcess[str]:
        workflow = yaml.safe_load(CI_WORKFLOW.read_text(encoding="utf-8"))
        script = named_step(
            workflow["jobs"]["lean-audit"], "Install pinned Lean toolchain"
        )["run"]
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            elan_bin = temporary / ".elan" / "bin"
            elan_bin.mkdir(parents=True)
            elan = elan_bin / "elan"
            elan.write_text(
                f"""#!{sys.executable}
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
            )
            elan.chmod(0o755)
            cached_toolchain = temporary / "cached-toolchain"
            if cached:
                cached_toolchain.touch()
            # The workflow script and fake elan executable are controlled fixtures.
            return subprocess.run(  # noqa: S603
                ["bash", "-euo", "pipefail", "-c", script],  # noqa: S607
                cwd=ROOT,
                env={
                    **os.environ,
                    "HOME": str(temporary),
                    "ELAN_HOME": str(temporary / ".elan"),
                    "GITHUB_PATH": str(temporary / "github-path"),
                    "RUNNER_TEMP": str(temporary),
                    "TEST_TOOLCHAIN_CACHE": str(cached_toolchain),
                    "TEST_INSTALL_FAILS": str(int(install_fails)),
                },
                check=False,
                capture_output=True,
                text=True,
                timeout=10,
            )

    def test_toolchain_setup_executes_pinned_lean_without_cache(self) -> None:
        completed = self.run_toolchain_setup(cached=False)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("Lean (version 4.32.2)", completed.stdout)

    def test_toolchain_setup_reuses_cache_without_installing(self) -> None:
        completed = self.run_toolchain_setup(cached=True, install_fails=True)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("Lean (version 4.32.2)", completed.stdout)

    def test_toolchain_setup_propagates_install_failure(self) -> None:
        completed = self.run_toolchain_setup(cached=False, install_fails=True)
        self.assertEqual(completed.returncode, 23, completed.stderr)

    def test_job_uses_pinned_tools_repository_toolchain_and_cache(self) -> None:
        workflow = workflow_contract(CI_WORKFLOW.read_text(encoding="utf-8"))
        job = workflow["jobs"]["lean-audit"]

        self.assertEqual(job["needs"], "quality")
        self.assertEqual(job["runs-on"], "ubuntu-latest")
        self.assertEqual(job["timeout-minutes"], 60)
        self.assertNotIn("if", job)
        self.assertEqual(job["steps"][0], {"uses": CHECKOUT_ACTION})
        self.assertEqual(
            job["steps"][1],
            {
                "uses": SETUP_PYTHON_ACTION,
                "with": {"python-version": "3.14"},
            },
        )
        cache = job["steps"][2]
        self.assertEqual(cache["uses"], LEAN_CACHE_ACTION)
        self.assertEqual(
            set(cache["with"]["path"].splitlines()),
            {"~/.elan/toolchains", "formal/HoiminOracle/.lake"},
        )
        self.assertIn("formal/HoiminOracle/lean-toolchain", cache["with"]["key"])
        self.assertIn("formal/HoiminOracle/lakefile.toml", cache["with"]["key"])
        self.assertIn("formal/HoiminOracle/**/*.lean", cache["with"]["key"])

        install = named_step(job, "Install pinned Lean toolchain")["run"]
        self.assertIn(f"releases/download/{LEAN_ELAN_VERSION}/", install)
        self.assertIn('echo "$HOME/.elan/bin" >> "$GITHUB_PATH"', install)
        self.assertEqual(
            LEAN_TOOLCHAIN.read_text(encoding="utf-8").strip(),
            "leanprover/lean4:v4.32.2",
        )
        lakefile = tomllib.loads(LEAN_LAKEFILE.read_text(encoding="utf-8"))
        self.assertEqual(
            lakefile["moreLeanArgs"],
            ["-j1", "-DElab.async=false"],
        )
        artifact = job["steps"][-1]
        self.assertEqual(artifact["if"], "always()")
        self.assertEqual(artifact["uses"], UPLOAD_ARTIFACT_ACTION)
        self.assertEqual(
            artifact["with"],
            {
                "name": "lean-audit-stats",
                "path": "${{ runner.temp }}/lean-audit",
                "if-no-files-found": "warn",
                "retention-days": 7,
            },
        )

    def test_bounded_audit_covers_every_module_and_generator(self) -> None:
        workflow = yaml.safe_load(CI_WORKFLOW.read_text(encoding="utf-8"))
        step = named_step(workflow["jobs"]["lean-audit"], "Run bounded Lean audit")

        self.assertEqual(step["working-directory"], "formal/HoiminOracle")
        self.assertEqual(step["shell"], "bash")
        completed, calls = lean_gate_invocations(self, step["run"])
        self.assertEqual(completed.returncode, 0, completed.stderr)

        guarded_commands: list[list[str]] = []
        stats_paths: list[str] = []
        for call in calls:
            arguments = call["argv"]
            self.assertEqual(arguments[0], "tools/lean_resource_guard.py")
            self.assertEqual(
                arguments[1:7],
                [
                    "--timeout-seconds",
                    "30",
                    "--rss-limit-mib",
                    "2048",
                    "--sample-ms",
                    "250",
                ],
            )
            self.assertEqual(arguments[7], "--stats")
            stats_paths.append(arguments[8])
            self.assertEqual(arguments[9], "--")
            guarded_commands.append(arguments[10:])
        self.assertEqual(len(stats_paths), len(set(stats_paths)))
        self.assertTrue(all("/runner/lean-audit/" in path for path in stats_paths))

        sources = lean_module_sources()
        module_count = len(sources)
        self.assertTrue(sources)
        module_commands = guarded_commands[:module_count]
        modules = [command[2][1:-2] for command in module_commands]
        self.assertTrue(
            all(
                command[:2] == ["lake", "build"]
                and command[2].startswith("+")
                and command[2].endswith(":o")
                for command in module_commands
            )
        )
        self.assertEqual(set(modules), set(sources))
        self.assertEqual(len(modules), len(set(modules)))
        positions = {module: index for index, module in enumerate(modules)}
        for module, source in sources.items():
            imports = re.findall(
                r"(?m)^import ([A-Za-z0-9_.]+)$",
                source.read_text(encoding="utf-8"),
            )
            for dependency in imports:
                if dependency in positions:
                    self.assertLess(
                        positions[dependency],
                        positions[module],
                        f"{dependency} must be built before {module}",
                    )

        remaining = guarded_commands[module_count:]
        self.assertEqual(remaining.pop(0), ["lake", "build", "HoiminOracle"])
        self.assertEqual(
            remaining.pop(0),
            [
                "lake",
                "env",
                "lean",
                "-j1",
                "-DElab.async=false",
                "--run",
                "ResourceCleanupAuditMain.lean",
                "4",
            ],
        )
        lakefile = tomllib.loads(LEAN_LAKEFILE.read_text(encoding="utf-8"))
        executable_names = [item["name"] for item in lakefile["lean_exe"]]
        self.assertEqual(executable_names, list(LEAN_CORPUS_BY_EXECUTABLE))
        expected_gates: list[list[str]] = []
        for executable, corpus in LEAN_CORPUS_BY_EXECUTABLE.items():
            expected_gates.append(["lake", "exe", executable, "--", "--check", corpus])
            if executable in LEAN_SENSITIVITY_EXECUTABLES:
                expected_gates.append(
                    ["lake", "exe", executable, "--", "--sensitivity"]
                )
        self.assertEqual(remaining, expected_gates)
        self.assertEqual(
            set(LEAN_CORPUS_BY_EXECUTABLE.values()),
            {
                str(path.relative_to(LEAN_ORACLE))
                for path in (LEAN_ORACLE / "corpus").glob("*.jsonl")
            },
        )

    def test_bounded_audit_stops_after_the_first_failed_gate(self) -> None:
        workflow = yaml.safe_load(CI_WORKFLOW.read_text(encoding="utf-8"))
        script = named_step(
            workflow["jobs"]["lean-audit"],
            "Run bounded Lean audit",
        )["run"]

        completed, calls = lean_gate_invocations(self, script, fail_at=4)

        self.assertEqual(completed.returncode, 23)
        self.assertEqual(len(calls), 4)

    def test_resource_cleanup_failure_stops_before_corpus_checks(self) -> None:
        workflow = yaml.safe_load(CI_WORKFLOW.read_text(encoding="utf-8"))
        script = named_step(workflow["jobs"]["lean-audit"], "Run bounded Lean audit")[
            "run"
        ]
        resource_gate = len(lean_module_sources()) + 2

        completed, calls = lean_gate_invocations(self, script, fail_at=resource_gate)

        self.assertEqual(completed.returncode, 23)
        self.assertEqual(len(calls), resource_gate)
        self.assertEqual(
            calls[-1]["argv"][10:],
            [
                "lake",
                "env",
                "lean",
                "-j1",
                "-DElab.async=false",
                "--run",
                "ResourceCleanupAuditMain.lean",
                "4",
            ],
        )


class PlatformExecutionPolicyContractTests(unittest.TestCase):
    def test_automatic_ci_hosted_matrices_are_linux_only(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        decoded = yaml.safe_load(workflow)
        jobs = decoded["jobs"]

        self.assertEqual(
            trigger_events(workflow),
            {"pull_request", "push", "merge_group", "workflow_dispatch"},
        )
        self.assertNotIn("windows-latest", workflow)
        self.assertNotIn("macos-14", workflow)
        for job_name in AUTOMATIC_LINUX_MATRIX_JOBS:
            matrix = jobs[job_name]["strategy"]["matrix"]
            self.assertEqual(matrix, {"os": ["ubuntu-latest"]}, job_name)

        matrix_runner_jobs = {
            job_name
            for job_name, job in jobs.items()
            if job["runs-on"] == "${{ matrix.os }}"
        }
        self.assertEqual(matrix_runner_jobs, AUTOMATIC_LINUX_MATRIX_JOBS)
        for job_name, job in jobs.items():
            runner = job["runs-on"]
            if job_name in matrix_runner_jobs:
                continue
            if isinstance(runner, list):
                self.assertIn("linux", runner, job_name)
                self.assertFalse(
                    {"windows", "macos"}.intersection(runner),
                    job_name,
                )
            else:
                self.assertEqual(runner, "ubuntu-latest", job_name)

    def test_non_linux_ci_has_only_a_manual_trigger(self) -> None:
        workflow = NON_LINUX_CI_WORKFLOW.read_text(encoding="utf-8")
        decoded = yaml.safe_load(workflow)

        self.assertEqual(
            set(decoded),
            {"name", True, "permissions", "jobs"},
        )
        self.assertEqual(trigger_events(workflow), {"workflow_dispatch"})
        self.assertEqual(decoded[True], {"workflow_dispatch": None})
        self.assertEqual(decoded["permissions"], {"contents": "read"})

    def test_manual_non_linux_jobs_are_complete_and_independent(self) -> None:
        automatic = yaml.safe_load(CI_WORKFLOW.read_text(encoding="utf-8"))
        workflow = NON_LINUX_CI_WORKFLOW.read_text(encoding="utf-8")
        manual = yaml.safe_load(workflow)
        jobs = manual["jobs"]

        self.assertEqual(
            set(jobs),
            set(MANUAL_NON_LINUX_JOB_NAMES)
            | {"windows-resource-scope", "windows-metrics-destinations"},
        )
        self.assertNotIn("ubuntu-latest", workflow)
        for job_name, job in jobs.items():
            self.assertNotIn("if", job, job_name)
            for step in job.get("steps", []):
                if step.get("uses", "").startswith("actions/upload-artifact@"):
                    self.assertEqual(step.get("if"), "always()", job_name)
                else:
                    self.assertNotIn("if", step, job_name)
        for job_name, expected_name in MANUAL_NON_LINUX_JOB_NAMES.items():
            job = jobs[job_name]
            self.assertEqual(job["name"], expected_name, job_name)
            self.assertNotIn("needs", job, job_name)
            self.assertNotIn("outputs", job, job_name)
            self.assertEqual(
                job["steps"],
                automatic["jobs"][job_name]["steps"],
                job_name,
            )

        for job_name, expected_os in MANUAL_NON_LINUX_MATRIX_JOBS.items():
            job = jobs[job_name]
            self.assertEqual(
                job["strategy"],
                {"fail-fast": False, "matrix": {"os": expected_os}},
                job_name,
            )
            self.assertEqual(job["runs-on"], "${{ matrix.os }}", job_name)

        purity = jobs["core-dependency-purity"]
        self.assertNotIn("strategy", purity)
        self.assertEqual(purity["runs-on"], "windows-latest")

    def test_windows_resource_scope_runs_native_acceptance_independently(self) -> None:
        jobs = workflow_contract(NON_LINUX_CI_WORKFLOW.read_text(encoding="utf-8"))[
            "jobs"
        ]
        self.assertIn("windows-resource-scope", jobs)
        self.assertEqual(
            jobs["windows-resource-scope"],
            {
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
            },
        )

    def test_windows_metrics_destinations_runs_native_acceptance_independently(
        self,
    ) -> None:
        jobs = workflow_contract(NON_LINUX_CI_WORKFLOW.read_text(encoding="utf-8"))[
            "jobs"
        ]

        self.assertIn("windows-metrics-destinations", jobs)
        self.assertEqual(
            jobs["windows-metrics-destinations"],
            {
                "name": "Manual Windows metrics destinations",
                "runs-on": "windows-latest",
                "timeout-minutes": 25,
                "env": {"HOIMIN_REQUIRE_WINDOWS_SYMLINKS": "1"},
                "steps": [
                    {"uses": CHECKOUT_ACTION},
                    {
                        "uses": SETUP_PYTHON_ACTION,
                        "with": {"python-version": "3.14"},
                    },
                    {
                        "uses": SETUP_UV_ACTION,
                        "with": {"enable-cache": True},
                    },
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
            },
        )


class LatestStableCanaryContractTests(unittest.TestCase):
    def test_latest_stable_canary_is_isolated_and_environment_complete(self) -> None:
        workflow = STABLE_CANARY_WORKFLOW.read_text(encoding="utf-8")
        decoded = workflow_contract(workflow)

        self.assertEqual(
            set(decoded),
            {"name", True, "permissions", "jobs"},
        )
        self.assertEqual(decoded["name"], "Latest stable Rust canary")
        self.assertEqual(
            trigger_events(workflow),
            {"schedule", "workflow_dispatch"},
        )
        self.assertEqual(decoded["permissions"], {"contents": "read"})
        self.assertEqual(decoded[True]["schedule"], [{"cron": "0 3 * * 1"}])
        self.assertIsNone(decoded[True]["workflow_dispatch"])
        self.assertEqual(set(decoded["jobs"]), {"stable"})
        job = decoded["jobs"]["stable"]
        self.assertEqual(set(job), {"runs-on", "steps"})
        self.assertEqual(job["runs-on"], "ubuntu-latest")
        self.assertEqual(
            job["steps"],
            [
                {"uses": CHECKOUT_ACTION},
                {
                    "uses": SETUP_PYTHON_ACTION,
                    "with": {"python-version": "3.14"},
                },
                {
                    "uses": SETUP_UV_ACTION,
                    "with": {"enable-cache": True},
                },
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
                    ),
                },
                {"run": "cargo +stable test --workspace"},
            ],
        )
        self.assertNotIn(
            "rust-stable-canary",
            CI_WORKFLOW.read_text(encoding="utf-8"),
        )


class ToolchainReleaseDocumentationContractTests(unittest.TestCase):
    def test_release_has_no_toolchain_override(self) -> None:
        release = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        self.assertNotIn("RUSTUP_TOOLCHAIN", release)
        self.assertNotIn("rust-toolchain:", release)

    def test_development_guide_separates_pin_updates_from_msrv_updates(self) -> None:
        guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")

        self.assertIn("## Pinned Rust toolchain", guide)
        self.assertIn("rust-toolchain.toml", guide)
        self.assertIn("major.minor.patch", guide)
        self.assertIn("does not raise the minimum supported Rust version", guide)
        expected_commands = [
            "uv sync --frozen --no-install-project",
            "uv run --frozen --no-sync ruff format --check .",
            "uv run --frozen --no-sync ruff check --no-fix .",
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
            "uv run --frozen python tests/wheel_smoke.py",
        ]
        fence = chr(96) * 3
        prefix = (
            "Run the quality gates locally with the same commands used in CI:"
            f"\n\n{fence}console\n"
        )
        start = guide.index(prefix) + len(prefix)
        end = guide.index(f"\n{fence}", start)
        self.assertEqual(guide[start:end].splitlines(), expected_commands)
        self.assertNotIn("uv run maturin build --release", guide)

    def test_development_guide_documents_one_shot_non_linux_ci(self) -> None:
        guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")
        normalized = " ".join(guide.split())

        self.assertIn("## CI platform execution policy", guide)
        self.assertIn("Linux CI consumes runner capacity", normalized)
        self.assertIn("not a dependency or merge condition", normalized)
        self.assertIn("once against the final ref", normalized)
        self.assertEqual(guide.count(MANUAL_NON_LINUX_CI_COMMAND), 1)


class ShuffleWorkflowContractTests(unittest.TestCase):
    def test_msrv_job_matches_the_manifest_and_checks_the_locked_workspace(
        self,
    ) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        manifest = tomllib.loads(CARGO_MANIFEST.read_text(encoding="utf-8"))
        msrv = manifest["workspace"]["package"]["rust-version"]
        job = job_block(workflow, "msrv")

        self.assertRegex(job, r"(?m)^    needs: quality$")
        self.assertRegex(job, r"(?m)^    runs-on: ubuntu-latest$")
        self.assertIn(
            f"rustup toolchain install {msrv} --profile minimal",
            job,
        )
        self.assertRegex(
            job,
            rf"(?m)^      - run: cargo \+{re.escape(msrv)} check "
            r"--workspace --all-targets --all-features --locked$",
        )

    def test_wheel_smoke_build_starts_from_an_empty_artifact_directory(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        wheel_smoke = job_block(workflow, "wheel-smoke")

        unit_tests = "uv run --frozen pytest"
        reset = (
            'python -c "import shutil; '
            "shutil.rmtree('target/wheels', ignore_errors=True)\""
        )
        build = "uvx maturin build --release"
        smoke = "uv run --frozen python tests/wheel_smoke.py"

        self.assertLess(wheel_smoke.index(unit_tests), wheel_smoke.index(reset))
        self.assertLess(wheel_smoke.index(reset), wheel_smoke.index(build))
        self.assertLess(wheel_smoke.index(build), wheel_smoke.index(smoke))

    def test_shuffle_job_is_pinned_isolated_and_complete(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")

        stable = job_block(workflow, "rust")
        self.assertIn("matrix:\n        os: [ubuntu-latest]", stable)
        self.assertRegex(stable, r"(?m)^      - run: cargo test --workspace$")
        self.assertIn("  rust-shuffle:\n", workflow)
        shuffle = job_block(workflow, "rust-shuffle")
        self.assertRegex(shuffle, r"(?m)^    needs: quality$")
        self.assertRegex(shuffle, r"(?m)^    runs-on: ubuntu-latest$")
        self.assertIn("python-version: '3.14'", shuffle)
        self.assertRegex(
            shuffle,
            r"(?m)^        run: rustup toolchain install "
            r"nightly-2026-07-27 --profile minimal$",
        )
        self.assertRegex(shuffle, r"(?m)^      - run: uv sync --frozen$")
        self.assertRegex(
            shuffle,
            r"(?m)^        run: cargo \+nightly-2026-07-27 test --workspace -- "
            r"-Z unstable-options --shuffle$",
        )

    def test_stable_quality_matrix_and_release_workflow_remain_nightly_free(
        self,
    ) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        quality = job_block(workflow, "quality")
        release = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn(
            "matrix:\n        os: [ubuntu-latest]",
            quality,
        )
        self.assertNotIn("nightly-2026-07-27", release)
        self.assertNotIn("--shuffle", release)

    def test_development_guide_documents_seed_replay(self) -> None:
        guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")

        self.assertIn(
            "cargo +nightly-2026-07-27 test --workspace -- "
            "-Z unstable-options --shuffle\n",
            guide,
        )
        self.assertIn(
            "cargo +nightly-2026-07-27 test --workspace -- \\\n"
            "  -Z unstable-options --shuffle-seed <SEED>\n",
            guide,
        )


class TriggerReachabilityContractTests(unittest.TestCase):
    def test_job_event_extraction_covers_multiline_expressions_and_quote_styles(
        self,
    ) -> None:
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

        self.assertEqual(
            job_event_conditions(workflow),
            {"push", "workflow_dispatch"},
        )

    def test_every_job_event_condition_is_reachable_from_a_workflow_trigger(
        self,
    ) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")

        self.assertLessEqual(job_event_conditions(workflow), trigger_events(workflow))

    def test_delegated_cgroup_job_remains_main_only_opted_in_and_fail_closed(
        self,
    ) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        delegated = job_block(workflow, "linux-cgroup-v2-hard")

        self.assertIn("github.event_name == 'push'", delegated)
        self.assertIn("github.ref == 'refs/heads/main'", delegated)
        self.assertIn("vars.HOIMIN_CGROUP_V2_DELEGATED == 'true'", delegated)
        self.assertIn(
            "runs-on: [self-hosted, linux, x64, cgroup-v2-delegated]",
            delegated,
        )
        self.assertIn("! grep -Fq 'SKIP:' cgroup-v2.log", delegated)


class ReleaseWorkflowContractTests(unittest.TestCase):
    def test_accepts_updated_matching_maturin_pins(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        for version in ("v1.99.0", "v2.0.1"):
            with self.subTest(version=version):
                updated, count = re.subn(
                    r"maturin-version: v[0-9]+\.[0-9]+\.[0-9]+",
                    "maturin-version: " + version,
                    workflow,
                )
                self.assertEqual(count, 2)
                assert_artifact_only_release(self, updated)

    def test_rejects_invalid_or_missing_maturin_pins(self) -> None:
        for version in (
            "latest",
            "v1",
            "v1.15",
            "1.15.0",
            "v01.15.0",
            "v1.015.0",
            "v1.15.00",
            "v1.15.0rc1",
            "v1.15.0\n",
            " v1.15.0",
            "${{ inputs.maturin }}",
            "",
            None,
            True,
            1,
            ["v1.15.0"],
        ):
            with self.subTest(version=version):
                document = yaml.safe_load(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
                for job in document["jobs"].values():
                    for step in job["steps"]:
                        if step.get("uses", "").startswith(MATURIN_ACTION + "@"):
                            step["with"]["maturin-version"] = version
                with self.assertRaises(AssertionError):
                    assert_artifact_only_release(self, yaml.safe_dump(document))
        document = yaml.safe_load(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
        for job in document["jobs"].values():
            for step in job["steps"]:
                if step.get("uses", "").startswith(MATURIN_ACTION + "@"):
                    del step["with"]["maturin-version"]
        with self.assertRaises(AssertionError):
            assert_artifact_only_release(self, yaml.safe_dump(document))

    def test_rejects_mismatched_maturin_pins_and_other_input_changes(self) -> None:
        for change in ("version", "args", "extra input"):
            with self.subTest(change=change):
                document = yaml.safe_load(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
                step = next(
                    step
                    for step in document["jobs"]["windows-wheel"]["steps"]
                    if step.get("uses", "").startswith(MATURIN_ACTION + "@")
                )
                if change == "version":
                    step["with"]["maturin-version"] = "v9.99.0"
                elif change == "args":
                    step["with"]["args"] += " --features unexpected"
                else:
                    step["with"]["unexpected-version"] = "v1.15.0"
                with self.assertRaises(AssertionError):
                    assert_artifact_only_release(self, yaml.safe_dump(document))

    def test_version_tags_build_artifacts_without_publication_credentials(
        self,
    ) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        assert_artifact_only_release(self, workflow)

    def test_artifact_only_policy_rejects_disguised_publication_paths(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        hostile_workflows = {
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
                "          name: wheels-windows-x86_64",
                "          name: pypi-distribution",
                1,
            ),
            "unexpected artifact path": workflow.replace(
                "          path: target/wheels/*.whl",
                "          path: dist/*",
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
                "      - run: uv run --frozen python tests/wheel_smoke.py\n",
                "      - run: uv run --frozen python tests/wheel_smoke.py\n"
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
                "      - run: uv run --frozen python tests/wheel_smoke.py\n",
                "      - run: uv run --frozen python tests/wheel_smoke.py\n"
                '      - "uses": attacker/publish@'
                "0123456789abcdef0123456789abcdef01234567\n",
                1,
            ),
            "explicit mapping uses key": workflow.replace(
                "      - run: uv run --frozen python tests/wheel_smoke.py\n",
                "      - run: uv run --frozen python tests/wheel_smoke.py\n"
                "      - ? uses\n"
                "        : attacker/publish@0123456789abcdef0123456789abcdef01234567\n",
                1,
            ),
            "escaped OIDC permission key": workflow.replace(
                "permissions:\n  contents: read",
                'permissions:\n  contents: read\n  "id\\u002dtoken": write',
            ),
            "publisher shell on expected smoke command": workflow.replace(
                "      - run: uv run --frozen python tests/wheel_smoke.py\n",
                "      - run: uv run --frozen python tests/wheel_smoke.py\n"
                "        shell: bash -c 'uv publish && bash \"$1\"' -- {0}\n",
                1,
            ),
            "literal publication token on expected smoke command": (
                workflow.replace(
                    "      - run: uv run --frozen python tests/wheel_smoke.py\n",
                    "      - run: uv run --frozen python tests/wheel_smoke.py\n"
                    "        env:\n"
                    "          UV_PUBLISH_TOKEN: pypi-hostile-token\n",
                    1,
                )
            ),
            "Maturin publish command": workflow.replace(
                "          command: build\n",
                "          command: publish\n",
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

        for case, hostile_workflow in hostile_workflows.items():
            with self.subTest(case=case), self.assertRaises(AssertionError):
                assert_artifact_only_release(self, hostile_workflow)
