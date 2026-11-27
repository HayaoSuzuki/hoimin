"""Infrastructure failure must never count as detection of a model defect."""

import json
from pathlib import Path

import pytest

from tools import verify_quint
from tools.verify_quint import canonical_trace, validate_result


def test_canonical_trace_preserves_model_state(tmp_path: Path) -> None:
    # Arrange.
    path = tmp_path / "trace.json"
    path.write_text(
        json.dumps(
            {
                "#meta": {"timestamp": 99, "source": "/host/spec.qnt", "status": "ok"},
                "vars": ["amount"],
                "states": [{"amount": {"#bigint": "3"}}],
            }
        )
    )
    # Act.
    result = json.loads(canonical_trace(path))
    # Assert.
    assert result == {
        "#meta": {"format": "ITF", "status": "ok"},
        "vars": ["amount"],
        "states": [{"amount": {"#bigint": "3"}}],
    }


@pytest.mark.parametrize("code", [0, 2, 124, 125, 126, -9])
def test_negative_control_rejects_non_violation_exit(code: int) -> None:
    # Arrange.
    output = "[violation] found a counterexample"
    # Act / Assert.
    with pytest.raises(RuntimeError, match="counterexample"):
        validate_result(code, output, counterexample=True)


@pytest.mark.parametrize("output", ["", "[failure] TLC error", "java not found"])
def test_negative_control_rejects_infrastructure_failure(output: str) -> None:
    # Arrange.
    code = 1
    # Act / Assert.
    with pytest.raises(RuntimeError, match="counterexample"):
        validate_result(code, output, counterexample=True)


def test_negative_control_accepts_actual_violation() -> None:
    # Arrange.
    output = "[violation] found a counterexample"
    # Act.
    result = validate_result(1, output, counterexample=True)
    # Assert.
    assert result is None


@pytest.mark.parametrize(
    ("mutant", "index", "main"),
    [(None, 1, "DiskLifecycle"), ("DiskBadDelivery", 2, "DiskBadDelivery")],
)
def test_tlc_uses_projected_step_for_positive_and_negative_models(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    mutant: str | None,
    index: int,
    main: str,
) -> None:
    # Arrange.
    calls: list[list[str]] = []

    def tool(arguments: list[str], _directory: Path, _name: str) -> tuple[int, str]:
        calls.append(arguments)
        if "--main=DiskBadDelivery" in arguments:
            return 1, "[violation] found a counterexample"
        return 0, "[ok]"

    monkeypatch.setattr(verify_quint, "guarded", tool)
    monkeypatch.setattr(verify_quint, "simulate_model", lambda *_args, **_kwargs: None)
    model = verify_quint.Model(
        "disk.qnt", "DiskLifecycle", "disk", mutant, verification_step="verifyStep"
    )
    # Act.
    verify_quint.verify_model(model, tmp_path, update=False, simulate_only=False)
    # Assert.
    assert calls[index] == [
        "verify",
        str(verify_quint.FORMAL / "disk.qnt"),
        "--backend=tlc",
        "--apalache-version=0.62.4",
        f"--tlc-config={verify_quint.FORMAL / 'tlc.json'}",
        f"--main={main}",
        "--step=verifyStep",
        "--invariant=Safety",
        "--verbosity=3",
    ]


@pytest.mark.parametrize("code", [1, 124, 125, 126])
def test_positive_model_rejects_failed_check(code: int) -> None:
    # Arrange.
    output = "[ok] No violation found"
    # Act / Assert.
    with pytest.raises(RuntimeError, match="verification"):
        validate_result(code, output, counterexample=False)


