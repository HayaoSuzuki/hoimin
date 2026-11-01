import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path('/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-1003-safe-reads-1002-escape-1004-config-1015-cleanup-1010-status-1005-cycles')
OUT = Path('/tmp/hoimin-priority-six-task7')
OUT.mkdir(exist_ok=True)
ENV = os.environ.copy()
ENV.update(CARGO_BUILD_JOBS='2', CARGO_TARGET_DIR='/tmp/hoimin-priority-six-target', RUST_TEST_THREADS='2', UV_PROJECT_ENVIRONMENT='/tmp/hoimin-priority-six-venv', UV_CACHE_DIR='/tmp/hoimin-priority-six-uv-cache')
ENV['PATH'] = '/tmp/hoimin-priority-six-venv/bin:' + ENV['PATH']
UV = ['uv', 'run', '--frozen', '--no-sync']
CHECKS = {
    'fmt': ['cargo', 'fmt', '--all', '--', '--check'],
    'fmt-vendor': ['cargo', 'fmt', '--manifest-path', 'vendor/ruff_python_parser/Cargo.toml', '--', '--check'],
    'clippy': ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings'],
    'clippy-vendor': ['cargo', 'clippy', '--locked', '-p', 'littrs-ruff-python-parser', '--lib', '--no-deps', '--', '-D', 'warnings'],
    'workspace': ['cargo', 'test', '--workspace', '--locked', '--', '--test-threads=2'],
    'contracts-core': ['cargo', 'test', '--locked', '-p', 'hoimin-core', '--features', 'contracts', '--', '--test-threads=2'],
    'contracts-cli': ['cargo', 'test', '--locked', '-p', 'hoimin-cli', '--features', 'contracts', '--', '--test-threads=2'],
    'cli-example': ['cargo', 'test', '--locked', '-p', 'hoimin-cli', '--example', 'generate_cli_reference', '--', '--test-threads=2'],
    'cli-reference': ['cargo', 'run', '--locked', '-p', 'hoimin-cli', '--example', 'generate_cli_reference', '--', '--check'],
    'python-format': UV + ['ruff', 'format', '--check', '.'],
    'python-lint': UV + ['ruff', 'check', '--no-fix', '.'],
    'python-types': UV + ['ty', 'check'],
    'python-tests': UV + ['pytest', '-q'],
    'performance-gates': UV + ['python', 'tools/performance_shapes.py', 'gate', '--output', str(OUT / 'performance-gates')],
    'okf': UV + ['python', '.superpowers/sdd/2026-10-19-priority-six/validate-task7-okf.py'],
}

def read_command(args):
    return subprocess.check_output(args, cwd=ROOT, env=ENV, text=True).strip()

manifest = OUT / 'checks.json'
records = json.loads(manifest.read_text()) if manifest.exists() else []
for name in sys.argv[1:]:
    args = CHECKS[name]
    log = OUT / (name + '.log')
    if log.exists():
        raise SystemExit('Refusing to overwrite evidence: ' + str(log))
    head = read_command(['git', 'rev-parse', 'HEAD'])
    dirty = read_command(['git', 'status', '--porcelain'])
    diff = subprocess.check_output(['git', 'diff', 'HEAD', '--binary'], cwd=ROOT)
    started = time.time()
    print('START', name, 'head=' + head, flush=True)
    with log.open('w') as output:
        completed = subprocess.run(args, cwd=ROOT, env=ENV, stdout=output, stderr=subprocess.STDOUT)
    text = log.read_text(errors='replace')
    results = re.findall(r'test result: .*', text)
    record = {
        'name': name, 'argv': args, 'cwd': str(ROOT), 'head': head,
        'git_status': dirty, 'tracked_diff_sha256': hashlib.sha256(diff).hexdigest(),
        'environment': {k: ENV[k] for k in ('CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'RUST_TEST_THREADS', 'UV_PROJECT_ENVIRONMENT', 'UV_CACHE_DIR')},
        'path_prefix': '/tmp/hoimin-priority-six-venv/bin',
        'started_unix': started, 'elapsed_seconds': time.time() - started,
        'exit_code': completed.returncode, 'log': str(log),
        'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest(),
        'rust_result_lines': results,
    }
    records.append(record)
    manifest.write_text(json.dumps(records, indent=2) + '\n')
    print('END', name, 'exit=' + str(completed.returncode), 'seconds=' + str(round(record['elapsed_seconds'], 2)), flush=True)
    print('\n'.join(text.splitlines()[-5:]), flush=True)
    if completed.returncode:
        raise SystemExit(completed.returncode)
