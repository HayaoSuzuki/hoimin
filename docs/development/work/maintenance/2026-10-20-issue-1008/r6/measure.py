"""R6: exactly 512 alternating pairs per fixture; no filtering or optional stopping."""
import hashlib
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import time

from r6_statistics import upper_median_bound, upper_rank

ROOT = Path.cwd()
HERE = Path(__file__).resolve().parent
OUT = Path('/tmp/hoimin-perf-three-task3-r6')
OLD = Path('/tmp/hoimin-perf-three-task3')
WRAPPER = HERE.parent.parent / '2026-10-20-issue-1006/resource_wrapper.py'
PAIRS = 512
TOLERANCE_NS = 50_000
FIXTURES = [('tiny', 0), ('small', 128)]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_digest(path):
    return digest(path.read_bytes())


def write_json(name, value):
    (OUT / name).write_text(json.dumps(value, indent=2) + '\n')


# Verify fixtures/binaries once before observation; no fixture regeneration.
prior = json.loads((HERE.parent / 'measurement.json').read_text())
builds = json.loads((OUT / 'build-variants.json').read_text())
assert builds['source_restored']
environment = os.environ.copy()
env_hash = digest(json.dumps(environment, sort_keys=True).encode())
commands = {}
for label, _ in FIXTURES:
    root = OLD / f'fixture-{label}'
    assert file_digest(root / 'source.py') == prior['fixtures'][label]['sha256']
    for variant in ['base', 'lazy']:
        binary = OUT / f'hoimin-{variant}'
        assert file_digest(binary) == builds['variants'][variant]['binary_sha256']
        command = [str(binary), 'plan', '--root', str(root), '--file', 'source.py', '--operators',
                   'boolean_literal', '--max-candidates', '1', '--jobs', '1', '--max-mutants', '1',
                   '--allow-best-effort-memory', '--min-free-space', '1B', '--', '/usr/bin/true']
        commands[f'{label}-{variant}'] = {'argv': command, 'sha256': digest(json.dumps(command).encode())}
rank, coverage = upper_rank(PAIRS)
metadata = {
    'pairs_per_fixture': PAIRS, 'ordinary_child_count': 2048, 'child_timeout_seconds': 25,
    'batch_timeout_seconds': 600, 'tolerance_ns': TOLERANCE_NS,
    'method': '512 alternating base/lazy then lazy/base pairs per fixture; tiny followed by small. First sample retained, no warmup discard, no trimming, all outliers retained. Single batch.',
    'measurement': 'Identical prior resource_wrapper.py: perf_counter_ns immediately before subprocess.run through captured child completion. Wrapper startup excluded. Ordinary executable, no allocator instrumentation. Heap probe separately invokes actual public run_with_writers plan after runtime/caller setup.',
    'confidence': {'one_sided_probability': '19/20', 'rank_one_based': rank,
        'coverage_numerator': str(coverage.numerator), 'coverage_denominator': str(coverage.denominator),
        'coverage_decimal': float(coverage),
        'rule': 'Smallest k with sum(comb(512,j),j=0..k-1)/2**512 >= 19/20; upper bound is ordered paired difference D_(k). Independent representative pairs assumed; per-fixture confidence, not simultaneous familywise confidence.'},
    'heap_criterion': 'For each fixture and all three matched observations, requested whole-plan peak and retained deltas lazy-base must each be <=0.',
    'commands': commands, 'fixtures': {label: prior['fixtures'][label] for label, _ in FIXTURES},
    'environment_sha256': env_hash,
    'environment': {key: environment.get(key) for key in ['CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'CARGO_INCREMENTAL', 'RUST_TEST_THREADS', 'PREK_NO_CONCURRENCY', 'UV_PROJECT_ENVIRONMENT', 'UV_CACHE_DIR', 'PATH', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']},
    'hashes': {str(p): file_digest(p) for p in [Path(__file__), HERE / 'r6_statistics.py', WRAPPER, HERE.parent / 'probe.rs', OUT / 'hoimin-base', OUT / 'hoimin-lazy', OUT / 'probe-base', OUT / 'probe-lazy']},
}
# Exclusive creation prevents accidental repeats replacing this batch's evidence.
raw = (OUT / 'ordinary-samples.jsonl').open('x')
write_json('protocol.json', metadata)
start = time.monotonic()
deadline = start + 600


def bounded(command):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError('R6 total batch limit reached')
    process = subprocess.Popen(['python3', str(WRAPPER), *command], stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, env=environment, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=min(30, remaining))
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.communicate()
        raise
    if process.returncode != 0:
        raise RuntimeError((command, process.returncode, stdout, stderr))
    result = json.loads(stdout)
    if result['code'] != 0:
        raise RuntimeError((command, result))
    return result


