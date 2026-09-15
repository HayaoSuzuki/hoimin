"""Exercise initial run, persistent resume, and fresh run for each Lean case."""
import argparse
import hashlib
import json
import shutil
import sqlite3
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PYTHON = ROOT / '.venv/bin/python'
SOURCE = 'def value():\n return (1+2)+(3+4)\n'
CHECK = ("from pathlib import Path\nimport subject\n"
         "if Path('strict.flag').exists():\n assert subject.value()==10\n")


def file_hashes(root):
    return {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
            for p in root.iterdir() if p.is_file()}


def invoke(binary, root, case, copied, output_cap, session, resume):
    if shutil.disk_usage(root).free <= 10 * 1024**3:
        raise RuntimeError('free space must exceed 10 GiB')
    args = [str(binary), 'run', '--root', str(root), '--file', 'subject.py',
            '--operators', 'binary_add_sub', '--jobs', '1', '--max-mutants', '1',
            '--max-workspace-size', '8GiB', '--min-free-space', '10GiB',
            '--max-output', str(output_cap) + 'B', '--fingerprint-file', 'strict.flag',
            '--allow-best-effort-memory', '--format', 'json']
    if case['mechanism'] == 'include':
        args += ['--include', 'subject.py', '--include', 'check.py']
        if copied:
            args += ['--include', 'strict.flag']
    elif case['mechanism'] == 'exclude':
        if not copied:
            args += ['--exclude', 'strict.flag']
    else:
        raise RuntimeError('unknown fixture mechanism')
    if session:
        args += ['--session', str(session)]
    if resume:
        args += ['--resume']
    args += ['--', str(PYTHON), 'check.py']
    proc = subprocess.run(args, cwd=root, capture_output=True, text=True, timeout=25)
    if proc.returncode != 4:
        raise RuntimeError(f'expected max-mutants exit 4, got {proc.returncode}: {proc.stderr}')
    doc = json.loads(proc.stdout)
    if doc['baseline']['termination'] != {'Exit': 0}:
        raise RuntimeError('baseline failed')
    summary = doc['summary']
    if summary['complete'] or summary['counts']['not_run'] != 2:
        raise RuntimeError('unexpected incomplete-run shape')
    disk = summary['disk']
    if disk['stop'] is not None:
        raise RuntimeError(f'disk-limit stop: {disk["stop"]}')
    cleanup = [{k: entry[k] for k in ('root_id', 'owner', 'status', 'remaining_root')}
               for entry in disk['cleanup']]
    if any(entry['status'] not in ('clean', 'cleanup_after_delivery')
           or entry['remaining_root'] is not None for entry in cleanup):
        raise RuntimeError(f'cleanup requires investigation: {cleanup}')
    candidate = doc['mutants'][0]
    if candidate['status'] not in ('killed', 'survived'):
        raise RuntimeError(f'unexpected first result: {candidate["status"]}')
    config = doc['run']['normalized_config']
    # Keep the evidence needed for correspondence, excluding unrelated janitor details.
    return {'exit_code': proc.returncode, 'run_id': doc['run']['run_id'],
            'baseline_termination': doc['baseline']['termination'],
            'first_mutant': candidate, 'counts': summary['counts'],
            'complete': summary['complete'], 'cleanup': cleanup,
            'selection': config['selection'], 'fingerprint_inputs': config['fingerprint_inputs'],
            'limits': config['limits'], 'argv': args}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    rows = []
    for line in (HERE / 'corpus.jsonl').read_text().splitlines():
        case = json.loads(line)
        row = {'id': case['id'], 'mode': case['mode'], 'expected': {
            'initial': case['expected_initial'], 'resumed': case['expected_resumed'],
            'fresh': case['expected_fresh'], 'reused': case['expected_reuse']}}
        try:
            if shutil.disk_usage(ROOT).free <= 10 * 1024**3:
                raise RuntimeError('repository free space must exceed 10 GiB')
            with tempfile.TemporaryDirectory(prefix='hoimin-copy-audit-') as tmp:
                root = Path(tmp, 'project')
                root.mkdir()
                (root / 'subject.py').write_text(SOURCE)
                (root / 'check.py').write_text(CHECK)
                (root / 'strict.flag').write_text('strict\n')
                if case['mechanism'] == 'include':
                    (root / '.ignore').write_text('strict.flag\n')
                original_test = subprocess.run([str(PYTHON), 'check.py'], cwd=root,
                                               capture_output=True, text=True, timeout=10)
                if original_test.returncode:
                    raise RuntimeError(f'original test failed: {original_test.stderr}')
                before_hashes = file_hashes(root)
                db = Path(tmp, 'session.sqlite3')
                first = invoke(binary, root, case, case['before_copied'], case['before_output'], db, False)
                resumed = invoke(binary, root, case, case['after_copied'], case['after_output'], db, True)
                with sqlite3.connect(f'file:{db}?mode=ro', uri=True) as connection:
                    session_rows = connection.execute(
                        'SELECT run_id, hex(fingerprint), complete FROM runs ORDER BY id').fetchall()
                fresh = invoke(binary, root, case, case['after_copied'], case['after_output'], None, False)
                after_hashes = file_hashes(root)
                if before_hashes != after_hashes:
                    raise RuntimeError('fixture files changed during replay')
                ids = [r['first_mutant']['candidate']['id'] for r in (first, resumed, fresh)]
                if len(set(ids)) != 1:
                    raise RuntimeError('candidate identities differ')
                reused = first['run_id'] == resumed['run_id']
                if reused != (resumed['first_mutant']['termination'] is None):
                    raise RuntimeError('reuse observations disagree')
                row['observed'] = {
                    'initial': first['first_mutant']['status'],
                    'resumed': resumed['first_mutant']['status'],
                    'fresh': fresh['first_mutant']['status'], 'reused': reused}
                row.update(initial=first, resumed=resumed, fresh=fresh,
                           session_rows=session_rows, unchanged_file_hashes=after_hashes)
                row['status'] = 'match' if row['observed'] == row['expected'] else 'mismatch'
        except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired,
                sqlite3.Error) as error:
            row.update(mode='infrastructure-error', status='infrastructure error', error=str(error))
        rows.append(row)
        if row['status'] == 'infrastructure error':
            break  # Do not continue after disk, cleanup, or observation failures.
    summary = {status: sum(r['status'] == status for r in rows)
               for status in ('match', 'mismatch', 'infrastructure error')}
    args.output.write_text(json.dumps({
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'python': subprocess.check_output([str(PYTHON), '--version'], text=True).strip(),
        'summary': summary, 'cases': rows}, indent=2) + '\n')
    print(summary)
    if summary['infrastructure error']:
        raise SystemExit(2)


if __name__ == '__main__':
    main()
