"""Compare Lean's proposed eligibility contract with the public plan command."""
import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PYTHON = ROOT / '.venv/bin/python'


def evaluate(source):
    probe = ("import json,sys\nns={}\ntry:\n"
             " exec(compile(sys.argv[1],'<fixture>','exec'),ns)\n"
             " out={'ok':True,'value':str(ns['observed'])}\n"
             "except Exception as error:\n"
             " out={'ok':False,'error':type(error).__name__,'message':str(error)}\n"
             "print(json.dumps(out))\n")
    result = subprocess.run([str(PYTHON), '-c', probe, source],
                            capture_output=True, text=True, timeout=10)
    if result.returncode:
        raise RuntimeError(f'Python process exit {result.returncode}: {result.stderr}')
    return json.loads(result.stdout)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    rows = []
    for line in (HERE / 'corpus.jsonl').read_text().splitlines():
        case = json.loads(line)
        row = {'id': case['id'], 'mode': case['mode'], 'expected_pairs': case['pairs']}
        with tempfile.TemporaryDirectory() as project:
            try:
                for path in (ROOT, Path(project)):
                    if shutil.disk_usage(path).free <= 10 * 1024**3:
                        raise RuntimeError(f'insufficient free space: {path}')
                baseline = evaluate(case['source'])
                if not baseline['ok']:
                    raise RuntimeError(f'invalid original fixture: {baseline}')
                row['original_evaluation'] = baseline
                Path(project, 'subject.py').write_text(case['source'])
                result = subprocess.run([
                    str(binary), 'plan', '--root', project, '--file', 'subject.py',
                    '--operators', case['operator'], '--allow-best-effort-memory',
                    '--jobs', '1', '--max-workspace-size', '8GiB', '--min-free-space', '10GiB',
                    '--', 'true'], capture_output=True, text=True, timeout=10)
                if result.returncode:
                    raise RuntimeError(f'plan exit {result.returncode}: {result.stderr}')
                candidates = json.loads(result.stdout)['candidates']
                row['candidates'] = candidates
                observed = sorted([c['original'], c['replacement']] for c in candidates)
                row['observed_pairs'] = observed
                row['status'] = 'match' if observed == sorted(case['pairs']) else 'mismatch'
                row['candidate_evaluations'] = []
                for candidate in candidates:
                    source = case['source'].encode()
                    start = candidate['span']['start']
                    end = start + candidate['span']['length']
                    if source[start:end].decode() != candidate['original']:
                        raise RuntimeError('span/original disagreement')
                    mutant = source[:start] + candidate['replacement'].encode() + source[end:]
                    row['candidate_evaluations'].append(evaluate(mutant.decode()))
                # Each fixture has exactly one eligible lookup. Its last spelling is
                # the use site, after any import/declaration. Expectations come from Lean.
                row['expected_evaluations'] = []
                for original, replacement in case['pairs']:
                    start = case['source'].rfind(original)
                    if start < 0:
                        raise RuntimeError('expected lookup not found in fixture')
                    mutant = (case['source'][:start] + replacement
                              + case['source'][start + len(original):])
                    observation = evaluate(mutant)
                    if not observation['ok']:
                        raise RuntimeError(f'invalid expected replacement: {observation}')
                    row['expected_evaluations'].append(observation)
            except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
                row.update(mode='infrastructure-error', status='infrastructure error', error=str(error))
            rows.append(row)
    summary = {status: sum(row['status'] == status for row in rows)
               for status in ('match', 'mismatch', 'infrastructure error')}
    args.output.write_text(json.dumps({
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'python': subprocess.check_output([str(PYTHON), '--version'], text=True).strip(),
        'contract': 'proposed precision improvement; conservative omission is currently permitted',
        'summary': summary, 'cases': rows}, indent=2) + '\n')
    print(summary)
    if summary['infrastructure error']:
        raise SystemExit(2)


if __name__ == '__main__':
    main()
