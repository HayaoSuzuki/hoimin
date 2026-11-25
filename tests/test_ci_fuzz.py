import json
import os
import subprocess
import sys
import time
from contextlib import nullcontext
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import create_autospec

import pytest
import yaml
from hypothesis import example, given, settings
from hypothesis import strategies as st
from pytest_mock import MockerFixture

from tools import ci_fuzz

ROOT = Path(__file__).resolve().parents[1]
FAILURE_CODE = 23
MAX_SEED = 4294967295
JOB_MINUTES = 11


@pytest.mark.parametrize("count", [0, 1, 512, 513, 1000])
def test_active_corpus_bounds_cached_replay(tmp_path: Path, count: int) -> None:
    # Arrange: old discoveries must survive a bounded replay unchanged.
    cache = tmp_path / "cache"
    cache.mkdir()
    original = {f"input-{index:04}": str(index).encode() for index in range(count)}
    for name, data in original.items():
        (cache / name).write_bytes(data)

    # Act: read the real files libFuzzer will receive.
    with ci_fuzz.active_corpus(cache, seed=19) as active:
        replay = {p.name: p.read_bytes() for p in active.iterdir()}

    # Assert: only cached replay is bounded; no cached evidence is deleted.
    assert len(replay) == min(count, 512)
    assert replay.items() <= original.items()
    assert {p.name: p.read_bytes() for p in cache.iterdir()} == original
    assert not active.exists()


@pytest.mark.parametrize("fail", [False, True])
def test_active_corpus_retains_new_discoveries_on_exit(
    tmp_path: Path, *, fail: bool
) -> None:
    # Arrange
    cache = tmp_path / "cache"
    active_path: Path | None = None

    # Act
    with (
        pytest.raises(ci_fuzz.FuzzError) if fail else nullcontext(),
        ci_fuzz.active_corpus(cache, seed=19) as active,
    ):
        active_path = active
        (active / "new-input").write_bytes(b"discovered")
        if fail:
            msg = "target failed"
            raise ci_fuzz.FuzzError(msg)

    # Assert
    assert (cache / "new-input").read_bytes() == b"discovered"
    assert active_path is not None
    assert not active_path.exists()


def test_campaign_passes_all_fresh_inputs_outside_sampled_cache(
    recorded_campaign: tuple[ci_fuzz.Campaign, dict[str, tuple[list[str], bool]]],
) -> None:
    # Arrange
    campaign, observations = recorded_campaign

    # Act
    ci_fuzz.run_campaign(campaign, seconds_per_target=1)
    analyzer_argv, _ = observations["python_analyzer"]
    generator_commands = [
        argv for name, (argv, _) in observations.items() if name.startswith("generate-")
    ]
    generated_paths = {argv[argv.index("--output") + 1] for argv in generator_commands}

    # Assert: every generator feeds an unsampled input directory to the analyzer.
    assert len(generator_commands) == 5
    assert generated_paths == {analyzer_argv[9]}
    assert Path(analyzer_argv[9]).is_dir()
    assert analyzer_argv[9] != analyzer_argv[7]


def test_campaign_runs_against_sampled_cache_and_saves_discovery(
    recorded_campaign: tuple[ci_fuzz.Campaign, dict[str, tuple[list[str], bool]]],
    mocker: MockerFixture,
) -> None:
    # Arrange: enough cached inputs to exceed the replay bound.
    campaign, _ = recorded_campaign
    cache = ci_fuzz.ROOT / "fuzz/corpus/python_analyzer"
    cache.mkdir(parents=True)
    for index in range(600):
        (cache / str(index)).write_text(str(index))
    replayed: list[int] = []

    def run(name: str, argv: list[str], **_options: object) -> None:
        if name == "python_analyzer":
            active = Path(argv[7])
            replayed.append(len(list(active.iterdir())))
            (active / "discovery").write_bytes(b"new input")

    mocker.patch.object(campaign, "run", side_effect=run)

    # Act
    ci_fuzz.run_campaign(campaign, seconds_per_target=1)

    # Assert: the process sees the bounded copy and its output reaches the cache.
    assert replayed == [512]
    assert len(list(cache.iterdir())) == 601
    assert (cache / "discovery").read_bytes() == b"new input"


