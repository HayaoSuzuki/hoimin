"""Bounded research runs; source restoration follows process-tree termination."""

import ctypes
import hashlib
import importlib.metadata
import json
import os
import platform
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from contextlib import contextmanager
from ctypes import wintypes
from datetime import datetime, timezone
from pathlib import Path

import blake3


class ContainmentError(BaseException):
    """Fatal: a working copy may still be in use and must remain quarantined."""


HERE = Path(__file__).resolve().parent
BOOTSTRAP = (
    "import subprocess,sys; token=sys.stdin.readline(); "
    "sys.exit(subprocess.call(sys.argv[1:]) if token == 'start\\n' else 125)"
)


class WindowsJob:
    """Own a process tree before the bootstrap is allowed to launch target code."""

    def __init__(self, pid):
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        self.kernel = kernel
        self.handle = None
        size = ctypes.c_size_t

        class Basic(ctypes.Structure):
            _fields_ = [
                ("user", ctypes.c_longlong),
                ("job_user", ctypes.c_longlong),
                ("flags", wintypes.DWORD),
                ("min_ws", size),
                ("max_ws", size),
                ("active", wintypes.DWORD),
                ("affinity", size),
                ("priority", wintypes.DWORD),
                ("scheduling", wintypes.DWORD),
            ]

        class Extended(ctypes.Structure):
            _fields_ = [
                ("basic", Basic),
                ("io", ctypes.c_ulonglong * 6),
                ("process_memory", size),
                ("job_memory", size),
                ("peak_process", size),
                ("peak_job", size),
            ]

        class Accounting(ctypes.Structure):
            _fields_ = [
                ("times", ctypes.c_longlong * 4),
                ("faults", wintypes.DWORD),
                ("total", wintypes.DWORD),
                ("active", wintypes.DWORD),
                ("terminated", wintypes.DWORD),
            ]

        self.Accounting = Accounting
        signatures = {
            "CreateJobObjectW": ([ctypes.c_void_p, wintypes.LPCWSTR], wintypes.HANDLE),
            "SetInformationJobObject": (
                [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD],
                wintypes.BOOL,
            ),
            "QueryInformationJobObject": (
                [
                    wintypes.HANDLE,
                    ctypes.c_int,
                    ctypes.c_void_p,
                    wintypes.DWORD,
                    ctypes.c_void_p,
                ],
                wintypes.BOOL,
            ),
            "OpenProcess": (
                [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD],
                wintypes.HANDLE,
            ),
            "AssignProcessToJobObject": (
                [wintypes.HANDLE, wintypes.HANDLE],
                wintypes.BOOL,
            ),
            "TerminateJobObject": ([wintypes.HANDLE, wintypes.UINT], wintypes.BOOL),
            "CloseHandle": ([wintypes.HANDLE], wintypes.BOOL),
        }
        for name, (args, result) in signatures.items():
            function = getattr(kernel, name)
            function.argtypes, function.restype = args, result
        self.handle = kernel.CreateJobObjectW(None, None)
        if not self.handle:
            raise ctypes.WinError(ctypes.get_last_error())
        process = None
        try:
            limits = Extended()
            limits.basic.flags = (
                0x2000 | 0x200 | 0x8
            )  # kill-on-close, job memory, process count
            limits.basic.active = 16
            limits.job_memory = 1024 * 1024 * 1024
            if not kernel.SetInformationJobObject(
                self.handle, 9, ctypes.byref(limits), ctypes.sizeof(limits)
            ):
                raise ctypes.WinError(ctypes.get_last_error())
            process = kernel.OpenProcess(0x100 | 0x1, False, pid)
            if not process or not kernel.AssignProcessToJobObject(self.handle, process):
                raise ctypes.WinError(ctypes.get_last_error())
        except BaseException:
            kernel.CloseHandle(self.handle)
            self.handle = None
            raise
        finally:
            if process:
                kernel.CloseHandle(process)

    def close(self):
        if self.handle is None:
            return
        try:
            if not self.kernel.TerminateJobObject(self.handle, 125):
                raise ctypes.WinError(ctypes.get_last_error())
            deadline = time.monotonic() + 3
            while True:
                info = self.Accounting()
                if not self.kernel.QueryInformationJobObject(
                    self.handle, 1, ctypes.byref(info), ctypes.sizeof(info), None
                ):
                    raise ctypes.WinError(ctypes.get_last_error())
                if info.active == 0:
                    break
                if time.monotonic() >= deadline:
                    raise ContainmentError("process-tree termination unconfirmed")
                time.sleep(0.01)
        except OSError as error:
            raise ContainmentError("cannot confirm process-tree termination") from error
        finally:
            self.kernel.CloseHandle(self.handle)
            self.handle = None


