from __future__ import annotations

import re
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
STABLE_CANARY_WORKFLOW = (
    ROOT / ".github" / "workflows" / "rust-stable-canary.yml"
)
REPOSITORY_RUST_JOBS = {
    "quality",
    "rust",
    "contracts",
    "core-dependency-purity",
    "wheel-smoke",
    "linux-best-effort",
    "linux-cgroup-v2-hard",
}
COMPATIBILITY_RUST_JOBS = {"msrv", "rust-shuffle"}
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
    "core-dependency-purity": (
        "Manual core dependency purity (windows-latest)"
    ),
    "wheel-smoke": "Manual wheel smoke (${{ matrix.os }})",
}
CHECKOUT_ACTION = (
    "actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10"
)
SETUP_PYTHON_ACTION = (
    "actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1"
)
SETUP_UV_ACTION = (
    "astral-sh/setup-uv@08807647e7069bb48b6ef5acd8ec9567f424441b"
)
UPLOAD_ARTIFACT_ACTION = (
    "actions/upload-artifact@"
    "ea165f8d65b6e75b540449e92b4886f43607fa02"
)
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
MANUAL_NON_LINUX_CI_COMMAND = (
    "gh workflow run non-linux-ci.yml --ref <REF>"
)
EXPECTED_RELEASE_JOBS = {
    "validate-tag": {
        "runs-on": "ubuntu-latest",
        "steps": [
            {
                "uses": (
                    "actions/checkout@"
                    "df4cb1c069e1874edd31b4311f1884172cec0e10"
                )
            },
            {
                "uses": (
                    "actions/setup-python@"
                    "ece7cb06caefa5fff74198d8649806c4678c61a1"
                ),
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
            {
                "uses": (
                    "actions/checkout@"
                    "df4cb1c069e1874edd31b4311f1884172cec0e10"
                )
            },
            {
                "uses": (
                    "actions/setup-python@"
                    "ece7cb06caefa5fff74198d8649806c4678c61a1"
                ),
                "with": {"python-version": "3.14"},
            },
            {
                "uses": (
                    "astral-sh/setup-uv@"
                    "08807647e7069bb48b6ef5acd8ec9567f424441b"
                ),
                "with": {"enable-cache": True},
            },
            {
                "uses": (
                    "PyO3/maturin-action@"
                    "e83996d129638aa358a18fbd1dfb82f0b0fb5d3b"
                ),
                "with": {
                    "command": "build",
                    "args": (
                        "--release --locked --compatibility pypi "
                        "--no-default-features"
                    ),
                    "maturin-version": "v1.14.1",
                    "target": "x86_64-pc-windows-msvc",
                },
            },
            {"run": WHEEL_SMOKE_COMMAND},
            {
                "uses": (
                    "actions/upload-artifact@"
                    "ea165f8d65b6e75b540449e92b4886f43607fa02"
                ),
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
            {
                "uses": (
                    "actions/checkout@"
                    "df4cb1c069e1874edd31b4311f1884172cec0e10"
                )
            },
            {
                "uses": (
                    "actions/setup-python@"
                    "ece7cb06caefa5fff74198d8649806c4678c61a1"
                ),
                "with": {"python-version": "3.14"},
            },
            {
                "uses": (
                    "astral-sh/setup-uv@"
                    "08807647e7069bb48b6ef5acd8ec9567f424441b"
                ),
                "with": {"enable-cache": True},
            },
            {
                "uses": (
                    "PyO3/maturin-action@"
                    "e83996d129638aa358a18fbd1dfb82f0b0fb5d3b"
                ),
                "with": {
                    "command": "build",
                    "args": (
                        "--release --locked --compatibility pypi "
                        "--no-default-features"
                    ),
                    "maturin-version": "v1.14.1",
                    "target": "x86_64-unknown-linux-gnu",
                    "manylinux": "2014",
                },
            },
            {"run": WHEEL_SMOKE_COMMAND},
            {
                "uses": (
                    "actions/upload-artifact@"
                    "ea165f8d65b6e75b540449e92b4886f43607fa02"
                ),
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
                    if candidate and len(candidate) - len(candidate.lstrip()) <= 4:
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


def assert_artifact_only_release(test: unittest.TestCase, workflow: str) -> None:
    decoded = yaml.safe_load(workflow)
    test.assertIsInstance(decoded, dict)
    test.assertEqual(decoded, EXPECTED_RELEASE_WORKFLOW)


class RepositoryRustToolchainContractTests(unittest.TestCase):
    def test_repository_toolchain_is_exact_and_complete(self) -> None:
        toolchain = tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8"))

        self.assertEqual(set(toolchain), {"toolchain"})
        declaration = toolchain["toolchain"]
        self.assertEqual(
            set(declaration),
            {"channel", "profile", "components"},
        )
        self.assertEqual(declaration["channel"], "1.98.0")
        self.assertEqual(declaration["profile"], "minimal")
        self.assertCountEqual(
            declaration["components"],
            ["clippy", "rustfmt"],
        )


class CiRustJobContractTests(unittest.TestCase):
    def test_rust_jobs_install_only_their_classified_toolchain(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        decoded = yaml.safe_load(workflow)

        self.assertEqual(
            set(decoded["jobs"]),
            REPOSITORY_RUST_JOBS | COMPATIBILITY_RUST_JOBS,
        )
        self.assertNotIn("RUSTUP_TOOLCHAIN", workflow)
        self.assertNotIn("rustup override", workflow)
        self.assertNotIn("rustup default", workflow)
        self.assertNotIn("rustup run", workflow)
        self.assertNotIn("rustup update", workflow)
        all_steps = [
            step
            for job in decoded["jobs"].values()
            for step in job["steps"]
        ]
        self.assertFalse(
            any(
                "toolchain" in step.get("uses", "").lower()
                for step in all_steps
            )
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
                (
                    "rustup toolchain install nightly-2026-07-27 "
                    "--profile minimal"
                ),
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


class PlatformExecutionPolicyContractTests(unittest.TestCase):
    def test_automatic_ci_hosted_matrices_are_linux_only(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        decoded = yaml.safe_load(workflow)

        self.assertNotIn("windows-latest", workflow)
        self.assertNotIn("macos-14", workflow)
        for job_name in AUTOMATIC_LINUX_MATRIX_JOBS:
            matrix = decoded["jobs"][job_name]["strategy"]["matrix"]
            self.assertEqual(matrix, {"os": ["ubuntu-latest"]}, job_name)

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

        self.assertEqual(set(jobs), set(MANUAL_NON_LINUX_JOB_NAMES))
        self.assertNotIn("ubuntu-latest", workflow)
        self.assertNotRegex(workflow, r"(?m)^\s+if:")
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


class LatestStableCanaryContractTests(unittest.TestCase):
    def test_latest_stable_canary_is_isolated_and_environment_complete(self) -> None:
        workflow = STABLE_CANARY_WORKFLOW.read_text(encoding="utf-8")
        decoded = yaml.safe_load(workflow)

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
        self.assertIn("1.98.0", guide)
        self.assertIn("does not raise the minimum supported Rust version", guide)
        expected_commands = [
            "cargo fmt --all -- --check",
            "cargo clippy --workspace --all-targets --all-features -- -D warnings",
            "cargo test --workspace",
            "cargo test -p hoimin-cli --test run_e2e",
            "cargo test -p hoimin-core --features contracts",
            "cargo test -p hoimin-cli --features contracts",
            "uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v",
            "uvx maturin build --release",
            "uv run --frozen python tests/wheel_smoke.py",
        ]
        fence = chr(96) * 3
        prefix = (
            "Run the Rust quality gate locally with the same commands used in CI:"
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
    def test_msrv_job_matches_the_manifest_and_checks_the_locked_workspace(self) -> None:
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

        unit_tests = "uv run --frozen python -m unittest discover"
        reset = (
            "python -c \"import shutil; "
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

    def test_stable_quality_matrix_and_release_workflow_remain_nightly_free(self) -> None:
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
                "attacker/publish@0123456789abcdef",
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
                '      - "uses": attacker/publish@0123456789abcdef\n',
                1,
            ),
            "explicit mapping uses key": workflow.replace(
                "      - run: uv run --frozen python tests/wheel_smoke.py\n",
                "      - run: uv run --frozen python tests/wheel_smoke.py\n"
                "      - ? uses\n"
                "        : attacker/publish@0123456789abcdef\n",
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
            with self.subTest(case=case):
                with self.assertRaises(AssertionError):
                    assert_artifact_only_release(self, hostile_workflow)