@given(seed=st.integers(1, MAX_SEED))
@settings(max_examples=10, deadline=None)
def test_active_corpus_selection_is_reproducible(seed: int) -> None:
    # Arrange
    with TemporaryDirectory() as directory:
        cache = Path(directory) / "cache"
        cache.mkdir()
        for index in range(520):
            (cache / str(index)).write_bytes(str(index).encode())

        # Act
        with ci_fuzz.active_corpus(cache, seed=seed) as active:
            first = {p.name for p in active.iterdir()}
        with ci_fuzz.active_corpus(cache, seed=seed) as active:
            second = {p.name for p in active.iterdir()}

        # Assert
        assert len(first) == 512
        assert second == first


@pytest.fixture
def recorded_campaign(
    tmp_path: Path, mocker: MockerFixture
) -> tuple[ci_fuzz.Campaign, dict[str, tuple[list[str], bool]]]:
    mocker.patch.object(ci_fuzz, "ROOT", tmp_path)
    (tmp_path / "fuzz").mkdir()
    (tmp_path / "fuzz/Cargo.toml").write_bytes((ROOT / "fuzz/Cargo.toml").read_bytes())
    campaign = ci_fuzz.Campaign(time.time() + 300, tmp_path / "report", seed=19)
    observations: dict[str, tuple[list[str], bool]] = {}

    def record(name: str, argv: list[str], **_options: object) -> None:
        observations[name] = (argv, (tmp_path / "fuzz/corpus" / name).is_dir())

    mocker.patch.object(campaign, "run", autospec=True, side_effect=record)
    return campaign, observations


@pytest.mark.parametrize(
    "target",
    [
        "source_encoding",
        "source_index",
        "candidate_validation",
        "python_analyzer",
        "run_stop",
        "disk_lifecycle",
        "report_lifecycle",
        "report_sequence",
        "target_resolution",
        "sampling",
        "budget_ledger",
        "python_reduce",
    ],
)
def test_campaign_dispatches_bounded_fuzz_commands_with_existing_output_corpus(
    recorded_campaign: tuple[ci_fuzz.Campaign, dict[str, tuple[list[str], bool]]],
    target: str,
) -> None:
    campaign, observations = recorded_campaign

    ci_fuzz.run_campaign(campaign, seconds_per_target=7)

    argv, corpus_exists = observations[target]
    assert argv[:5] == [
        "cargo",
        f"+{ci_fuzz.TOOLCHAIN}",
        "fuzz",
        "run",
        "--codegen-units",
    ]
    assert int(argv[5]) > 0
    assert argv[6] == target
    assert argv[8] == f"fuzz/seeds/{target}"
    options = argv[argv.index("--") + 1 :]
    assert options[:2] == ["-max_total_time=7", "-seed=19"]
    assert options[2].startswith("-max_len=")
    assert int(options[2].removeprefix("-max_len=")) > 0
    assert options[3].startswith("-timeout=")
    assert int(options[3].removeprefix("-timeout=")) > 0
    assert options[4].startswith("-rss_limit_mb=")
    assert int(options[4].removeprefix("-rss_limit_mb=")) > 0
    assert len(options) == 5
    assert corpus_exists


@pytest.mark.parametrize(("version", "suffix"), [("3.14", "314"), ("3.15", "315")])
def test_campaign_selects_frozen_generator_dependencies(
    recorded_campaign: tuple[ci_fuzz.Campaign, dict[str, tuple[list[str], bool]]],
    version: str,
    suffix: str,
) -> None:
    campaign, observations = recorded_campaign

    ci_fuzz.run_campaign(campaign, seconds_per_target=1)

    argv, _ = observations[f"dependencies-{suffix}"]
    assert argv == [
        "uv",
        "sync",
        "--frozen",
        "--group",
        "fuzz",
        "--no-install-project",
        "--python",
        version,
    ]