def command(argv, cwd, deadline, *, timeout=20, env=None, output_limit=262144):
    """Run within remaining wall budget; never return with an owned Windows child."""
    started = time.monotonic()
    if started >= deadline:
        return {
            "termination": "budget_exhausted",
            "seconds": 0,
            "stdout": "",
            "stderr": "",
            "returncode": None,
        }
    end = min(deadline, started + timeout)
    termination = "exit"
    job = None
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        process = subprocess.Popen(
            [sys.executable, "-c", BOOTSTRAP, *map(str, argv)],
            cwd=cwd,
            env=env,
            stdin=subprocess.PIPE,
            stdout=stdout,
            stderr=stderr,
            start_new_session=os.name != "nt",
        )
        try:
            if os.name == "nt":
                job = WindowsJob(process.pid)
            process.stdin.write(b"start\n")
            process.stdin.close()
            while process.poll() is None:
                if time.monotonic() >= end:
                    termination = "budget_exhausted" if end == deadline else "timeout"
                    break
                if (
                    os.fstat(stdout.fileno()).st_size
                    + os.fstat(stderr.fileno()).st_size
                    > output_limit
                ):
                    termination = "output_limit"
                    break
                time.sleep(0.01)
        finally:
            if job:
                job.close()
            elif os.name != "nt":
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            elif process.poll() is None:
                process.kill()
            try:
                process.wait(timeout=3)
            except (OSError, subprocess.TimeoutExpired) as error:
                raise ContainmentError("bootstrap termination unconfirmed") from error
        if (
            termination == "exit"
            and os.fstat(stdout.fileno()).st_size + os.fstat(stderr.fileno()).st_size
            > output_limit
        ):
            termination = "output_limit"
        stdout.seek(0)
        stderr.seek(0)
        return {
            "termination": termination,
            "returncode": process.returncode,
            "seconds": time.monotonic() - started,
            "stdout": stdout.read(output_limit).decode("utf-8", "replace"),
            "stderr": stderr.read(output_limit).decode("utf-8", "replace"),
        }


def observe(
    root,
    tests,
    sources,
    deadline,
    *,
    coverage=False,
    import_roots=None,
    extra_pythonpath=(),
):
    """Read a complete observation only after its child tree has stopped."""
    with tempfile.TemporaryDirectory(prefix="localization-observation-") as folder:
        output = Path(folder) / "result.json"
        request = Path(folder) / "request.json"
        request.write_text(
            json.dumps(
                {
                    "tests": tests,
                    "sources": sources,
                    "coverage": coverage,
                    "import_roots": import_roots or ["."],
                    "output": str(output),
                }
            ),
            encoding="utf-8",
        )
        environment = {
            **os.environ,
            "PYTEST_DISABLE_PLUGIN_AUTOLOAD": "1",
            "PYTHONDONTWRITEBYTECODE": "1",
            "PYTHONHASHSEED": "0",
            "PYTHONUTF8": "1",
            "COVERAGE_CORE": "ctrace",
            "PYTHONPYCACHEPREFIX": str(Path(folder) / "uncached"),
            "PYTHONPATH": os.pathsep.join(map(str, extra_pythonpath)),
        }
        receipt = command(
            [sys.executable, str(HERE / "collect.py"), str(request)],
            root,
            deadline,
            env=environment,
        )
        if (
            receipt["termination"] != "exit"
            or receipt["returncode"] != 0
            or not output.is_file()
        ):
            return {"complete": False, "tests": {}, "process": receipt}
        result = json.loads(output.read_text(encoding="utf-8"))
        result["process"] = receipt
        return result


