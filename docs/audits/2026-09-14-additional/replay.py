"""Compare Lean-owned candidate counts with plan and observe CPython annotations."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def observe(source):
    probe = "import sys,json\nns={}\nexec(compile(sys.stdin.read(),'<fixture>','exec'),ns)\n"
    probe += "try:\n print(json.dumps({'annotation':str(ns['record'].__annotations__['value'])}))\n"
    probe += "except Exception as e:\n print(json.dumps({'error':type(e).__name__, 'message':str(e)}))\n"
    result = subprocess.run([str(ROOT / '.venv/bin/python'), '-c', probe], input=source,
                            capture_output=True, text=True, timeout=10)
    if result.returncode:
        raise RuntimeError(result.stderr)
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
        with tempfile.TemporaryDirectory() as project:
            Path(project, 'subject.py').write_text(case['source'])
            result = subprocess.run([str(binary), 'plan', '--root', project, '--file', 'subject.py',
                '--operators', case['operator'], '--allow-best-effort-memory', '--', 'true'],
                capture_output=True, text=True, timeout=15)
            if result.returncode:
                raise RuntimeError(result.stderr)
            candidates = json.loads(result.stdout)['candidates']
            runtime = []
            for candidate in candidates:
                raw = case['source'].encode()
                start = candidate['span']['start']
                end = start + candidate['span']['length']
                assert raw[start:end].decode() == candidate['original']
                mutant = raw[:start] + candidate['replacement'].encode() + raw[end:]
                runtime.append(observe(mutant.decode()))
            baseline = observe(case['source'])
            assert 'annotation' in baseline, baseline
            rows.append({'id': case['id'], 'mode': 'strict',
                'expected_candidate_count': case['expected_candidate_count'],
                'actual_candidate_count': len(candidates),
                'status': 'match' if len(candidates) == case['expected_candidate_count'] else 'mismatch',
                'candidates': candidates, 'baseline': baseline, 'mutants': runtime})
    report = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'rows': rows}
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    for row in rows:
        print(row['id'], row['status'], row['baseline'], row['mutants'])


if __name__ == '__main__':
    main()
