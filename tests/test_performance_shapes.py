import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('performance_shapes', ROOT / 'tools/performance_shapes.py')
shapes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shapes)


class PerformanceShapesTests(unittest.TestCase):
    def test_registry_covers_all_dimensions_and_rejects_duplicates(self):
        registry = shapes.load_registry(ROOT / 'docs/performance/shapes.json')
        self.assertEqual({row['dimension'] for row in registry['shapes']}, {
            'discovery', 'fingerprint', 'layout', 'ast', 'analysis-state', 'verify', 'output', 'workspace'})
        duplicate = json.loads(json.dumps(registry))
        duplicate['shapes'].append(duplicate['shapes'][0])
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            shapes.validate_registry(duplicate)

    def test_registry_rejects_invalid_growth_and_untraceable_pending_gate(self):
        for field, value in [('sizes', [2, 2, 4]), ('metric', 'memory'), ('status', 'passing')]:
            registry = shapes.load_registry(ROOT / 'docs/performance/shapes.json')
            registry['shapes'][0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                shapes.validate_registry(registry)
        registry = shapes.load_registry(ROOT / 'docs/performance/shapes.json')
        registry['gates'].append({
            'id': 'untraceable', 'status': 'pending', 'metric': 'operations', 'args': []})
        with self.assertRaises(ValueError):
            shapes.validate_registry(registry)
        registry = shapes.load_registry(ROOT / 'docs/performance/shapes.json')
        registry['gates'][0]['metric'] = 'memory'
        with self.assertRaisesRegex(ValueError, 'gate metric'):
            shapes.validate_registry(registry)

    def test_test_gate_rejects_empty_ignored_and_failed_runs(self):
        for output in ['test result: ok. 0 passed; 0 failed; 0 ignored;',
                       'test result: ok. 0 passed; 0 failed; 1 ignored;',
                       'test result: FAILED. 1 passed; 1 failed; 0 ignored;']:
            with self.subTest(output=output), self.assertRaises(ValueError):
                shapes.executed_tests(output)
        self.assertEqual(shapes.executed_tests('test result: ok. 1 passed; 0 failed; 0 ignored;'), 1)

    def test_fixtures_have_independent_expected_outputs_and_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            for name, expected in [('discovery', 1), ('fingerprint', 1), ('long-line', 1),
                                   ('many-lines', 1), ('ast', 1), ('imports', 0), ('bindings', 0),
                                   ('verify-top1', 1), ('verify-topn', 4), ('output', 4), ('workspace', 1)]:
                root = Path(directory) / name
                fixture = shapes.make_fixture(name, 4, root)
                self.assertEqual(fixture['expected_candidates'], expected, name)
                self.assertTrue((root / 'case.py').is_file())
                compile((root / 'case.py').read_text(), 'case.py', 'exec')
            self.assertEqual((Path(directory)/'long-line/case.py').stat().st_size,
                             (Path(directory)/'many-lines/case.py').stat().st_size)
        self.assertFalse(Path(directory).exists())

    def test_minimum_size_is_a_valid_fixture_for_every_shape(self):
        with tempfile.TemporaryDirectory() as directory:
            for name in sorted(shapes.SHAPES):
                with self.subTest(shape=name):
                    root = Path(directory)/name
                    fixture = shapes.make_fixture(name, 1, root)
                    self.assertEqual(fixture['size'], 1)
                    self.assertFalse(fixture['truncated'])
                    compile((root/'case.py').read_text(), 'case.py', 'exec')

    def test_output_validation_does_not_accept_empty_candidates(self):
        fixture = {'mode': 'plan', 'expected_candidates': 1, 'truncated': True}
        with self.assertRaises(ValueError):
            shapes.validate_output({'candidates': [], 'truncated': True}, fixture)
        self.assertEqual(shapes.validate_output({'candidates': [{'id': 'm1'}], 'truncated': True}, fixture)['candidates'], 1)
        with self.assertRaises(ValueError):
            shapes.validate_output({'candidates': [{'id': 'm1'}], 'truncated': False}, fixture)

    def test_size_bounds_and_unknown_shapes_fail_before_writing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)/'project'
            for size in [0, -1, 129]:
                with self.assertRaises(ValueError):
                    shapes.make_fixture('ast', size, root)
            with self.assertRaises(ValueError):
                shapes.make_fixture('unknown', 1, root)
            self.assertFalse(root.exists())

    def test_unobserved_rss_is_null_and_monitor_failure_is_not_success(self):
        self.assertIsNone(shapes.observed_rss({'reason': 'child_exit', 'peak_rss_kib': 0}))
        self.assertEqual(shapes.observed_rss({'reason': 'child_exit', 'peak_rss_kib': 12}), 12288)
        for reason in ['timeout', 'rss_limit', 'monitor_error']:
            with self.assertRaises(ValueError):
                shapes.observed_rss({'reason': reason, 'peak_rss_kib': 12})

    def test_comparisons_include_elapsed_and_nullable_rss_medians(self):
        medians = [
            {'shape': 'ast', 'size': 16, 'label': 'baseline', 'elapsed_ms': 10,
             'sampled_tree_rss_bytes': 100},
            {'shape': 'ast', 'size': 16, 'label': 'candidate', 'elapsed_ms': 8,
             'sampled_tree_rss_bytes': 90},
            {'shape': 'output', 'size': 4, 'label': 'baseline', 'elapsed_ms': 0,
             'sampled_tree_rss_bytes': None},
            {'shape': 'output', 'size': 4, 'label': 'candidate', 'elapsed_ms': 1,
             'sampled_tree_rss_bytes': 80},
        ]
        ast, output = shapes.summarize_comparisons(medians)
        self.assertEqual(ast['elapsed_ms_delta'], -2)
        self.assertEqual(ast['elapsed_ratio'], .8)
        self.assertEqual(ast['sampled_tree_rss_bytes_delta'], -10)
        self.assertEqual(ast['sampled_tree_rss_ratio'], .9)
        self.assertEqual(output['elapsed_ms_delta'], 1)
        self.assertIsNone(output['elapsed_ratio'])
        self.assertIsNone(output['sampled_tree_rss_bytes_delta'])
        self.assertIsNone(output['sampled_tree_rss_ratio'])

    def test_environment_probe_failure_writes_infrastructure_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'artifacts'
            registry = json.loads((ROOT/'docs/performance/shapes.json').read_text())
            for gate in registry['gates']:
                gate.setdefault('metric', 'operations')
            registry_path = Path(directory)/'registry.json'
            registry_path.write_text(json.dumps(registry))
            completed = subprocess.run(
                [sys.executable, str(ROOT/'tools/performance_shapes.py'), 'measure',
                 '--output', str(output), '--registry', str(registry_path),
                 '--baseline', 'missing-baseline',
                 '--candidate', 'missing-candidate'],
                cwd=ROOT, env={**os.environ, 'PATH': ''}, capture_output=True, text=True,
                timeout=10, check=False,
            )
            self.assertEqual(completed.returncode, 1)
            result = json.loads((output/'result.json').read_text())
            self.assertEqual(result['status'], 'infrastructure-error')
            self.assertEqual(result['environment'], {})
            self.assertIn('rustc', result['error'])

    def test_large_flat_inputs_do_not_raise_the_ast_depth_limit(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = shapes.make_fixture('long-line', 1000, Path(directory)/'flat')
            self.assertEqual(fixture['size'], 1000)
            with self.assertRaises(ValueError):
                shapes.make_fixture('ast', 129, Path(directory)/'deep')

    def test_top1_checks_available_manifest_size_before_measurement(self):
        if os.name != 'posix':
            self.skipTest('executable fixture uses a POSIX shebang')
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            fixture = shapes.make_fixture('verify-top1', 4, parent/'project')
            artifact = parent/'artifact'
            artifact.mkdir()
            binary = parent/'fake-hoimin'
            binary.write_text(f'#!{sys.executable}\nimport json\nprint(json.dumps({{"candidates": [{{"id": "m1"}}], "truncated": False}}))\n')
            binary.chmod(0o755)
            with self.assertRaises(shapes.SemanticMismatch):
                shapes.measure_once(binary, fixture, parent/'project', artifact, 25)
            self.assertFalse((artifact/'resource.json').exists())

    def test_timed_out_gate_keeps_partial_process_log(self):
        from unittest.mock import patch
        def timeout(command, **kwargs):
            output = kwargs['stdout']
            if hasattr(output, 'write'):
                output.write('partial compiler diagnostic\n')
                output.flush()
            raise subprocess.TimeoutExpired(command, 300, output='partial compiler diagnostic\n')
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory)
            registry = {'gates': [{'id': 'blocked', 'status': 'active', 'args': ['test']}]}
            with patch.object(shapes.subprocess, 'run', side_effect=timeout):
                with self.assertRaises(subprocess.TimeoutExpired):
                    shapes.run_gate(registry, artifact)
            self.assertEqual((artifact/'blocked.log').read_text(), 'partial compiler diagnostic\n')
