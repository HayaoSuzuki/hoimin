"""Small serial CLI measurements; each process has a wall/RSS resource guard."""
import argparse
import hashlib
import json
import platform
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
GUARD = ROOT / 'formal/HoiminOracle/tools/lean_resource_guard.py'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    args.output.mkdir(exist_ok=False)
    rows = []
    # All fixture originals compile. Loops are never executed by plan.
    families = [('imports', [512, 1024, 2048]), ('loops', [18, 19, 20])]
    for family, sizes in families:
        for n in sizes:
            source = (''.join(f'import typing as t{i}\n' for i in range(n))
                      + 'x: int\n' * n if family == 'imports' else
                      ''.join(' ' * i + 'for _ in []:\n' for i in range(n))
                      + ' ' * n + 'x: list[int]\n')
            compile(source, '<performance-shape>', 'exec')
            for operators in ['type_list_sequence', 'boolean_literal']:
                for repetition in range(3):
                    stem = f'{family}-{n}-{operators}-{repetition}'
                    stats = args.output / (stem + '.stats.json')
                    with tempfile.TemporaryDirectory() as project:
                        Path(project, 'subject.py').write_text(source)
                        command = [str(binary), 'plan', '--root', project, '--file', 'subject.py',
                                   '--allow-best-effort-memory', '--operators', operators,
                                   '--analyzer-timeout', '5s', '--', 'true']
                        with (args.output / (stem + '.stdout')).open('wb') as out, \
                             (args.output / (stem + '.stderr')).open('wb') as err:
                            result = subprocess.run([sys.executable, str(GUARD),
                                '--timeout-seconds', '10', '--rss-limit-mib', '1024',
                                '--sample-ms', '10', '--stats', str(stats), '--', *command],
                                stdout=out, stderr=err, timeout=15)
                    stat = json.loads(stats.read_text())
                    stat['sampled_tree_rss_kib'] = stat['peak_rss_kib'] or None
                    row = {'family': family, 'n': n, 'operators': operators,
                           'repetition': repetition, 'input_bytes': len(source.encode()), **stat}
                    if result.returncode == 0:
                        document = json.loads((args.output / (stem + '.stdout')).read_text())
                        row.update(candidates=len(document['candidates']), truncated=document['truncated'])
                    rows.append(row)
                    print(json.dumps(row), flush=True)
                    if result.returncode:
                        raise RuntimeError('Guard/CLI failure; stop without increasing size')
    (args.output / 'summary.json').write_text(json.dumps({
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'platform': platform.platform(), 'python': sys.version,
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'rows': rows}, indent=2) + '\n')


if __name__ == '__main__':
    main()
