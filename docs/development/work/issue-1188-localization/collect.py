"""Observe a single pytest run without changing tests or interpreting repairs."""

import hashlib
import json
import sys
from pathlib import Path

import coverage
import pytest
from analysis import _validated_tests


class Observation:
    def __init__(self, root, sources, measure):
        self.root = root
        self.sources = sources
        self.tests = {}
        self.identities = []
        self.errors = []
        self.tracebacks = {}
        self.coverage = (
            coverage.Coverage(
                data_file=None,
                config_file=False,
                include=[str(root / name) for name in sources],
            )
            if measure and sources
            else None
        )

    def pytest_collection_finish(self, session):
        self.identities = [item.nodeid for item in session.items]
        if len(set(self.identities)) != len(self.identities):
            self.errors.append("duplicate test identities")

    def pytest_collectreport(self, report):
        if report.failed:
            self.errors.append("collection failure")

    @pytest.hookimpl(hookwrapper=True)
    def pytest_runtest_protocol(self, item, nextitem):
        if self.coverage:
            self.coverage.switch_context(item.nodeid)
        yield
        if self.coverage:
            self.coverage.switch_context("")

    @pytest.hookimpl(hookwrapper=True)
    def pytest_runtest_makereport(self, item, call):
        outcome = yield
        report = outcome.get_result()
        observation = self.tests.setdefault(
            item.nodeid, {"fingerprint": None, "special": False}
        )
        if report.when in observation:
            self.errors.append("duplicate phase")
        observation[report.when] = report.outcome
        if hasattr(report, "wasxfail") or report.outcome not in {"passed", "failed"}:
            observation["special"] = True
        if report.when == "call" and report.failed and call.excinfo is not None:
            error_type = call.excinfo.type
            message = str(call.excinfo.value)
            # Only exception identity/message: mutated traceback source is not output.
            payload = (
                error_type.__module__ + "." + error_type.__qualname__ + "\n" + message
            )
            observation["fingerprint"] = hashlib.sha256(
                payload.encode("utf-8")
            ).hexdigest()
            frames = []
            for entry in call.excinfo.traceback:
                try:
                    name = (
                        Path(str(entry.path))
                        .resolve()
                        .relative_to(self.root)
                        .as_posix()
                    )
                except ValueError:
                    continue
                if name in self.sources:
                    frames.append([name, entry.lineno + 1])
            self.tracebacks[item.nodeid] = frames

    def result(self, code):
        matrix = {}
        statements = {}
        if self.coverage:
            self.coverage.stop()
            data = self.coverage.get_data()
            for name in self.sources:
                filename = str(self.root / name)
                contexts = data.contexts_by_lineno(filename)
                matrix[name] = {
                    str(line): sorted(ids) for line, ids in sorted(contexts.items())
                }
                statements[name] = self.coverage.analysis2(filename)[1]
        record = {
            "complete": code in (0, 1)
            and not self.errors
            and set(self.tests) == set(self.identities),
            "tests": self.tests,
            "coverage": matrix,
            "statements": statements,
            "tracebacks": self.tracebacks,
            "errors": self.errors,
            "exit_code": int(code),
        }
        try:
            _validated_tests(record)
        except ValueError as error:
            record["complete"] = False
            record["errors"].append(str(error))
        return record


def main():
    request = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    root = Path.cwd().resolve()
    for import_root in reversed(request.get("import_roots", ["."])):
        sys.path.insert(0, str(root / import_root))
    observation = Observation(root, request["sources"], request["coverage"])
    if observation.coverage:
        observation.coverage.start()
    code = pytest.main(
        [
            "-q",
            "--tb=short",
            "-o",
            "addopts=",
            "-p",
            "no:cacheprovider",
            *request["tests"],
        ],
        plugins=[observation],
    )
    Path(request["output"]).write_text(
        json.dumps(observation.result(code), ensure_ascii=False), encoding="utf-8"
    )


if __name__ == "__main__":
    main()
