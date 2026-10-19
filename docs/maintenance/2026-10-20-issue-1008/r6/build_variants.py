"""Build exact task-local ordinary variants serially, restoring source in finally."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path.cwd()
OUT = Path('/tmp/hoimin-perf-three-task3-r6')
TARGET = Path('/tmp/hoimin-perf-three-target')
EVIDENCE = ROOT / 'docs/maintenance/2026-10-20-issue-1008'
BASE = 'e5e33603a4ed6a269d4131fbb6dbe99a409de05c'
FIX_BASE = '952ab52780b181ae8de316e4b1f45988b5d0d1b0'
FILES = ['crates/hoimin-core/src/candidate.rs', 'crates/hoimin-python/src/analyzer/rust.rs']


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, timeout):
    print('COMMAND', command, flush=True)
    subprocess.run(command, check=True, timeout=timeout)


assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == FIX_BASE
original = {name: (ROOT / name).read_bytes() for name in FILES}
for name, content in original.items():
    assert content == subprocess.check_output(['git', 'show', f'{FIX_BASE}:{name}'])
prior = json.loads((EVIDENCE / 'build-before.json').read_text())['hashes']
for name, expected in prior.items():
    assert sha(Path(name)) == expected, name
receipt = {'base': BASE, 'candidate': FIX_BASE, 'commands': [], 'variants': {}, 'environment': {
    name: os.environ.get(name) for name in ['CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'CARGO_INCREMENTAL',
        'RUST_TEST_THREADS', 'PREK_NO_CONCURRENCY', 'UV_PROJECT_ENVIRONMENT', 'UV_CACHE_DIR', 'PATH', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']},
    'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
    'cargo': subprocess.check_output(['cargo', '--version'], text=True).strip()}


def build(variant):
    command = ['cargo', 'build', '--locked', '--release', '-p', 'hoimin-cli']
    receipt['commands'].append(command)
    run(command, 300)
    binary = OUT / f'hoimin-{variant}'
    shutil.copyfile(TARGET / 'release/hoimin', binary)
    binary.chmod(0o755)
    command = ['python3', str(EVIDENCE / 'build_probe.py'), str(TARGET), f'r6-{variant}']
    receipt['commands'].append(command)
    run(command, 180)
    old_out = Path('/tmp/hoimin-perf-three-task3')
    probe = OUT / f'probe-{variant}'
    shutil.copyfile(old_out / f'probe-r6-{variant}', probe)
    probe.chmod(0o755)
    shutil.copyfile(old_out / f'build-r6-{variant}.json', OUT / f'build-probe-{variant}.json')
    receipt['variants'][variant] = {
        'source_hashes': {name: sha(ROOT / name) for name in FILES},
        'binary': str(binary), 'binary_sha256': sha(binary),
        'probe': str(probe), 'probe_sha256': sha(probe),
        'cargo_lock_sha256': sha(ROOT / 'Cargo.lock'),
        'cargo_toml_sha256': sha(ROOT / 'Cargo.toml'),
        'probe_source_sha256': sha(EVIDENCE / 'probe.rs'),
    }


try:
    for name in FILES:
        (ROOT / name).write_bytes(subprocess.check_output(['git', 'show', f'{BASE}:{name}']))
    receipt['base_production_diff'] = subprocess.check_output(['git', 'diff', BASE, '--', *FILES], text=True)
    assert not receipt['base_production_diff']
    build('base')
finally:
    for name, content in original.items():
        (ROOT / name).write_bytes(content)
    receipt['source_restored'] = all((ROOT / name).read_bytes() == content for name, content in original.items())
    (OUT / 'build-variants.json').write_text(json.dumps(receipt, indent=2) + '\n')

build('lazy')
receipt['candidate_matches_prior_ordinary_binary'] = sha(OUT / 'hoimin-lazy') == sha(Path('/tmp/hoimin-perf-three-task3/hoimin-after'))
assert receipt['candidate_matches_prior_ordinary_binary']
for name, expected in prior.items():
    assert sha(Path(name)) == expected, name
receipt['original_baseline_and_prior_probe_unchanged'] = True
(OUT / 'build-variants.json').write_text(json.dumps(receipt, indent=2) + '\n')
print('SOURCE_RESTORED_AND_VARIANTS_PRESERVED', flush=True)
