"""Check proofs and corpus freshness with the repository's pinned Lean/guard."""
import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
FORMAL = ROOT / 'formal/HoiminOracle'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(exist_ok=False)
    with tempfile.TemporaryDirectory() as build:
        commands = [
            ['lake', 'env', 'lean', '-j1', '-DElab.async=false', '-R', str(HERE),
             '-o', str(Path(build) / 'SliceTupleModel.olean'), str(HERE / 'SliceTupleModel.lean')],
            ['lake', 'env', 'env', 'LEAN_PATH=' + build, 'lean', '-j1', '-DElab.async=false',
             '-R', str(HERE), '--run', str(HERE / 'SliceTupleMain.lean'),
             '--output', str(Path(build) / 'corpus.jsonl')],
        ]
        for name, command in zip(['model', 'search'], commands):
            stats = args.output / (name + '.json')
            with (args.output / (name + '.log')).open('wb') as log:
                result = subprocess.run([
                    sys.executable, 'tools/lean_resource_guard.py',
                    '--timeout-seconds', '20', '--rss-limit-mib', '2048',
                    '--sample-ms', '50', '--stats', str(stats.resolve()), '--', *command],
                    cwd=FORMAL, stdout=log, stderr=subprocess.STDOUT, timeout=25)
            print(name, json.loads(stats.read_text()))
            if result.returncode:
                raise RuntimeError((args.output / (name + '.log')).read_text())
        if not (HERE / 'corpus.jsonl').exists():
            (HERE / 'corpus.jsonl').write_bytes((Path(build) / 'corpus.jsonl').read_bytes())
        if (Path(build) / 'corpus.jsonl').read_bytes() != (HERE / 'corpus.jsonl').read_bytes():
            raise RuntimeError('Lean corpus differs from the checked-in corpus')
    print('proofs, sensitivity, bounded search and corpus freshness: passed')


if __name__ == '__main__':
    main()
