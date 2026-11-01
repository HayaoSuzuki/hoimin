"""Replay Lean-owned fixtures through the real plan CLI and CPython 3.14."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


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
            path = Path(project) / 'subject.py'
            path.write_text(case['source'])
            command = [str(binary), 'plan', '--root', project, '--file', 'subject.py',
                       '--allow-best-effort-memory', '--operators', 'type_list_sequence',
                       '--', 'true']
            result = subprocess.run(command, capture_output=True, text=True, timeout=20)
            if result.returncode != 0:
                rows.append({'id': case['id'], 'status': 'infrastructure error',
                             'stderr': result.stderr, 'exit': result.returncode})
                continue
            document = json.loads(result.stdout)
            candidates = document['candidates']
            runtime = []
            # The exception is raised by a supplied real callable, without modifying source.
            for raises in [False, True]:
                probe = (
                    'import typing,json,sys\n'
                    'def hazard():\n'
                    + ('    raise KeyError()\n' if raises else '    pass\n')
                    + "ns={'hazard':hazard}\n"
                    + "exec(compile(open(sys.argv[1]).read(),sys.argv[1],'exec'),ns)\n"
                    + "print(json.dumps({'typing':ns['observed']==typing.Sequence[int],"
                      "'observed':str(ns['observed'])}))\n"
                )
                observed = subprocess.run([str(ROOT / '.venv/bin/python'), '-c', probe, str(path)],
                                          capture_output=True, text=True, timeout=10)
                if observed.returncode:
                    raise RuntimeError(observed.stderr)
                runtime.append({'raises': raises, **json.loads(observed.stdout)})
            rows.append({'id': case['id'], 'mode': case['mode'],
                         'expected_candidate_count': case['expected_candidate_count'],
                         'actual_candidate_count': len(candidates),
                         'status': 'match' if len(candidates) == case['expected_candidate_count'] else 'mismatch',
                         'candidates': candidates, 'runtime': runtime})
    report = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'results': rows}
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
