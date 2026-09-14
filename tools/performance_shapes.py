"""Declared performance gates and bounded release CLI measurements."""
import argparse
import hashlib
import json
import platform
import re
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIMENSIONS = {'discovery', 'fingerprint', 'layout', 'ast', 'analysis-state', 'verify', 'output', 'workspace'}
SHAPES = {'discovery', 'fingerprint', 'long-line', 'many-lines', 'ast', 'imports', 'bindings', 'verify-top1', 'verify-topn', 'output', 'workspace'}
LIMITS = {name: (128 if name in {'ast', 'verify-top1', 'verify-topn', 'output', 'workspace'} else 65536) for name in SHAPES}
METRICS = {'operations', 'retained_heap', 'peak_heap', 'sampled_tree_rss', 'time'}


def validate_registry(registry):
    if registry.get('schema_version') != 1:
        raise ValueError('unsupported schema')
    ids = set()
    for row in registry['shapes']:
        if row['id'] in ids:
            raise ValueError('duplicate shape')
        ids.add(row['id'])
        sizes = row['sizes']
        if (len(sizes) != 3 or any(type(n) is not int for n in sizes)
                or not 1 <= sizes[0] or sizes != [sizes[0], 2*sizes[0], 4*sizes[0]]
                or sizes[-1] > LIMITS.get(row["fixture"], 0)):
            raise ValueError('sizes must be bounded N/2N/4N')
        if row['metric'] not in METRICS or row['status'] not in {'active', 'measurement-only', 'pending'}:
            raise ValueError('invalid metric/status')
        if row['fixture'] not in SHAPES or not row['growth_model']:
            raise ValueError('missing fixture/model')
    if {r['dimension'] for r in registry['shapes']} != DIMENSIONS:
        raise ValueError('all eight dimensions required')
    ids = set()
    for gate in registry['gates']:
        if gate['id'] in ids:
            raise ValueError('duplicate gate')
        ids.add(gate['id'])
        if gate.get('metric') not in METRICS:
            raise ValueError('invalid gate metric')
        if gate['status'] == 'pending':
            if not gate.get('issue', '').startswith('https://github.com/tokyogas-tech/hoimin/issues/'):
                raise ValueError('pending dependency required')
        elif gate['status'] == 'active':
            args = gate['args']
            if not args or args[0] != 'test' or '--exact' not in args or '--ignored' in args:
                raise ValueError('exact non-ignored Rust test required')
        else:
            raise ValueError('unknown gate status')
    return registry


def load_registry(path):
    return validate_registry(json.loads(path.read_text()))


def executed_tests(output):
    results = re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;', output)
    if (not results or any(s != 'ok' or int(f) for s, _, f, _ in results)
            or sum(int(p) for _, p, _, _ in results) == 0):
        raise ValueError('gate did not execute a passing test')
    return sum(int(p) for _, p, _, _ in results)


def make_fixture(name, size, root):
    if name not in SHAPES or type(size) is not int or not 1 <= size <= LIMITS[name]:
        raise ValueError('unknown shape or out-of-bounds size')
    root.mkdir(parents=True)
    source, operator, count, truncated, mode = 'x = 1 + 2\n', 'binary_add_sub', 1, False, 'plan'
    options = []
    if name == 'discovery':
        (root/'unrelated').mkdir()
        for i in range(size):
            (root/'unrelated'/f'{i}.txt').write_text('')
    elif name == 'fingerprint':
        (root/'config.toml').write_text('value = 1\n')
        options = ['--fingerprint-include', '*.toml'] * size
    elif name in {'long-line', 'many-lines'}:
        source = 'x = [' + (' ' if name == 'long-line' else '\n').join(['True,'] * size) + ']\n'
        operator, truncated = 'boolean_literal', size > 1
    elif name == 'ast':
        source = 'x = ' + '[' * size + '1 + 2' + ']' * size + '\n'
    elif name == 'imports':
        source = ''.join(f'from typing import Optional as A{i}\n' for i in range(size))
        source += ''.join(f'x{i}: A{i}[int]\n' for i in range(size))
        operator, count = 'boolean_literal', 0
    elif name == 'bindings':
        source, operator, count = 'list = tuple\nx = list()\n' * size, 'collection_list_tuple', 0
    elif name in {'verify-top1', 'verify-topn', 'output'}:
        source, mode = 'x = 1 + 2\n' * size, 'run' if name == 'output' else 'verify'
        count = 1 if name == 'verify-top1' else size
    elif name == 'workspace':
        mode = 'run'
        (root/'data').mkdir()
        for i in range(size):
            (root/'data'/f'{i}.txt').write_bytes(b'x' * 65536)
    (root/'case.py').write_text(source)
    return dict(name=name, size=size, mode=mode, operator=operator, expected_candidates=count,
                truncated=truncated, options=options, source_bytes=len(source.encode()),
                input_files=sum(1 for p in root.rglob('*') if p.is_file()),
                input_bytes=sum(p.stat().st_size for p in root.rglob('*') if p.is_file()))


