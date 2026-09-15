"""Evaluate the Lean-owned intended replacements, including missing candidates."""
import argparse
import json
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    rows = []
    for line in (HERE / 'corpus.jsonl').read_text().splitlines():
        case = json.loads(line)
        for old, new in case['pairs']:
            if case['source'].count(old) != 1:
                raise RuntimeError(f"ambiguous expected replacement: {case['id']}")
            source = case['source'].replace(old, new, 1)
            probe = 'ns={}\nexec(' + repr(source) + ',ns)\nprint(str(ns["observed"]))'
            result = subprocess.run([str(HERE.parents[2] / '.venv/bin/python'), '-c', probe],
                                    capture_output=True, text=True, timeout=10, check=True)
            rows.append({'id': case['id'], 'replacement': new, 'exit': result.returncode,
                         'annotation': result.stdout.strip()})
    args.output.write_text(json.dumps(rows, indent=2) + '\n')
    print('Lean expected replacements evaluated successfully:', len(rows))


if __name__ == '__main__':
    main()
