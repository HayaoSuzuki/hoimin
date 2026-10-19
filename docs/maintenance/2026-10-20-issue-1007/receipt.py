"""Record checked source, tools, preserved rlibs, probes and raw-command receipts."""
import hashlib
import json
import pathlib
import platform
import shutil
import subprocess

root = pathlib.Path(__file__).resolve().parents[3]
here = pathlib.Path(__file__).resolve().parent
raw = pathlib.Path('/tmp/hoimin-perf-three-task2')

def command(*args):
    return subprocess.check_output(args, cwd=root, text=True, timeout=30).strip()

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

baseline = pathlib.Path('/tmp/hoimin-perf-three-baseline')
original = json.loads((root / '.superpowers/sdd/2026-10-20-perf-three/baseline-release-receipt.json').read_text())
assert digest(baseline / 'release/hoimin') == original['binary_sha256']
sources = ['crates/hoimin-cli/src/workspace/manifest.rs', 'crates/hoimin-cli/src/workspace/copy.rs',
           'crates/hoimin-cli/tests/workspace_manifest_heap.rs', 'crates/hoimin-cli/tests/workspace_worker_metadata_heap.rs',
           'docs/performance/shapes.json', 'Cargo.lock', 'Cargo.toml']
source_hashes = {name: digest(root / name) for name in sources}
source_hashes.update({str(p.relative_to(root)): digest(p) for p in here.iterdir() if p.suffix in ('.rs', '.py', '.json') and p.name != 'verification.json'})
rlibs = {}
for target in (baseline, pathlib.Path('/tmp/hoimin-perf-three-target')):
    for crate in ('hoimin_cli', 'hoimin_core', 'hoimin_python', 'camino', 'tempfile', 'blake3'):
        paths = list((target / 'release/deps').glob(f'lib{crate}-*.rlib'))
        assert len(paths) == 1, paths
        rlibs[str(paths[0])] = digest(paths[0])
tool_paths = {tool: pathlib.Path(shutil.which(tool)).resolve() for tool in ('python3', 'cargo')}
tool_paths['rustc'] = pathlib.Path(command('rustup', 'which', 'rustc'))
receipt = dict(
    issue=1007, checked_base=command('git', 'rev-parse', 'HEAD'), baseline_product=original['commit'],
    tracked_diff_sha256=hashlib.sha256(command('git', 'diff', '--binary').encode()).hexdigest(),
    source_sha256=source_hashes,
    original_manifest_sha256=hashlib.sha256(subprocess.check_output(['git', 'show', original['commit'] + ':crates/hoimin-cli/src/workspace/manifest.rs'], cwd=root, timeout=30)).hexdigest(),
    release_rlibs_sha256=rlibs, original_binary_sha256=original['binary_sha256'],
    tools=dict(platform=platform.platform(), rustc=command('rustc', '-Vv'), cargo=command('cargo', '-V'), python=command('python3', '--version'), executables={str(p): digest(p) for p in tool_paths.values()}),
    environment=dict(CARGO_BUILD_JOBS='2', CARGO_TARGET_DIR='/tmp/hoimin-perf-three-target', CARGO_INCREMENTAL='0', RUST_TEST_THREADS='2', PREK_NO_CONCURRENCY='1', UV_PROJECT_ENVIRONMENT='/tmp/hoimin-priority-six-venv', UV_CACHE_DIR='/tmp/hoimin-priority-six-uv-cache', PATH_prefix='/tmp/hoimin-priority-six-venv/bin'),
    raw_receipts={str(p): digest(p) for p in raw.glob('*.log')},
    probes_sha256={str(p): digest(p) for p in raw.glob('probe-*')},
    reproduction=['cargo build --locked -p hoimin-cli --release --lib',
                  'python3 docs/maintenance/2026-10-20-issue-1007/build_probe.py /tmp/hoimin-perf-three-baseline /tmp/hoimin-perf-three-task2/probe-before',
                  'python3 docs/maintenance/2026-10-20-issue-1007/build_probe.py /tmp/hoimin-perf-three-target /tmp/hoimin-perf-three-task2/probe-after',
                  'python3 -u docs/maintenance/2026-10-20-issue-1007/measure.py /tmp/hoimin-perf-three-task2/probe-before /tmp/hoimin-perf-three-task2/probe-after docs/maintenance/2026-10-20-issue-1007/measurement.json'],
    limits=['Requested live and peak heap are allocator-requested bytes, not RSS.',
            'No process RSS observations were taken.',
            'Elapsed windows exclude fixture/preflight, content verification and sequential cleanup.',
            'Snapshot-plus-worker payload is logical data, not filesystem allocated blocks or metadata.',
            'Actual copy charge is checked independently before and after sequential worker destruction.',
            'No wall-clock test threshold or universal materialization-speedup claim.'])
(here / 'verification.json').write_text(json.dumps(receipt, indent=2) + '\n')
print('Verified preserved baseline binary; recorded', len(source_hashes), 'sources and', len(rlibs), 'rlibs')