class SemanticMismatch(ValueError):
    """The CLI completed but its observable result violates the fixture."""


def validate_output(document, fixture):
    if fixture['mode'] == 'plan':
        records = document.get('candidates')
        if document.get('truncated') is not fixture['truncated']:
            raise SemanticMismatch('unexpected truncation')
    else:
        records = document.get('mutants')
        if document.get('summary', {}).get('complete') is not True:
            raise SemanticMismatch('incomplete execution')
    if not isinstance(records, list) or len(records) != fixture['expected_candidates']:
        raise SemanticMismatch('unexpected candidate count')
    return dict(candidates=len(records), truncated=fixture['truncated'])


def observed_rss(stats):
    if stats.get('reason') != 'child_exit':
        raise ValueError(f"resource guard failed: {stats.get('reason')}")
    return stats['peak_rss_kib'] * 1024 if stats['peak_rss_kib'] else None


def measure_once(binary, fixture, root, artifact, sample_ms):
    common = ['--root', str(root), '--file', 'case.py', '--operators', fixture['operator'],
              '--jobs', '1', '--max-mutants', '128', '--max-candidates',
              '1' if fixture['mode'] == 'plan' else '128', '--allow-best-effort-memory',
              '--max-workspace-size', '8GiB', '--min-free-space', '10GiB'] + fixture['options']
    test = ['--', sys.executable, '-c', 'pass']
    command = [str(binary), 'plan', *common, *test]
    if fixture['mode'] == 'verify':
        manifest = artifact/'plan.json'
        with manifest.open('wb') as out, (artifact/'plan.stderr').open('wb') as err:
            prepared = subprocess.run(command, stdout=out, stderr=err, timeout=30, check=False)
        if prepared.returncode != 0:
            raise ValueError('verify fixture plan failed')
        validate_output(json.loads(manifest.read_text()), dict(mode='plan', expected_candidates=fixture['size'], truncated=False))
        command = [str(binary), 'verify', str(manifest), '--top', str(fixture['expected_candidates']), '--format', 'json']
    elif fixture['mode'] == 'run':
        command = [str(binary), 'run', *common, '--format', 'json', *test]
    stats_path = artifact/'resource.json'
    guard = [sys.executable, str(ROOT/'formal/HoiminOracle/tools/lean_resource_guard.py'),
             '--timeout-seconds', '30', '--rss-limit-mib', '2048', '--sample-ms', str(sample_ms),
             '--stats', str(stats_path), '--', *command]
    with (artifact/'stdout.json').open('wb') as out, (artifact/'stderr.txt').open('wb') as err:
        completed = subprocess.run(guard, stdout=out, stderr=err, timeout=40, check=False)
    stats = json.loads(stats_path.read_text())
    rss = observed_rss(stats)
    expected_exit = 4 if fixture['truncated'] else (0 if fixture['mode'] == 'plan' else 1)
    if completed.returncode != expected_exit:
        raise ValueError(f'CLI exit {completed.returncode}; expected {expected_exit}')
    observation = validate_output(json.loads((artifact/'stdout.json').read_text()), fixture)
    return dict(argv=command, elapsed_ms=stats['elapsed_ms'], sampled_tree_rss_bytes=rss,
                sample_ms=sample_ms, exit_code=completed.returncode, **observation)


def run_gate(registry, artifact):
    outcomes = []
    for gate in registry['gates']:
        if gate['status'] == 'pending':
            outcomes.append(dict(id=gate['id'], status='pending', issue=gate['issue']))
            continue
        log = artifact/f"{gate['id']}.log"
        with log.open('w') as output:
            completed = subprocess.run(['cargo', *gate['args']], cwd=ROOT, stdout=output,
                                       stderr=subprocess.STDOUT, text=True, timeout=300, check=False)
        output = log.read_text()
        if completed.returncode:
            print(output, file=sys.stderr)
            raise ValueError(f"gate failed: {gate['id']}")
        outcomes.append(dict(id=gate['id'], status='passed', tests=executed_tests(output)))
    return outcomes


