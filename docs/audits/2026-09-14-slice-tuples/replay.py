"""Replay Lean-owned slice shapes through plan and CPython compilation."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def compile_source(source):
    probe = "import sys,json\ntry:\n compile(sys.stdin.read(),'<fixture>','exec')\n print(json.dumps({'valid':True}))\nexcept SyntaxError as e:\n print(json.dumps({'valid':False,'error':str(e)}))\n"
    r = subprocess.run([str(ROOT / '.venv/bin/python'), '-c', probe], input=source,
                       text=True, capture_output=True, timeout=10)
    if r.returncode:
        raise RuntimeError(r.stderr)
    return json.loads(r.stdout)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    rows = []
    for line in (HERE / 'corpus.jsonl').read_text().splitlines():
        case = json.loads(line)
        assert compile_source(case['source'])['valid']
        with tempfile.TemporaryDirectory() as project:
            Path(project, 'subject.py').write_text(case['source'])
            result = subprocess.run([str(binary), 'plan', '--root', project, '--file', 'subject.py',
                '--operators', 'collection_list_tuple', '--allow-best-effort-memory', '--', 'true'],
                capture_output=True, text=True, timeout=15)
            if result.returncode:
                raise RuntimeError(result.stderr)
            candidates = json.loads(result.stdout)['candidates']
            mutants = []
            for c in candidates:
                raw = case['source'].encode()
                start = c['span']['start']
                end = start + c['span']['length']
                assert raw[start:end].decode() == c['original']
                source = (raw[:start] + c['replacement'].encode() + raw[end:]).decode()
                mutants.append({'source': source, **compile_source(source)})
            rows.append({'id': case['id'], 'mode': 'strict',
                'expected_candidate_count': case['expected_candidate_count'],
                'actual_candidate_count': len(candidates),
                'status': 'match' if len(candidates) == case['expected_candidate_count'] else 'mismatch',
                'candidates': candidates, 'mutants': mutants})
    report = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'rows': rows}
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print('cases', len(rows), 'mismatches', sum(r['status'] == 'mismatch' for r in rows),
          'invalid_mutants', sum(not m['valid'] for r in rows for m in r['mutants']))


if __name__ == '__main__':
    main()