@contextmanager
def applied(path, candidate):
    """Apply one checked UTF-8 byte span; restore even when execution fails."""
    original = path.read_bytes()
    start, length = candidate["span"]["start"], candidate["span"]["length"]
    if (
        type(start) is not int
        or type(length) is not int
        or start < 0
        or length < 0
        or start + length > len(original)
        or blake3.blake3(original).hexdigest() != candidate["file_hash"]
        or original[start : start + length] != candidate["original"].encode("utf-8")
    ):
        raise ValueError("stale source or invalid candidate byte span")
    mutated = (
        original[:start]
        + candidate["replacement"].encode("utf-8")
        + original[start + length :]
    )
    compile(mutated, str(path), "exec")
    quarantined = False
    try:
        path.write_bytes(mutated)
        yield
    except ContainmentError:
        quarantined = True
        raise
    finally:
        if not quarantined:
            path.write_bytes(original)


def select_candidates(candidates, eligible, limit):
    """Round-robin over a frozen hash order of eligible lines, without fault labels."""
    groups = {}
    for candidate in candidates:
        if candidate["line"] in eligible:
            groups.setdefault(candidate["line"], []).append(candidate)
    lines = sorted(
        groups, key=lambda line: hashlib.sha256(f"1188:{line}".encode()).digest()
    )
    for group in groups.values():
        group.sort(key=lambda candidate: candidate["id"])
    ordered = []
    depth = 0
    while len(ordered) < limit:
        row = [groups[line][depth] for line in lines if len(groups[line]) > depth]
        if not row:
            break
        ordered.extend(row[: limit - len(ordered)])
        depth += 1
    return ordered