rows = []
heap_rows = []
try:
    for label, _ in FIXTURES:
        for pair in range(PAIRS):
            order = ['base', 'lazy'] if pair % 2 == 0 else ['lazy', 'base']
            for variant in order:
                key = f'{label}-{variant}'
                result = bounded(commands[key]['argv'])
                stdout = result.pop('stdout')
                document = json.loads(stdout)
                assert len(document['candidates']) == 1
                projection = {name: document.get(name) for name in ['candidates', 'diagnostics', 'truncated']}
                row = {'fixture': label, 'pair': pair, 'variant': variant, 'command_key': key,
                       'elapsed_ns': result['elapsed_ns'], 'maxrss_bytes': result['maxrss_bytes'],
                       'code': result['code'], 'stderr': result['stderr'],
                       'stdout_sha256': digest(stdout.encode()),
                       'stable_output_sha256': digest(json.dumps(projection, sort_keys=True).encode()),
                       'candidate_id': document['candidates'][0]['id']}
                raw.write(json.dumps(row) + '\n')
                raw.flush()
                rows.append(row)
            if (pair + 1) % 64 == 0:
                print('PAIRS', label, pair + 1, 'ELAPSED_SECONDS', time.monotonic() - start, flush=True)
    for label, n in FIXTURES:
        for pair in range(3):
            for variant in (['base', 'lazy'] if pair % 2 == 0 else ['lazy', 'base']):
                command = [str(OUT / f'probe-{variant}'), str(OLD / f'fixture-{label}'), 'plan', str(n)]
                result = bounded(command)
                stdout = result.pop('stdout')
                measurement = json.loads(stdout)
                assert measurement['candidates'] == 1
                heap_rows.append({'fixture': label, 'pair': pair, 'variant': variant,
                                  'command': command, 'command_sha256': digest(json.dumps(command).encode()),
                                  'measurement': measurement, 'wrapper': result})
    summary = []
    for label, _ in FIXTURES:
        group = [row for row in rows if row['fixture'] == label]
        assert len({row['stable_output_sha256'] for row in group}) == 1
        differences = []
        for pair in range(PAIRS):
            samples = {row['variant']: row['elapsed_ns'] for row in group if row['pair'] == pair}
            differences.append(samples['lazy'] - samples['base'])
        upper = upper_median_bound(differences)
        heap_pairs = []
        for pair in range(3):
            samples = {row['variant']: row['measurement'] for row in heap_rows if row['fixture'] == label and row['pair'] == pair}
            assert samples['base']['digest'] == samples['lazy']['digest'] == group[0]['candidate_id']
            heap_pairs.append({metric: samples['lazy'][metric] - samples['base'][metric] for metric in ['requested_peak_delta', 'requested_live_delta']})
        summary.append({'fixture': label, 'paired_differences_ns': differences,
            'upper_bound_ns': upper, 'rank_one_based': rank, 'time_pass': upper is not None and upper <= TOLERANCE_NS,
            'positive': sum(value > 0 for value in differences), 'zero': differences.count(0),
            'negative': sum(value < 0 for value in differences),
            'paired_median_ns': statistics.median(differences), 'paired_mean_ns': statistics.mean(differences),
            'paired_min_ns': min(differences), 'paired_max_ns': max(differences),
            'absolute': {variant: {'median_ns': statistics.median(row['elapsed_ns'] for row in group if row['variant'] == variant),
                                  'mean_ns': statistics.mean(row['elapsed_ns'] for row in group if row['variant'] == variant)} for variant in ['base', 'lazy']},
            'heap_paired_differences': heap_pairs,
            'heap_pass': all(value <= 0 for pair in heap_pairs for value in pair.values())})
    elapsed = time.monotonic() - start
    assert elapsed <= 600
    for label, _ in FIXTURES:
        assert file_digest(OLD / f'fixture-{label}/source.py') == prior['fixtures'][label]['sha256']
    write_json('heap-samples.json', heap_rows)
    write_json('summary.json', {'batch_elapsed_seconds': elapsed, 'ordinary_samples': len(rows),
                               'heap_samples': len(heap_rows), 'summary': summary,
                               'pass': all(row['time_pass'] and row['heap_pass'] for row in summary)})
    print(json.dumps({row['fixture']: {key: value for key, value in row.items() if key != 'paired_differences_ns'} for row in summary}, indent=2), flush=True)
finally:
    raw.close()
if not all(row['time_pass'] and row['heap_pass'] for row in summary):
    raise SystemExit('R6 acceptance criterion not met; evidence retained, no additional batch authorized')
