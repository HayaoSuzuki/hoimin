"""Compile an allocator probe against existing local artifacts and measure once."""
import argparse
import json
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--profile', choices=('debug',), default='debug')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    artifacts = ROOT / 'target' / args.profile
    deps = artifacts / 'deps'
    with tempfile.TemporaryDirectory() as build, tempfile.TemporaryDirectory() as project:
        binary = Path(build) / 'alloc-probe'
        command = ['rustc', '--edition=2024', str(HERE / 'alloc_probe.rs'),
                   '-L', f'dependency={deps}', '--extern', f'hoimin_cli={artifacts}/libhoimin_cli.rlib']
        for name in ('hoimin_core', 'camino', 'tokio'):
            paths = list(deps.glob(f'lib{name}-*.rlib'))
            if len(paths) != 1:
                raise RuntimeError(f'ambiguous dependency {name}: {paths}')
            command += ['--extern', f'{name}={paths[0]}']
        command += ['-o', str(binary)]
        subprocess.run(command, check=True, timeout=30)
        # Compile every fixture independently of the analyzer, before measurement.
        for depth in (1, 8, 16, 32):
            for method in ('ignore', 'append'):
                source = f'obj.{method}(' * depth + "('" + 'a' * 500_000 + "', 1+2)" + ')' * depth + '\n'
                compile(source, '<allocation-fixture>', 'exec')
        result = subprocess.run([str(binary), project], capture_output=True, text=True,
                                timeout=20, check=True)
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        if len(rows) != 8:
            raise RuntimeError('incomplete measurement')
        args.output.write_text(json.dumps({'profile': args.profile, 'rows': rows}, indent=2) + '\n')
        print(result.stdout, end='')


if __name__ == '__main__':
    main()