def summarize_comparisons(medians):
    """Compare candidate medians with their matching baseline observations."""
    grouped = {}
    for item in medians:
        grouped.setdefault((item['shape'], item['size']), {})[item['label']] = item
    comparisons = []
    for (shape, size), labels in grouped.items():
        if set(labels) != {'baseline', 'candidate'}:
            raise ValueError(f'missing baseline or candidate median: {shape}/{size}')
        baseline, candidate = labels['baseline'], labels['candidate']
        elapsed_base, elapsed_candidate = baseline['elapsed_ms'], candidate['elapsed_ms']
        rss_base = baseline['sampled_tree_rss_bytes']
        rss_candidate = candidate['sampled_tree_rss_bytes']
        comparisons.append(dict(
            shape=shape,
            size=size,
            elapsed_ms_delta=elapsed_candidate-elapsed_base,
            elapsed_ratio=(elapsed_candidate/elapsed_base if elapsed_base else None),
            sampled_tree_rss_bytes_delta=(rss_candidate-rss_base
                                           if rss_base is not None and rss_candidate is not None
                                           else None),
            sampled_tree_rss_ratio=(rss_candidate/rss_base
                                    if rss_base not in {None, 0} and rss_candidate is not None
                                    else None),
        ))
    return comparisons


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['check', 'gate', 'measure'])
    parser.add_argument('--registry', type=Path, default=ROOT/'docs/performance/shapes.json')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--candidate', type=Path)
    parser.add_argument('--shape', action='append', choices=sorted(SHAPES))
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--sample-ms', type=int, default=25)
    args = parser.parse_args()
    if args.mode == 'check':
        registry = load_registry(args.registry)
        print(f"validated {len(registry['shapes'])} shapes")
        return 0
    if args.output is None:
        parser.error('--output is required')
    if not 3 <= args.repeats <= 10 or not 10 <= args.sample_ms <= 1000:
        parser.error('repeats must be 3..10 and sample-ms 10..1000')
    args.output.mkdir(parents=True, exist_ok=False)
    result = dict(schema_version=1, environment={}, runs=[])
    try:
        registry = load_registry(args.registry)
        result['environment'] = dict(
            platform=platform.platform(), machine=platform.machine(), cpu=platform.processor(),
            python=sys.version,
            rust=subprocess.run(['rustc', '--version'], capture_output=True, text=True,
                                check=True).stdout.strip(),
        )
        result['registry_sha256'] = hashlib.sha256(args.registry.read_bytes()).hexdigest()
        if args.mode == 'gate':
            result['gates'] = run_gate(registry, args.output)
        else:
            if args.baseline is None or args.candidate is None:
                raise ValueError('--baseline and --candidate required')
            result['binaries'] = {name: dict(path=str(path.resolve(strict=True)),
                sha256=hashlib.sha256(path.read_bytes()).hexdigest())
                for name, path in [('baseline', args.baseline), ('candidate', args.candidate)]}
            for row in registry['shapes']:
                if args.shape and row['fixture'] not in args.shape:
                    continue
                for size in row['sizes']:
                    for label, info in result['binaries'].items():
                        elapsed = []
                        rss_samples = []
                        for repeat in range(args.repeats):
                            artifact = args.output/f"{row['id']}-{size}-{label}-{repeat}"
                            artifact.mkdir()
                            with tempfile.TemporaryDirectory(prefix='hoimin-perf-') as temporary:
                                root = Path(temporary)/'project'
                                fixture = make_fixture(row['fixture'], size, root)
                                run = measure_once(Path(info['path']), fixture, root, artifact, args.sample_ms)
                            result['runs'].append(dict(shape=row['id'], label=label, repeat=repeat, fixture=fixture, **run))
                            elapsed.append(run['elapsed_ms'])
                            rss_samples.append(run['sampled_tree_rss_bytes'])
                        result.setdefault('medians', []).append(dict(shape=row['id'], size=size, label=label,
                            elapsed_ms=statistics.median(elapsed),
                            sampled_tree_rss_bytes=(statistics.median(rss_samples)
                                                    if all(value is not None for value in rss_samples)
                                                    else None)))
            result['comparisons'] = summarize_comparisons(result.get('medians', []))
        result['status'] = 'passed'
    except SemanticMismatch as error:
        result['status'], result['error'] = 'mismatch', str(error)
    except (KeyError, OSError, ValueError, subprocess.SubprocessError) as error:
        result['status'], result['error'] = 'infrastructure-error', str(error)
    finally:
        (args.output/'result.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(dict(status=result['status'], artifact=str(args.output/'result.json'))))
    return 0 if result['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
