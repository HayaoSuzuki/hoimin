import unittest
from tools.boundary_contracts import classify


class BoundaryRunnerTests(unittest.TestCase):
    def test_only_exact_executed_test_is_a_match(self):
        self.assertEqual(classify(0, 'test wanted ... ok\ntest result: ok. 1 passed;', 'wanted'), 'match')
        self.assertEqual(classify(0, 'test result: ok. 0 passed;', 'wanted'), 'infrastructure-error')
        self.assertEqual(classify(0, 'test other ... ok', 'wanted'), 'infrastructure-error')

    def test_skip_and_crash_are_not_semantic_success(self):
        self.assertEqual(classify(0, 'SKIP: backend unavailable\ntest wanted ... ok', 'wanted'), 'unexecuted')
        self.assertEqual(classify(0, 'BOUNDARY_OBSERVATION {"status":"unexecuted"}\ntest wanted ... ok\ntest result: ok. 1 passed;', 'wanted'), 'unexecuted')
        self.assertEqual(classify(-9, '', 'wanted'), 'infrastructure-error')
        self.assertEqual(classify(101, 'test wanted ... FAILED', 'wanted'), 'mismatch')
        self.assertEqual(classify(101, 'error: compilation failed', 'wanted'), 'infrastructure-error')

    def test_semantic_matrix_errors_override_outer_test_failure(self):
        self.assertEqual(classify(101, 'BOUNDARY_OBSERVATION {"status":"infrastructure-error"}\ntest wanted ... FAILED', 'wanted'), 'infrastructure-error')

    def test_captured_case_output_can_interrupt_the_named_test_line(self):
        log = 'test wanted ... BOUNDARY_OBSERVATION {"status":"match"}\nok\ntest result: ok. 1 passed;'
        self.assertEqual(classify(0, log, 'wanted'), 'match')

    def test_report_mode_preserves_failure_and_complete_unexecuted_rows(self):
        import hashlib
        import json
        import tempfile
        from pathlib import Path
        from tools.boundary_contracts import REGISTRY, load_registry, main
        rows = [{"id": case["id"], "mode": case["mode"], "status": "mismatch" if case["mode"] == "strict" else "unexecuted"} for case in load_registry()["cases"]]
        report = {"schema": 1, "registry_sha256": hashlib.sha256(REGISTRY.read_bytes()).hexdigest(), "rows": rows}
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "captured.json"
            source.write_text(json.dumps(report))
            self.assertEqual(main(["report", "--from-results", str(source), "--output", tmp]), 0)
            self.assertEqual(json.loads((Path(tmp) / "report.json").read_text()), report)

    def test_nested_mismatch_cannot_be_laundered_by_outer_success(self):
        log = 'test wanted ... BOUNDARY_OBSERVATION {"status":"mismatch"}\nok\ntest result: ok. 1 passed;'
        self.assertEqual(classify(0, log, 'wanted'), 'mismatch')

    def test_missing_strict_results_are_reported_as_unexecuted(self):
        import json
        import tempfile
        from pathlib import Path
        from tools.boundary_contracts import main
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(main(["report", "--output", tmp]), 0)
            rows = json.loads((Path(tmp) / "report.json").read_text())["rows"]
            self.assertTrue(rows)
            self.assertTrue(all(row["status"] == "unexecuted" for row in rows))

    def test_fixture_setup_failure_is_infrastructure_not_mismatch(self):
        self.assertEqual(classify(101, 'test wanted ... BOUNDARY_INFRASTRUCTURE: permission fixture unavailable\nFAILED', 'wanted'), 'infrastructure-error')

    def test_report_rejects_duplicate_or_unknown_status_rows(self):
        import hashlib
        import json
        import tempfile
        from pathlib import Path
        from tools.boundary_contracts import REGISTRY, load_registry, main
        rows = [{"id": case["id"], "mode": case["mode"], "status": "unexecuted"} for case in load_registry()["cases"]]
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "captured.json"
            for invalid in [rows + [rows[0]], [rows[0] | {"status": "unknown"}] + rows[1:]]:
                source.write_text(json.dumps({"registry_sha256": hashlib.sha256(REGISTRY.read_bytes()).hexdigest(), "rows": invalid}))
                with self.assertRaises(SystemExit) as error:
                    main(["report", "--from-results", str(source), "--output", tmp])
                self.assertEqual(error.exception.code, 2)
                self.assertFalse((Path(tmp) / "report.json").exists())

    def test_strict_execution_removes_inherited_corpus_filters(self):
        import os
        import tempfile
        from pathlib import Path
        from unittest.mock import Mock, patch
        from tools.boundary_contracts import execute, load_registry
        case = load_registry()["cases"][0]
        def spawn(argv, **kwargs):
            self.assertNotIn("HOIMIN_BOUNDARY_CASE", kwargs["env"])
            self.assertNotIn("HOIMIN_SESSION_ORACLE_CASE", kwargs["env"])
            self.assertEqual(kwargs["env"]["PATH"], os.environ["PATH"])
            kwargs["stdout"].write(f'test {case["test"]} ... ok\ntest result: ok. 1 passed;')
            kwargs["stdout"].flush()
            return Mock(wait=Mock(return_value=0))
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {"HOIMIN_BOUNDARY_CASE": "one", "HOIMIN_SESSION_ORACLE_CASE": "one"}), patch("tools.boundary_contracts.subprocess.Popen", side_effect=spawn):
            self.assertEqual(execute(case, Path(tmp))["status"], "match")