def test_cli_requires_an_explicit_reproducibility_seed(
    tmp_path: Path, mocker: MockerFixture, capsys: pytest.CaptureFixture[str]
) -> None:
    mocker.patch.object(
        sys, "argv", ["ci-fuzz", "--deadline", "1", "--report", str(tmp_path)]
    )
    construct = mocker.patch.object(ci_fuzz, "Campaign", autospec=True)
    execute = mocker.patch.object(ci_fuzz, "run_campaign", autospec=True)

    with pytest.raises(SystemExit) as error:
        ci_fuzz.main()

    assert error.value.code == 2
    assert "--seed" in capsys.readouterr().err
    construct.assert_not_called()
    execute.assert_not_called()


@pytest.mark.parametrize("name", ["ci.yml", "fuzz.yml"])
def test_fuzz_workflows_install_and_cache_the_runner_toolchain(name: str) -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows" / name).read_text(encoding="utf-8")
    )
    steps = workflow["jobs"]["fuzz"]["steps"]
    install = next(s for s in steps if s.get("name") == "Install pinned fuzz toolchain")

    cache = next(s for s in steps if s.get("name") == "Cache cargo-fuzz executable")

    build = next(
        s
        for s in steps
        if s.get("name") == "Install cargo-fuzz within remaining budget"
    )

    assert (
        install["run"]
        == f"rustup toolchain install {ci_fuzz.TOOLCHAIN} --profile minimal"
    )
    assert cache["with"]["key"].endswith(f"-{ci_fuzz.TOOLCHAIN}")
    assert f"cargo +{ci_fuzz.TOOLCHAIN} install cargo-fuzz" in build["run"]


@pytest.fixture
def fake_fuzz_root(tmp_path: Path, mocker: MockerFixture) -> Path:
    # Cargo and uv are the expensive/external boundary. Execute lightweight
    # stand-ins while retaining the real orchestration and process management.
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    for tool in ("cargo", "uv"):
        executable = fake_bin / tool
        executable.write_text(
            f"#!{sys.executable}\n"
            "import os\n"
            "import sys\n"
            "print(sys.argv[1:])\n"
            "fail_target = os.environ.get('FAKE_FUZZ_FAIL_TARGET', '')\n"
            "if fail_target and fail_target in sys.argv:\n"
            "    sys.exit(23)\n"
        )
        executable.chmod(0o755)
    mocker.patch.dict(os.environ, {"PATH": str(fake_bin)})
    (tmp_path / "fuzz").mkdir()
    (tmp_path / "fuzz/Cargo.toml").write_bytes((ROOT / "fuzz/Cargo.toml").read_bytes())
    mocker.patch.object(ci_fuzz, "ROOT", tmp_path)
    return tmp_path


@given(
    targets=st.integers(1, 64),
    seconds=st.integers(1, 600),
    fraction=st.floats(
        min_value=0, max_value=0.999, allow_nan=False, allow_infinity=False
    ),
)
@example(targets=5, seconds=50, fraction=0)
@example(targets=3, seconds=30, fraction=0)
@example(targets=1, seconds=1, fraction=0)
@example(targets=5, seconds=1, fraction=0.9)
@example(targets=5, seconds=1, fraction=4.9)
def test_shares_remaining_budget_with_startup_margin(
    targets: int, seconds: int, fraction: float
) -> None:
    remaining = targets * (seconds + 10) + fraction

    fuzz_seconds_result = ci_fuzz.fuzz_seconds(remaining, targets)

    assert fuzz_seconds_result == seconds


@given(
    remaining=st.floats(
        min_value=-1000, max_value=54.999999, allow_nan=False, allow_infinity=False
    )
)
@example(remaining=-1)
@example(remaining=0)
@example(remaining=10)
@example(remaining=54.9)
def test_insufficient_budget_cannot_be_success(remaining: float) -> None:
    with pytest.raises(ci_fuzz.FuzzError, match="budget"):
        ci_fuzz.fuzz_seconds(remaining, 5)


def test_command_failure_keeps_diagnostics(tmp_path: Path) -> None:
    campaign = ci_fuzz.Campaign(time.time() + 10, tmp_path, seed=123)

    with pytest.raises(ci_fuzz.FuzzError, match="broken"):
        campaign.run(
            "broken",
            [sys.executable, "-c", "print('failure detail'); raise SystemExit(23)"],
        )
    report = json.loads((tmp_path / "summary.json").read_text())

    assert report["stages"][0]["returncode"] == FAILURE_CODE
    assert "failure detail" in (tmp_path / "broken.log").read_text()
    assert report["status"] != "success"