@pytest.fixture
def model_tools(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> tuple[Path, dict[str, int]]:

    formal = tmp_path / "formal"
    formal.mkdir()
    executable = formal / "quint"
    executable.touch()
    monkeypatch.setattr(verify_quint, "FORMAL", formal)
    monkeypatch.setattr(verify_quint, "QUINT", executable)
    monkeypatch.setattr(
        verify_quint,
        "MODELS",
        (
            verify_quint.Model("first.qnt", "First", "first", "FirstMutant"),
            verify_quint.Model(
                "second.qnt", "Second", "second", scenarios=("targetStep",)
            ),
        ),
    )
    controls = {"negative_exit": 1, "forbid_tlc": 0}

    def tool(arguments: list[str], directory: Path, name: str) -> tuple[int, str]:
        del directory, name
        if arguments[0] == "verify":
            if controls["forbid_tlc"]:
                pytest.fail("simulate-only launched TLC")
            if "--main=FirstMutant" in arguments:
                return controls["negative_exit"], "[violation] found a counterexample"
        if arguments[0] == "run":
            pattern = next(
                arg.removeprefix("--out-itf=")
                for arg in arguments
                if arg.startswith("--out-itf=")
            )
            component = next(
                arg.removeprefix("--main=")
                for arg in arguments
                if arg.startswith("--main=")
            )
            for index in range(4):
                Path(pattern.replace("{seq}", str(index))).write_text(
                    json.dumps(
                        {
                            "#meta": {"status": "ok", "timestamp": 123},
                            "states": [
                                {"component": component, "amount": {"#bigint": "7"}}
                            ],
                        }
                    )
                )
        return 0, ""

    monkeypatch.setattr(verify_quint, "guarded", tool)
    return formal, controls


@pytest.mark.parametrize(
    ("prefix", "component"), [("first", "First"), ("second", "Second")]
)
def test_publishes_each_registered_model(
    model_tools: tuple[Path, dict[str, int]], prefix: str, component: str
) -> None:
    # Arrange.

    formal, _ = model_tools
    # Act.
    verify_quint.verify(update=True)
    # Assert.
    trace = json.loads((formal / f"traces/{prefix}_3.itf.json").read_text())
    assert trace["states"] == [{"component": component, "amount": {"#bigint": "7"}}]


def test_rejects_stale_oracle_in_later_model(
    model_tools: tuple[Path, dict[str, int]],
) -> None:
    # Arrange.

    formal, _ = model_tools
    verify_quint.verify(update=True)
    (formal / "traces/second_0.itf.json").write_text("{}")
    # Act / Assert.
    with pytest.raises(RuntimeError, match=r"Stale Quint oracle.*second_0"):
        verify_quint.verify(update=False)


@pytest.mark.parametrize("exit_code", [0, 2, 124, 125, 126])
def test_negative_tool_failure_stops_model_pipeline(
    model_tools: tuple[Path, dict[str, int]], exit_code: int
) -> None:
    # Arrange.

    _, controls = model_tools
    controls["negative_exit"] = exit_code
    # Act / Assert.
    with pytest.raises(RuntimeError, match="counterexample"):
        verify_quint.verify(update=True)


def test_simulate_only_publishes_without_tlc(
    model_tools: tuple[Path, dict[str, int]],
) -> None:
    # Arrange.

    formal, controls = model_tools
    controls["forbid_tlc"] = 1
    # Act.
    verify_quint.verify(update=True, simulate_only=True)
    # Assert.
    assert (formal / "traces/second_3.itf.json").is_file()


def test_simulation_only_model_publishes_without_tlc(
    model_tools: tuple[Path, dict[str, int]],
) -> None:
    # Arrange.
    formal, controls = model_tools
    controls["forbid_tlc"] = 1
    build = formal / "_build"
    build.mkdir()
    model = verify_quint.Model("first.qnt", "First", "first", simulation_only=True)
    # Act.
    verify_quint.verify_model(model, build, update=True, simulate_only=False)
    # Assert.
    assert (formal / "traces/first_3.itf.json").is_file()


@pytest.mark.parametrize(
    ("code", "output", "detected"),
    [
        (1, "[violation] Found an issue\nerror: Invariant violated", True),
        (1, "[violation] Found an issue\nerror: unsupported expression", False),
        (1, "error: Invariant violated", False),
        (0, "[ok] No violation found", False),
        (125, "[violation] Found an issue\nerror: Invariant violated", False),
    ],
)
def test_model_requires_a_semantic_simulation_violation(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
    code: int,
    output: str,
    *,
    detected: bool,
) -> None:
    # Arrange.
    calls: list[list[str]] = []

    def tool(arguments: list[str], _directory: Path, _name: str) -> tuple[int, str]:
        calls.append(arguments)
        return (0, "") if arguments[0] == "typecheck" else (code, output)

    monkeypatch.setattr(verify_quint, "guarded", tool)
    monkeypatch.setattr(verify_quint, "simulate_model", lambda *_args, **_kwargs: None)
    model = verify_quint.Model(
        "first.qnt",
        "First",
        "first",
        simulation_only=True,
        simulation_mutant_step="brokenStep",
    )
    # Act / Assert.
    if detected:
        verify_quint.verify_model(model, tmp_path, update=False, simulate_only=True)
    else:
        with pytest.raises(RuntimeError, match="semantic simulation violation"):
            verify_quint.verify_model(model, tmp_path, update=False, simulate_only=True)
    assert [call[0] for call in calls] == ["typecheck", "run"]
    assert "--step=brokenStep" in calls[1]
    assert "--invariant=Safety" in calls[1]


def test_publishes_targeted_model_schedule(
    model_tools: tuple[Path, dict[str, int]],
) -> None:
    # Arrange.
    formal, _ = model_tools
    # Act.
    verify_quint.verify(update=True)
    # Assert.
    trace = json.loads((formal / "traces/second_targetStep_0.itf.json").read_text())
    assert trace["states"][0]["component"] == "Second"


def test_rejects_stale_targeted_schedule(
    model_tools: tuple[Path, dict[str, int]],
) -> None:
    # Arrange.
    formal, _ = model_tools
    verify_quint.verify(update=True)
    (formal / "traces/second_targetStep_0.itf.json").write_text("{}")
    # Act / Assert.
    with pytest.raises(RuntimeError, match=r"Stale Quint oracle.*second_targetStep_0"):
        verify_quint.verify(update=False)