def _evaluate_copy(
    root,
    case,
    binary,
    output,
    *,
    extra_pythonpath=(),
    budget=600,
    limit=96,
    started=None,
):
    """Evaluate a frozen historical case; interrupted work stays explicitly partial."""
    from analysis import stable_run, summarize

    start = started if started is not None else time.monotonic()
    deadline = start + budget
    source = root / case["source"]
    initial = source.read_bytes()
    fixed = (root / "fixed-source.bin").read_bytes()
    buggy = (root / "buggy-source.bin").read_bytes()
    record = {
        "case": case,
        "budget_seconds": budget,
        "candidate_limit": limit,
        "complete": False,
        "variants": [],
        "source_sha256": hashlib.sha256(buggy).hexdigest(),
        "fixed_source_sha256": hashlib.sha256(fixed).hexdigest(),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "environment": {
            "recorded_at": datetime.now(timezone.utc).isoformat(),
            "python": sys.version,
            "platform": platform.platform(),
            "packages": {
                distribution.metadata["Name"]: distribution.version
                for distribution in importlib.metadata.distributions(
                    path=[*map(str, extra_pythonpath), *sys.path]
                )
                if distribution.metadata["Name"].lower()
                in {
                    "pytest",
                    "hypothesis",
                    "coverage",
                    "blake3",
                    "werkzeug",
                    "markupsafe",
                    "requests",
                    "urllib3",
                    "certifi",
                    "charset-normalizer",
                    "idna",
                }
            },
        },
        "harness_sha256": {
            name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
            for name in (
                "analysis.py",
                "collect.py",
                "evaluate.py",
                "prepare_public.py",
            )
        },
    }
    output.parent.mkdir(parents=True, exist_ok=True)

    def save():
        record["seconds"] = time.monotonic() - start
        output.write_text(json.dumps(record, ensure_ascii=False), encoding="utf-8")

    def sample(measure=False):
        return observe(
            root,
            case["tests"],
            [case["source"]],
            deadline,
            coverage=measure,
            import_roots=case.get("import_roots"),
            extra_pythonpath=extra_pythonpath,
        )

    quarantined = False
    try:
        source.write_bytes(fixed)
        record["fixed"] = [sample(), sample()]
        fixed_observation = stable_run(record["fixed"])
        if any(
            item["call"] != "passed" for item in fixed_observation["tests"].values()
        ):
            raise ValueError("fixed revision fails selected tests")
        source.write_bytes(buggy)
        record["baseline"] = [sample(True), sample(True)]
        baseline = dict(stable_run(record["baseline"], mode="outcome"))
        try:
            stable_run(record["baseline"])
            baseline["fingerprint_stable"] = True
        except ValueError as error:
            baseline["fingerprint_stable"] = False
            record["fingerprint_baseline_error"] = str(error)
        if baseline["tests"].keys() != fixed_observation["tests"].keys():
            raise ValueError("fixed and buggy test identities differ")
        failed = {
            name for name, item in baseline["tests"].items() if item["call"] == "failed"
        }
        if not failed or len(failed) == len(baseline["tests"]):
            raise ValueError("baseline needs passing and failing tests")
        plan_run = command(
            [
                binary,
                "plan",
                "--root",
                root,
                "--file",
                case["source"],
                "--max-candidates",
                "10000",
                "--max-mutants",
                str(limit),
                "--",
                sys.executable,
                "-m",
                "pytest",
                *case["tests"],
            ],
            root,
            deadline,
            timeout=30,
            output_limit=16 * 1024 * 1024,
        )
        record["plan_process"] = {k: v for k, v in plan_run.items() if k != "stdout"}
        if plan_run["termination"] != "exit" or plan_run["returncode"] != 0:
            raise ValueError("candidate generation failed")
        plan = json.loads(plan_run["stdout"])
        record["plan"] = plan
        if any(c["path"] != case["source"] for c in plan["candidates"]):
            raise ValueError("candidate escaped selected file")
        spectra = baseline["coverage"][case["source"]]
        eligible = {
            int(line) for line, ids in spectra.items() if set(ids) & (failed | {""})
        }
        selected = select_candidates(plan["candidates"], eligible, limit)
        record["selected_ids"] = [c["id"] for c in selected]
        record["eligible_candidates"] = sum(
            c["line"] in eligible for c in plan["candidates"]
        )
        record["sampled_lines"] = sorted({c["line"] for c in selected})
        record["eligible_lines"] = sorted(eligible)
        record["ranking_scope"] = "all executable lines in the selected module"
        record["preparation_seconds"] = time.monotonic() - start
        save()
        for index, candidate in enumerate(selected):
            if time.monotonic() >= deadline:
                record["stop_reason"] = "budget_exhausted"
                break
            variant = {"candidate": candidate, "observations": []}
            record["variants"].append(variant)
            try:
                with applied(source, candidate):
                    for _ in range(2):
                        if time.monotonic() >= deadline:
                            break
                        variant["observations"].append(sample())
            except SyntaxError:
                variant["invalid"] = "syntax"
            save()
            if (index + 1) % 10 == 0:
                print(
                    json.dumps(
                        {
                            "case": case["name"],
                            "attempted": index + 1,
                            "seconds": round(record["seconds"], 1),
                        }
                    ),
                    flush=True,
                )
        record["complete"], record["stop_reason"] = completion(
            record["variants"], len(selected)
        )
        record["analysis"] = summarize(
            baseline, record["variants"], case["source"], set(case["targets"])
        )
        record["sample_exhaustive"] = (
            len(selected) == record["eligible_candidates"] and not plan["truncated"]
        )
    except ContainmentError:
        quarantined = True
        record["error"] = (
            "fatal: process-tree termination unconfirmed; copy quarantined"
        )
        raise
    except (ValueError, RuntimeError) as error:
        record["error"] = str(error)
    finally:
        if not quarantined:
            source.write_bytes(initial)
        save()
    return record