@pytest.mark.skipif(os.name != "posix", reason="CI process groups require POSIX")
def test_timeout_terminates_child_processes(tmp_path: Path) -> None:
    sentinel = tmp_path / "child-survived"
    child_code = (
        "import time; from pathlib import Path; time.sleep(1); "
        f"Path({str(sentinel)!r}).touch()"
    )
    parent_code = (
        "import subprocess, sys, time; "
        f"subprocess.Popen([sys.executable, '-c', {child_code!r}]); "
        "print('started', flush=True); time.sleep(30)"
    )

    campaign = ci_fuzz.Campaign(time.time() + 10, tmp_path, seed=123)

    with pytest.raises(ci_fuzz.FuzzError, match="timeout"):
        campaign.run("timeout", [sys.executable, "-c", parent_code], timeout=0.5)
    time.sleep(1)

    report = json.loads((tmp_path / "summary.json").read_text())

    assert not sentinel.exists()
    assert "started" in (tmp_path / "timeout.log").read_text()
    assert report["stages"][0]["status"] == "timeout"


def test_expired_deadline_never_starts_command(tmp_path: Path) -> None:
    sentinel = tmp_path / "started"

    campaign = ci_fuzz.Campaign(time.time() - 1, tmp_path, seed=123)

    with pytest.raises(ci_fuzz.FuzzError, match="budget"):
        campaign.run(
            "expired",
            [sys.executable, "-c", f"open({str(sentinel)!r}, 'w').close()"],
        )

    assert not sentinel.exists()


