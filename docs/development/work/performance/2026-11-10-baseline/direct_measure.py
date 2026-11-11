"""One-off baseline; run from the repository root after the guarded lane finishes."""
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import re
import signal
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path.cwd()
sys.path.insert(0, str(ROOT / 'tools'))
import performance_shapes as shapes

OUT = Path(sys.argv[1])
OUT.mkdir(exist_ok=False, parents=True)
BINARY = ROOT / 'target/release/hoimin'
ENV = dict(os.environ)
ENV.pop('RUST_LOG', None)
REPEATS = 9
result = {
    'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
    'binary_sha256': hashlib.sha256(BINARY.read_bytes()).hexdigest(),
    'platform': platform.platform(), 'python': sys.version,
    'method': 'One discarded warmup per case; nine rounds with seeded shuffled case order. perf_counter_ns around /usr/bin/time -l and CLI; stdout and stderr to files. No ps monitor. Wall time includes process startup, serialization, file output and time wrapper. Darwin maximum RSS is bytes for the CLI process, not sampled process-tree RSS. RUST_LOG unset. Inputs prepared outside timing; warm filesystem cache; no cache flushing.',
    'seed': 20261110, 'runs': [], 'cases': [],
}
with tempfile.TemporaryDirectory(prefix='hoimin-direct-') as temporary:
    cases = []
    for row in shapes.load_registry(ROOT / 'docs/development/performance/shapes.json')['shapes']:
        for size in row['sizes']:
            root = Path(temporary) / f"{row['id']}-{size}"
            fixture = shapes.make_fixture(row['fixture'], size, root)
            if fixture['mode'] != 'plan':
                continue
            argv = [str(BINARY), 'plan', '--root', str(root), *fixture['selectors'],
                    '--operators', fixture['operator'], '--jobs', '1',
                    '--max-candidates', str(fixture['candidate_limit']),
                    '--allow-best-effort-memory', *fixture['options'], '--', sys.executable, '-c', 'pass']
            cases.append({'id': root.name, 'argv': argv, 'fixture': fixture})
    for path in ['tools/performance_shapes.py', 'tests/test_performance_shapes.py']:
        for limit in [1, 10000]:
            cases.append({'id': f"real-{Path(path).stem}-{limit}",
                'argv': [str(BINARY), 'plan', '--root', str(ROOT), '--file', path,
                         '--max-candidates', str(limit), '--allow-best-effort-memory', '--', sys.executable, '-c', 'pass'],
                'source': path, 'source_bytes': (ROOT / path).stat().st_size,
                'source_sha256': hashlib.sha256((ROOT / path).read_bytes()).hexdigest()})
    cases.append({'id': 'version-control', 'argv': [str(BINARY), '--version']})

    def observe(case, repeat):
        name = f"{case['id']}-{repeat}"
        stdout = OUT / (name + '.stdout')
        stderr = OUT / (name + '.stderr')
        with stdout.open('wb') as out, stderr.open('wb') as err:
            started = time.perf_counter_ns()
            process = subprocess.Popen(['/usr/bin/time', '-l', *case['argv']],
                stdout=out, stderr=err, env=ENV, start_new_session=True)
            def expire(signum, frame):
                os.killpg(process.pid, signal.SIGKILL)
                raise TimeoutError(case['id'])
            previous = signal.signal(signal.SIGALRM, expire)
            signal.alarm(30)
            try:
                process.wait()
            finally:
                signal.alarm(0)
                signal.signal(signal.SIGALRM, previous)
            elapsed = (time.perf_counter_ns() - started) / 1e6
        text = stderr.read_text()
        rss = re.search(r'(\d+)\s+maximum resident set size', text)
        assert rss, text
        observation = {'case': case['id'], 'repeat': repeat,
            'elapsed_ms': elapsed, 'max_rss_bytes': int(rss[1]),
            'exit_code': process.returncode, 'output_bytes': stdout.stat().st_size}
        for field, label in [('instructions', 'instructions retired'), ('cycles', 'cycles elapsed')]:
            match = re.search(r'(\d+)\s+' + label, text)
            observation[field] = int(match[1]) if match else None
        if case['id'] != 'version-control':
            document = json.loads(stdout.read_text())
            if 'fixture' in case:
                expected = case['fixture']
                validated = shapes.validate_output(document, expected)
                assert process.returncode == (4 if expected['truncated'] else 0)
                observation.update(validated)
            else:
                assert process.returncode in (0, 4), text
                observation.update(candidates=len(document['candidates']),
                    truncated=document['truncated'])
                assert process.returncode == (4 if document['truncated'] else 0)
        else:
            assert process.returncode == 0
        return observation

    for case in cases:
        observe(case, 'warmup')
    rng = random.Random(20261110)
    for repeat in range(REPEATS):
        order = list(cases)
        rng.shuffle(order)
        for case in order:
            result['runs'].append(observe(case, repeat))
    for case in cases:
        runs = [r for r in result['runs'] if r['case'] == case['id']]
        times = [r['elapsed_ms'] for r in runs]
        counts = {(r.get('candidates'), r.get('truncated')) for r in runs}
        assert len(counts) == 1, (case['id'], counts)
        result['cases'].append(dict(case, median_ms=statistics.median(times),
            min_ms=min(times), max_ms=max(times),
            median_instructions=statistics.median(r['instructions'] for r in runs) if all(r['instructions'] is not None for r in runs) else None,
            median_max_rss_bytes=statistics.median(r['max_rss_bytes'] for r in runs),
            candidates=runs[0].get('candidates'), truncated=runs[0].get('truncated')))
result['status'] = 'passed'
(OUT / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'status': 'passed', 'cases': len(cases), 'runs': len(result['runs']), 'output': str(OUT)}))
