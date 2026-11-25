"""Exercise the live-generator boundary without treating tool failures as success."""

import json
import sys
from pathlib import Path

import pytest

from tools import verify_quint_connect as bridge


@pytest.fixture
def guarded_tool(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    guard = tmp_path / "formal/HoiminOracle/tools/lean_resource_guard.py"
    guard.parent.mkdir(parents=True)
    guard.write_text(
        "import json,os,sys\n"
        "from pathlib import Path\n"
        "Path(os.environ['CONNECT_ARGV']).write_text(json.dumps(sys.argv[1:]))\n"
        "Path(os.environ['CONNECT_ARGV']).with_suffix('.env.json').write_text(json.dumps("
        "{k:os.environ.get(k) for k in ('PATH','CARGO_TARGET_DIR',"
        "'CARGO_BUILD_JOBS')}))\n"
        "raise SystemExit(int(os.environ['CONNECT_EXIT']))\n"
    )
    output = tmp_path / "argv.json"
    monkeypatch.setattr(bridge, "ROOT", tmp_path)
    monkeypatch.setattr(bridge, "BUILD", tmp_path / "build")
    monkeypatch.setenv("HOIMIN_QUINT_PYTHON", sys.executable)
    monkeypatch.setenv("CONNECT_ARGV", str(output))
    return output


@pytest.mark.usefixtures("guarded_tool")
@pytest.mark.parametrize("code", [0, 1, 124, 125, 126])
def test_live_bridge_propagates_guard_exit(
    monkeypatch: pytest.MonkeyPatch, code: int
) -> None:
    # Arrange.
    monkeypatch.setenv("CONNECT_EXIT", str(code))
    # Act.
    actual = bridge.quint_cli(["run", "spec.qnt", "--seed", "1172"])
    # Assert.
    assert actual == code


def test_live_bridge_pins_backend_safety_and_resource_guard(
    guarded_tool: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    # Arrange.
    monkeypatch.setenv("CONNECT_EXIT", "0")
    # Act.
    bridge.quint_cli(["run", "spec.qnt"])
    # Assert.
    argv = json.loads(guarded_tool.read_text())
    assert argv[:6] == [
        "--timeout-seconds",
        "20",
        "--rss-limit-mib",
        "2048",
        "--sample-ms",
        "50",
    ]
    assert argv[-4:] == [
        "run",
        "spec.qnt",
        "--backend=typescript",
        "--invariant=Safety",
    ]
    assert argv[argv.index("--") + 1] == str(
        bridge.ROOT / "formal/quint/node_modules/.bin/quint"
    )


@pytest.mark.parametrize("arguments", [[], ["test"], ["verify"], ["compile"]])
def test_live_bridge_rejects_uncontrolled_cli_modes(arguments: list[str]) -> None:
    # Arrange.
    command = arguments
    # Act / Assert.
    with pytest.raises(ValueError, match="only quint run"):
        bridge.quint_cli(command)


def test_live_verification_guards_cargo_and_descendants(
    guarded_tool: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    # Arrange.
    monkeypatch.setenv("CONNECT_EXIT", "126")
    monkeypatch.setattr(bridge.shutil, "which", lambda _name: "/controlled/cargo")
    # Act.
    result = bridge.verify()
    # Assert.
    argv = json.loads(guarded_tool.read_text())
    environment = json.loads(guarded_tool.with_suffix(".env.json").read_text())
    assert result == 126
    assert argv[:4] == ["--timeout-seconds", "300", "--rss-limit-mib", "2048"]
    assert argv[argv.index("--") + 1 : argv.index("--") + 5] == [
        "/controlled/cargo",
        "test",
        "--locked",
        "--manifest-path",
    ]
    assert "--all-features" in argv
    assert argv[-2:] == ["--", "--test-threads=1"]
    assert environment["CARGO_BUILD_JOBS"] == "2"
    assert environment["CARGO_TARGET_DIR"] == str(bridge.ROOT / "target/quint-connect")
    assert environment["PATH"].split(bridge.os.pathsep)[0] == str(
        bridge.CONNECT / "bin"
    )
