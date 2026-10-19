"""Build this same public-API probe against preserved and current release rlibs."""
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[3]
target = pathlib.Path(sys.argv[1])
output = pathlib.Path(sys.argv[2])
deps = target / 'release/deps'
command = ['rustc', '--edition=2024', '-O', '-C', 'lto=thin', '-C',
           'embed-bitcode=yes', str(pathlib.Path(__file__).with_name('probe.rs')),
           '-L', f'dependency={deps}']
for crate in ['camino', 'hoimin_cli', 'hoimin_core', 'blake3', 'tempfile']:
    matches = list(deps.glob(f'lib{crate}-*.rlib'))
    assert len(matches) == 1, (crate, matches)
    command += ['--extern', f'{crate}={matches[0]}']
command += ['-o', str(output)]
print(command, flush=True)
subprocess.run(command, check=True, timeout=180, cwd=root)