def main():
    import argparse

    parser = argparse.ArgumentParser(
        description="Run the independent localization experiment"
    )
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--projects", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--extra-pythonpath", type=Path, action="append", default=[])
    args = parser.parse_args()
    for case in json.loads(args.manifest.read_text(encoding="utf-8")):
        result = evaluate_case(
            args.projects.resolve() / case["name"],
            case,
            args.binary.resolve(),
            args.output / (case["name"] + ".json"),
            extra_pythonpath=[p.resolve() for p in args.extra_pythonpath],
        )
        print(
            json.dumps(
                {
                    "case": case["name"],
                    "complete": result["complete"],
                    "error": result.get("error"),
                    "seconds": result["seconds"],
                    "attempted": len(result["variants"]),
                }
            ),
            flush=True,
        )


def completion(variants, selected_count):
    """Completion means the frozen sample was attempted, not all mutants comparable."""
    if any(
        observation.get("process", {}).get("termination") == "budget_exhausted"
        for variant in variants
        for observation in variant["observations"]
    ):
        return False, "budget_exhausted"
    if len(variants) != selected_count or any(
        len(variant["observations"]) != 2 and variant.get("invalid") != "syntax"
        for variant in variants
    ):
        return False, "incomplete_sample"
    return True, "sample_attempted"


@contextmanager
def project_copy(root):
    """Mutations run in an owned copy; fatal containment errors quarantine it."""
    parent = Path(tempfile.gettempdir()).resolve()
    folder = Path(tempfile.mkdtemp(prefix="localization-case-", dir=parent)).resolve()
    quarantined = False
    try:
        shutil.copytree(
            root,
            folder,
            dirs_exist_ok=True,
            ignore=shutil.ignore_patterns(
                ".git", ".venv", "__pycache__", "*.pyc", ".pytest_cache"
            ),
        )
        yield folder
    except ContainmentError:
        quarantined = True
        raise
    finally:
        if not quarantined:
            if folder.parent != parent or not folder.name.startswith(
                "localization-case-"
            ):
                raise RuntimeError("refusing cleanup outside owned temporary directory")
            shutil.rmtree(folder)


def evaluate_case(root, case, binary, output, **kwargs):
    """Keep input checkout immutable and charge copying to the case budget."""
    started = time.monotonic()
    receipt = validate_prepared(root, case)
    started -= receipt.get("export_seconds", 0.0)
    with project_copy(root) as copied:
        validate_prepared(copied, case)
        result = _evaluate_copy(copied, case, binary, output, started=started, **kwargs)
        result["prepared_inputs"] = receipt
        output.write_text(json.dumps(result, ensure_ascii=False), encoding="utf-8")
        return result


def validate_prepared(root, case):
    """Validate the pinned preparation receipt before using a project copy."""
    root = root.resolve()
    receipt = json.loads((root / "prepared.json").read_text(encoding="utf-8"))
    if receipt["case"] != case:
        raise ValueError("prepared case metadata changed")
    for name, digest in receipt["files"].items():
        path = (root / name).resolve()
        if (
            not path.is_relative_to(root)
            or not path.is_file()
            or hashlib.sha256(path.read_bytes()).hexdigest() != digest
        ):
            raise ValueError("prepared input changed: " + name)
    actual = {
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_file()
        and path.name != "prepared.json"
        and not any(
            part in {".git", ".venv", "__pycache__", ".pytest_cache"}
            for part in path.relative_to(root).parts
        )
        and path.suffix != ".pyc"
    }
    if actual != receipt["files"].keys():
        raise ValueError("prepared input set changed")
    source = (root / case.get("source", "")).resolve()
    if not source.is_relative_to(root):
        raise ValueError("source is outside the prepared project")
    return receipt


if __name__ == "__main__":
    main()