def test_budget_uses_monotonic_time_after_initial_setup(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    clock = mocker.patch.object(ci_fuzz, "time", autospec=True)
    clock.time.return_value = 1000
    clock.monotonic.return_value = 100

    campaign = ci_fuzz.Campaign(1080, tmp_path, seed=1)
    # Setup already consumed 400 of 480 seconds. A wall-clock correction must
    # not restore that time or extend the remaining 80 seconds.
    clock.time.return_value = 500
    clock.monotonic.return_value = 130
    expected_remaining = 50

    observed_result = campaign.remaining()

    assert observed_result == expected_remaining


@pytest.mark.skipif(os.name != "posix", reason="Fuzz CI runs on Linux")
def test_campaign_stops_at_failure(fake_fuzz_root: Path, mocker: MockerFixture) -> None:
    mocker.patch.dict(os.environ, {"FAKE_FUZZ_FAIL_TARGET": "source_index"})
    report_dir = fake_fuzz_root / "report"

    campaign = ci_fuzz.Campaign(time.time() + 180, report_dir, seed=123)
    with pytest.raises(ci_fuzz.FuzzError, match="source_index"):
        ci_fuzz.run_campaign(campaign)

    assert campaign.completed_targets == ["source_encoding"]
    assert not (report_dir / "candidate_validation.log").exists()


@pytest.mark.skipif(os.name != "posix", reason="Fuzz CI runs on Linux")
def test_campaign_reports_all_targets(
    fake_fuzz_root: Path, mocker: MockerFixture
) -> None:
    mocker.patch.dict(os.environ, {"FAKE_FUZZ_FAIL_TARGET": ""})
    report_dir = fake_fuzz_root / "report"

    campaign = ci_fuzz.Campaign(time.time() + 180, report_dir, seed=123)

    ci_fuzz.run_campaign(campaign)

    report = json.loads((report_dir / "summary.json").read_text())
    stages = {stage["name"]: stage for stage in report["stages"]}

    target_argv = {name: stages[name]["argv"] for name in campaign.completed_targets}
    durations = [
        int(
            next(arg for arg in argv if arg.startswith("-max_total_time=")).split("=")[
                1
            ]
        )
        for argv in target_argv.values()
    ]

    assert campaign.completed_targets == [
        "source_encoding",
        "source_index",
        "candidate_validation",
        "python_analyzer",
        "run_stop",
        "disk_lifecycle",
        "report_lifecycle",
        "report_sequence",
        "target_resolution",
        "sampling",
        "budget_ledger",
        "python_reduce",
    ]
    assert {
        "generate-grammar-314",
        "generate-libcst-314",
        "generate-grammar-315",
        "generate-libcst-315",
        "generate-python315-315",
    } <= stages.keys()
    assert all(argv[8] == f"fuzz/seeds/{name}" for name, argv in target_argv.items())
    assert durations == sorted(durations)


@pytest.mark.skipif(os.name != "posix", reason="Fuzz CI runs on Linux")
def test_campaign_gives_every_target_the_requested_fixed_time(
    fake_fuzz_root: Path,
) -> None:
    report_dir = fake_fuzz_root / "report"

    campaign = ci_fuzz.Campaign(time.time() + 180, report_dir, seed=123)

    ci_fuzz.run_campaign(campaign, seconds_per_target=37)

    report = json.loads((report_dir / "summary.json").read_text())
    stages = {stage["name"]: stage for stage in report["stages"]}

    assert all(
        "-max_total_time=37" in stages[target]["argv"]
        for target in campaign.completed_targets
    )


def test_cli_failure_writes_report(tmp_path: Path) -> None:
    result = subprocess.run(  # noqa: S603 -- Repository CLI, fixed arguments.
        [
            sys.executable,
            str(ROOT / "tools/ci_fuzz.py"),
            "--deadline",
            "1",
            "--report",
            str(tmp_path),
            "--seed",
            "36158455427",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        timeout=10,
    )

    report = json.loads((tmp_path / "summary.json").read_text())

    assert result.returncode == 1
    assert report["status"] == "failure"
    assert "budget" in report["error"]
    assert 1 <= report["seed"] <= MAX_SEED
    assert report["completed_targets"] == []


@pytest.mark.parametrize("seconds", ["1", "37"])
def test_cli_accepts_positive_fixed_time_and_writes_report(
    tmp_path: Path, seconds: str
) -> None:
    result = subprocess.run(  # noqa: S603 -- Repository CLI, fixed arguments.
        [
            sys.executable,
            str(ROOT / "tools/ci_fuzz.py"),
            "--deadline",
            "1",
            "--report",
            str(tmp_path),
            "--seed",
            "36158455427",
            "--seconds-per-target",
            seconds,
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        timeout=10,
    )

    report = json.loads((tmp_path / "summary.json").read_text())

    assert result.returncode == 1
    assert report["status"] == "failure"
    assert "budget" in report["error"]
    assert report["completed_targets"] == []


@pytest.mark.parametrize("seconds", ["0", "-1"])
def test_cli_rejects_non_positive_fixed_time(tmp_path: Path, seconds: str) -> None:
    result = subprocess.run(  # noqa: S603 -- Repository CLI, fixed arguments.
        [
            sys.executable,
            str(ROOT / "tools/ci_fuzz.py"),
            "--deadline",
            "1",
            "--report",
            str(tmp_path),
            "--seed",
            "1",
            "--seconds-per-target",
            seconds,
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        timeout=10,
    )

    assert result.returncode == 2
    assert "positive" in result.stderr
    assert not (tmp_path / "summary.json").exists()


def test_workflow_runs_fuzz_in_parallel_with_bounded_total_time() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    )
    job = workflow["jobs"]["fuzz"]

    uploads = [
        step
        for step in job["steps"]
        if step.get("uses", "").startswith("actions/upload-artifact@")
    ]

    assert job["needs"] == ["changes", "quality"]
    assert job["if"] == "needs.changes.outputs.rust == 'true'"
    assert job["runs-on"] == "ubuntu-latest"
    assert job["timeout-minutes"] == JOB_MINUTES
    assert workflow["permissions"] == {"contents": "read"}
    assert "FUZZ_DEADLINE" in job["steps"][0]["run"]
    assert "+ 600" in job["steps"][0]["run"]
    assert len(uploads) == 1
    assert uploads[0]["if"] == "always()"
    assert "fuzz/artifacts" in uploads[0]["with"]["path"]


def test_campaign_generates_with_both_interpreters_before_one_fuzz_run(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    mocker.patch.object(ci_fuzz, "ROOT", tmp_path)
    (tmp_path / "fuzz").mkdir()
    (tmp_path / "fuzz/Cargo.toml").write_bytes((ROOT / "fuzz/Cargo.toml").read_bytes())

    campaign = ci_fuzz.Campaign(time.time() + 300, tmp_path / "report", seed=7)

    def create_environment(
        name: str,
        _argv: list[str],
        *,
        env: dict[str, str] | None = None,
        **_options: object,
    ) -> None:
        if name.startswith("dependencies-"):
            assert env is not None
            environment = Path(env["UV_PROJECT_ENVIRONMENT"])
            environment.mkdir(parents=True)
            (environment / "sentinel").write_text("temporary")

    run = mocker.patch.object(campaign, "run", side_effect=create_environment)

    ci_fuzz.run_campaign(campaign, seconds_per_target=1)

    calls = {call.args[0]: call for call in run.call_args_list}
    stage_names = {
        "3.14": [
            "dependencies-314",
            "generator-tests-314",
            "generate-grammar-314",
            "generate-libcst-314",
        ],
        "3.15": [
            "dependencies-315",
            "generator-tests-315",
            "generate-grammar-315",
            "generate-libcst-315",
            "generate-python315-315",
        ],
    }
    interpreters = {
        name: calls[name].args[1][calls[name].args[1].index("--python") + 1]
        for stages in stage_names.values()
        for name in stages
    }
    selected_environments = {
        version: {
            calls[name].kwargs["env"]["UV_PROJECT_ENVIRONMENT"] for name in stages
        }
        for version, stages in stage_names.items()
    }
    environments = {
        version: next(iter(selected))
        for version, selected in selected_environments.items()
    }
    names = [call.args[0] for call in run.call_args_list]

    assert interpreters == {
        name: version for version, stages in stage_names.items() for name in stages
    }
    assert all(len(selected) == 1 for selected in selected_environments.values())
    assert all(
        not Path(environment).exists() for environment in environments.values()
    ), "generation environments must be cleaned"
    assert environments["3.14"] != environments["3.15"]
    assert names.count("build") == 1
    assert names.index("generate-python315-315") < names.index("python_analyzer")
    assert campaign.completed_targets == [
        "source_encoding",
        "source_index",
        "candidate_validation",
        "python_analyzer",
        "run_stop",
        "disk_lifecycle",
        "report_lifecycle",
        "report_sequence",
        "target_resolution",
        "sampling",
        "budget_ledger",
        "python_reduce",
    ]


def test_child_environment_overlay_does_not_change_parent(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    mocker.patch.dict(os.environ, {"HOIMIN_FUZZ_OVERLAY": "parent"})

    campaign = ci_fuzz.Campaign(time.time() + 10, tmp_path, seed=1)
    campaign.run(
        "child-env",
        [sys.executable, "-c", "import os; print(os.environ['HOIMIN_FUZZ_OVERLAY'])"],
        env={"HOIMIN_FUZZ_OVERLAY": "child"},
    )
    report = json.loads((tmp_path / "summary.json").read_text())

    assert (tmp_path / "child-env.log").read_text().strip() == "child"
    assert os.environ["HOIMIN_FUZZ_OVERLAY"] == "parent"
    assert report["stages"][0]["environment"] == {"HOIMIN_FUZZ_OVERLAY": "child"}


def test_successful_campaign_cli_writes_success_report(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    original_run_campaign_spec = ci_fuzz.run_campaign

    def campaign(campaign: ci_fuzz.Campaign, *, seconds_per_target: int | None) -> None:
        assert seconds_per_target == 1
        campaign.completed_targets.append("completed-fixture")

    mocker.patch.object(
        ci_fuzz,
        "run_campaign",
        create_autospec(original_run_campaign_spec, side_effect=campaign),
    )
    mocker.patch.object(
        sys,
        "argv",
        [
            "ci-fuzz",
            "--deadline",
            str(int(time.time()) + 60),
            "--report",
            str(tmp_path),
            "--seed",
            "7",
            "--seconds-per-target",
            "1",
        ],
    )
    assert ci_fuzz.main() == 0
    report = json.loads((tmp_path / "summary.json").read_text(encoding="utf-8"))
    assert report["status"] == "success"
    assert report["error"] is None
    assert report["completed_targets"] == ["completed-fixture"]
