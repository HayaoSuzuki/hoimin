"""Replay Lean-owned expectations; record public observations without fixing Rust."""
import argparse
import hashlib
import json
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--bench', action='store_true')
    parser.add_argument('--full-run', action='store_true')
    args = parser.parse_args()
    binary = args.binary.resolve()
    python = ROOT / '.venv/bin/python'
    rows = []
    for line in (HERE / 'corpus.jsonl').read_text().splitlines():
        case = json.loads(line)
        with tempfile.TemporaryDirectory() as project:
            subject = Path(project) / 'subject.py'
            subject.write_text(case['source'])
            command = [str(binary), 'plan', '--root', project, '--file', 'subject.py',
                       '--allow-best-effort-memory', '--operators', 'type_list_sequence',
                       '--', 'true']
            row = {'id': case['id'], 'mode': case['mode'],
                   'expected_count': case['candidate_count'],
                   'expected_runtime_typing': case['runtime_typing']}
            try:
                result = subprocess.run(command, capture_output=True, text=True, timeout=10)
                if result.returncode:
                    raise RuntimeError(f'plan exit={result.returncode}: {result.stderr}')
                candidates = json.loads(result.stdout)['candidates']
                observed = []
                for raises in (False, True):
                    probe = ('import typing,json,sys\n'
                             'def hazard():\n' +
                             ('    raise KeyError()\n' if raises else '    pass\n') +
                             "ns={'hazard':hazard}\n"
                             "exec(compile(open(sys.argv[1]).read(),sys.argv[1],'exec'),ns)\n"
                             "print(json.dumps({'typing':ns['observed']==typing.Sequence[int],"
                             "'annotation':str(ns['observed'])}))\n")
                    runtime = subprocess.run([str(python), '-c', probe, str(subject)],
                                             capture_output=True, text=True, timeout=10)
                    if runtime.returncode:
                        raise RuntimeError(f'Python exit={runtime.returncode}: {runtime.stderr}')
                    observed.append(json.loads(runtime.stdout))
                row.update(candidates=candidates, runtime=observed)
                row['status'] = ('match' if len(candidates) == case['candidate_count'] and
                                 [r['typing'] for r in observed] == case['runtime_typing']
                                 else 'mismatch')
                if args.full_run and case['id'] == 'call_before_import':
                    if shutil.disk_usage(project).free <= 10 * 1024 ** 3:
                        raise RuntimeError('free space does not exceed 10 GiB')
                    runner = Path(project) / 'check.py'
                    runner.write_text("def hazard():\n    raise KeyError()\n"
                                      "ns={'hazard':hazard}\n"
                                      "exec(compile(open('subject.py').read(),'subject.py','exec'),ns)\n"
                                      "assert ns['observed'] == set[int]\n")
                    normal = subprocess.run([str(python), str(runner)], cwd=project,
                                            capture_output=True, text=True, timeout=10)
                    if normal.returncode:
                        raise RuntimeError(f'normal test failed: {normal.stderr}')
                    run_cmd = [str(binary), 'run', '--root', project, '--file', 'subject.py',
                               '--operators', 'type_list_sequence', '--allow-best-effort-memory',
                               '--format', 'json', '--', str(python), 'check.py']
                    run = subprocess.run(run_cmd, capture_output=True, text=True, timeout=20)
                    if run.returncode not in (0, 1):
                        raise RuntimeError(f'run exit={run.returncode}: {run.stderr}')
                    row['full_run'] = {'exit': run.returncode, 'report': json.loads(run.stdout)}
            except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
                row.update(status='infrastructure error', mode='infrastructure-error', error=str(error))
            rows.append(row)

    benchmarks = []
    if args.bench:
        with tempfile.TemporaryDirectory() as project:
            subject = Path(project) / 'subject.py'
            for depth in range(10, 21):
                source = 'value: Sequence[int]\n'
                for _ in range(depth):
                    source = 'try:\n    pass\nfinally:\n' + ''.join(
                        '    ' + line + '\n' for line in source.splitlines())
                source = 'from typing import Sequence\n' + source
                # Verify these are valid Python inputs, independently of Hoimin.
                compile(source, '<benchmark>', 'exec')
                subject.write_text(source)
                samples = []
                for _ in range(3):
                    started = time.monotonic()
                    run = subprocess.run([str(binary), 'plan', '--root', project,
                                          '--file', 'subject.py', '--allow-best-effort-memory',
                                          '--operators', 'type_list_sequence', '--', 'true'],
                                         capture_output=True, text=True, timeout=10)
                    samples.append(time.monotonic() - started)
                    if run.returncode or len(json.loads(run.stdout)['candidates']) != 1:
                        raise RuntimeError(f'benchmark failed at {depth}: {run.stderr}')
                benchmarks.append({'depth': depth, 'bytes': len(source.encode()),
                                   'seconds': samples, 'median': statistics.median(samples),
                                   'candidate_count': 1})
                print(benchmarks[-1], flush=True)
                if max(samples) > 2:
                    break
    output = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'python': subprocess.check_output([str(python), '--version'], text=True).strip(),
              'cases': rows, 'benchmarks': benchmarks}
    args.output.write_text(json.dumps(output, indent=2) + '\n')
    print({status: sum(row['status'] == status for row in rows)
           for status in ('match', 'mismatch', 'infrastructure error')})
    if any(row['status'] == 'infrastructure error' for row in rows):
        raise SystemExit(2)


if __name__ == '__main__':
    main()
