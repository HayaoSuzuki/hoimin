"""Run the pinned workflow linters over every workflow, including new files."""

from __future__ import annotations

import re
import shutil
import subprocess
from pathlib import Path

import yaml

from tools.ci_selection import object_mapping


def default_shell(scope: dict[str, object], fallback: str) -> str:
    defaults = object_mapping(scope.get("defaults", {}))
    run = object_mapping(defaults.get("run", {}))
    return str(run.get("shell", fallback)).split()[0]


def check_shells(paths: list[Path], shellcheck: str) -> int:
    """Windows workaround for actionlint writing large stdin before process start."""
    failed = False
    for path in paths:
        document = yaml.safe_load(path.read_text(encoding="utf-8"))
        jobs = object_mapping(document["jobs"])
        for job_id, raw_job in jobs.items():
            job = object_mapping(raw_job)
            runner = str(job.get("runs-on", ""))
            fallback = "pwsh" if "windows" in runner else "bash"
            shell = default_shell(job, default_shell(document, fallback))
            steps = job.get("steps", [])
            if not isinstance(steps, list):
                raise TypeError
            for index, raw_step in enumerate(steps):
                step = object_mapping(raw_step)
                script = step.get("run")
                step_shell = str(step.get("shell", shell)).split()[0]
                if not isinstance(script, str) or step_shell not in {"bash", "sh"}:
                    continue
                # actionlint validates expressions; zizmor checks their trust.
                script = re.sub(r"\$\{\{.*?\}\}", "expression", script, flags=re.DOTALL)
                result = subprocess.run(  # noqa: S603 -- resolved executable and argv, no shell
                    [
                        shellcheck,
                        "--norc",
                        "--shell",
                        step_shell,
                        "-x",
                        "-",
                    ],
                    input=script.encode("utf-8"),
                    check=False,
                    timeout=30,
                )
                if result.returncode:
                    print(f"ShellCheck: {path}, job {job_id}, step {index + 1}")  # noqa: T201 -- CLI output contract
                    failed = True
    return int(failed)


def main() -> int:
    workflows = sorted(
        str(path)
        for path in Path(".github/workflows").iterdir()
        if path.suffix in {".yml", ".yaml"}
    )
    if not workflows:
        msg = "No workflows found"
        raise RuntimeError(msg)
    for tool in ("actionlint", "shellcheck", "zizmor"):
        if shutil.which(tool) is None:
            msg = f"Missing {tool}; install the workflow dependency group"
            raise RuntimeError(msg)
    shellcheck = shutil.which("shellcheck")
    assert shellcheck is not None
    commands = [
        [
            "actionlint",
            "-shellcheck",
            # actionlint 1.7.12 deadlocks on Windows for scripts larger than
            # the stdin pipe buffer. Run strict ShellCheck separately on all OSes.
            "",
            "-pyflakes",
            "",
            *workflows,
        ],
        [
            "zizmor",
            "--offline",
            "--no-progress",
            "--persona",
            "auditor",
            "--format",
            "plain",
            *workflows,
        ],
    ]
    failed = False
    for command in commands:
        result = subprocess.run(command, check=False, timeout=180)  # noqa: S603 -- resolved executable and argv, no shell
        failed |= result.returncode != 0
    failed |= bool(check_shells([Path(path) for path in workflows], shellcheck))
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
