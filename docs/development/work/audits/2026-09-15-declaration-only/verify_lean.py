"""Check the small audit under the pinned toolchain and resource guard."""
import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
FORMAL = HERE.parents[2] / 'formal/HoiminOracle'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--generate', action='store_true')
    args = parser.parse_args()
    args.output.mkdir(exist_ok=False)
    with tempfile.TemporaryDirectory() as build:
        commands = [
            ['lake', 'env', 'lean', '-j1', '-DElab.async=false', '-R', str(HERE),
             '-o', str(Path(build) / 'DeclarationModel.olean'), str(HERE / 'DeclarationModel.lean')],
            ['lake', 'env', 'env', 'LEAN_PATH=' + build, 'lean', '-j1', '-DElab.async=false',
             '-R', str(HERE), '--run', str(HERE / 'DeclarationMain.lean'),
             '--output' if args.generate else '--check', str(HERE / 'corpus.jsonl')],
        ]
        for name, command in zip(['model', 'search'], commands):
            stats = args.output / (name + '.json')
            log_path = args.output / (name + '.log')
            with log_path.open('wb') as log:
                result = subprocess.run([
                    sys.executable, 'tools/lean_resource_guard.py',
                    '--timeout-seconds', '20', '--rss-limit-mib', '2048',
                    '--sample-ms', '50', '--stats', str(stats.resolve()), '--', *command],
                    cwd=FORMAL, stdout=log, stderr=subprocess.STDOUT, timeout=25)
            print(name, json.loads(stats.read_text()), flush=True)
            if result.returncode:
                raise RuntimeError(log_path.read_text())
    print('model proofs, finite checks, sensitivity and corpus '
          + ('generation' if args.generate else 'freshness') + ': passed')


if __name__ == '__main__':
    main()
