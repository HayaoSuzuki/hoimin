"""Compare Lean candidate pairs with plan, then evaluate original and mutants."""
import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--full-run', action='store_true')
    args = parser.parse_args()
    binary = args.binary.resolve()
    python = ROOT / '.venv/bin/python'
    rows = []
    for line in (HERE / 'corpus.jsonl').read_text().splitlines():
        case = json.loads(line)
        row = {'id': case['id'], 'mode': case['mode'], 'expected_pairs': case['pairs']}
        with tempfile.TemporaryDirectory() as project:
            subject = Path(project) / 'subject.py'
            subject.write_text(case['source'])
            try:
                command = [str(binary), 'plan', '--root', project, '--file', 'subject.py',
                           '--allow-best-effort-memory', '--operators', case['operator'], '--', 'true']
                result = subprocess.run(command, capture_output=True, text=True, timeout=10)
                if result.returncode:
                    raise RuntimeError(f'plan exit {result.returncode}: {result.stderr}')
                candidates = json.loads(result.stdout)['candidates']
                row['candidates'] = candidates
                row['observed_pairs'] = sorted([c['original'], c['replacement']] for c in candidates)
                row['status'] = 'match' if row['observed_pairs'] == sorted(case['pairs']) else 'mismatch'
                row['evaluations'] = []
                for candidate in [None] + candidates:
                    source = case['source'].encode()
                    if candidate:
                        a = candidate['span']['start']
                        end = a + candidate['span']['length']
                        if source[a:end].decode() != candidate['original']:
                            raise RuntimeError('candidate span/original mismatch')
                        source = source[:a] + candidate['replacement'].encode() + source[end:]
                    probe = ("import json,sys\nns={}\ntry:\n"
                             "    exec(compile(sys.argv[1],'<fixture>','exec'),ns)\n"
                             "    out={'ok':True,'value':str(ns['observed'])}\n"
                             "except Exception as error:\n"
                             "    out={'ok':False,'error':type(error).__name__,'message':str(error)}\n"
                             "print(json.dumps(out))\n")
                    runtime = subprocess.run([str(python), '-c', probe, source.decode()],
                                             capture_output=True, text=True, timeout=10)
                    if runtime.returncode:
                        raise RuntimeError(f'Python process exit {runtime.returncode}: {runtime.stderr}')
                    observation = json.loads(runtime.stdout)
                    if candidate is None and not observation['ok']:
                        raise RuntimeError(f'original fixture invalid: {observation}')
                    row['evaluations'].append({'candidate_id': candidate['id'] if candidate else None,
                                               **observation})
                if args.full_run and case['id'] in ('unpack_target',):
                    if shutil.disk_usage(project).free <= 10 * 1024 ** 3:
                        raise RuntimeError('free space does not exceed 10 GiB')
                    runner = Path(project) / 'check.py'
                    runner.write_text('import subject\n' + (
                        "assert subject.observed == ['custom']\n"))
                    baseline = subprocess.run([str(python), str(runner)], cwd=project,
                                              capture_output=True, text=True, timeout=10)
                    if baseline.returncode:
                        raise RuntimeError(f'normal test failed: {baseline.stderr}')
                    result = subprocess.run([
                        str(binary), 'run', '--root', project, '--file', 'subject.py',
                        '--allow-best-effort-memory', '--operators', case['operator'],
                        '--format', 'json', '--', str(python), 'check.py'],
                        capture_output=True, text=True, timeout=20)
                    if result.returncode not in (0, 1):
                        raise RuntimeError(f'run exit {result.returncode}: {result.stderr}')
                    row['full_run'] = {'exit': result.returncode, 'report': json.loads(result.stdout)}
            except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
                row.update(mode='infrastructure-error', status='infrastructure error', error=str(error))
            rows.append(row)
    args.output.write_text(json.dumps({
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'python': subprocess.check_output([str(python), '--version'], text=True).strip(),
        'cases': rows}, indent=2) + '\n')
    print({status: sum(row['status'] == status for row in rows)
           for status in ('match', 'mismatch', 'infrastructure error')})
    if any(row['status'] == 'infrastructure error' for row in rows):
        raise SystemExit(2)


if __name__ == '__main__':
    main()
