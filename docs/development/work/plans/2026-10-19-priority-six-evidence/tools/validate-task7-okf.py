"""Task-scoped OKF structure/reference check; content review is separate."""
import hashlib
import re
import subprocess
from pathlib import Path
from urllib.parse import unquote, urlsplit

import yaml

ROOT = Path.cwd()
BUNDLE = ROOT / 'docs/knowledge'
BASE = '1e6200d71237c6e445f337babf02f5f0354b610f'
changed = set(subprocess.check_output(
    ['git', 'diff', '--name-only', BASE, '--', 'docs/knowledge'], text=True
).splitlines())
pages = sorted(BUNDLE.rglob('*.md'))
assert pages, 'OKF bundle missing'
metadata = {}
for path in pages:
    text = path.read_text()
    header = text.startswith('---\n')
    if path.name in ('index.md', 'log.md'):
        if path == BUNDLE / 'index.md':
            assert header, path
            front, _ = text[4:].split('\n---\n', 1)
            assert yaml.safe_load(front) == {'okf_version': '0.2'}, path
        else:
            assert not header, path
        continue
    assert header, path
    front, _ = text[4:].split('\n---\n', 1)
    meta = yaml.safe_load(front)
    assert isinstance(meta, dict), path
    assert isinstance(meta.get('type'), str) and meta['type'].strip(), path
    metadata[path] = meta
print(f'Structure: YAML/type/reserved-file checks passed for {len(pages)} Markdown files')

def local_target(path, value):
    parts = urlsplit(value.strip('<>'))
    if parts.scheme or parts.netloc or not parts.path:
        return None
    return (path.parent / unquote(parts.path)).resolve()

link_pattern = re.compile(r'(?<!!)\[[^\]\n]+\]\(([^\s)]+)(?:\s+"[^"]*")?\)')
source_count = link_count = 0
historical = []
unavailable = []
inherited_missing = []
new_sources = []
task_snapshots = ['451a4fdd', '3d9e33d1', '6d78789e', '8720b41a', 'b199e83d', '6ea967d6', 'b2b01195', 'faff6415', '3aaf82b0']
for path in sorted(metadata):
    relative = path.relative_to(ROOT).as_posix()
    if relative not in changed:
        continue
    meta = metadata[path]
    body = path.read_text().split('\n---\n', 1)[1]
    old_text = subprocess.check_output(['git', 'show', BASE + ':' + relative], text=True)
    old_front, old_body = old_text[4:].split('\n---\n', 1)
    old_meta = yaml.safe_load(old_front)
    ids = []
    for source in meta.get('sources', []):
        sid = source.get('id')
        assert isinstance(sid, str) and sid, (relative, source)
        ids.append(sid)
        source_changed = source not in old_meta.get('sources', [])
        target = local_target(path, source['resource'])
        if target is not None:
            if not target.exists():
                # Keep historical references, but verify that this branch did not
                # introduce the missing path or change the recorded source entry.
                assert source in old_meta.get('sources', []), (relative, sid, target)
                old_target = target.relative_to(ROOT).as_posix()
                assert subprocess.run(['git', 'cat-file', '-e', BASE + ':' + old_target], capture_output=True).returncode != 0, (relative, sid, 'path removed by this branch')
                inherited_missing.append((relative, sid, old_target))
            assert '.superpowers/sdd' not in str(target), (relative, sid)
        raw_revision = source.get('revision')
        # An inherited short all-digit revision is a YAML integer. Preserve its
        # source metadata and use its literal decimal form for the object query.
        revision = str(raw_revision) if raw_revision is not None else None
        if revision:
            available = subprocess.run(['git', 'cat-file', '-e', revision + '^{commit}'], capture_output=True).returncode == 0
            if source_changed:
                assert available, (relative, sid, revision)
            elif not available:
                unavailable.append((relative, sid, revision))
        expected = source.get('sha256')
        if expected and target is not None and target.is_file():
            current = hashlib.sha256(target.read_bytes()).hexdigest()
            if source_changed:
                match = 'current working-tree bytes'
                if current != expected:
                    match = None
                    source_path = target.relative_to(ROOT).as_posix()
                    for snapshot in task_snapshots:
                        committed = subprocess.run(['git', 'show', snapshot + ':' + source_path], capture_output=True)
                        if committed.returncode == 0 and hashlib.sha256(committed.stdout).hexdigest() == expected:
                            match = snapshot + ':' + source_path
                            break
                    assert match, (relative, sid, 'new read-time hash not found in task snapshots')
                new_sources.append((relative, sid, match))
            elif current != expected:
                # Existing modified snapshots describe their actual read-time state.
                # A later HEAD is not a license to refresh that historical evidence.
                historical.append((relative, sid))
        source_count += 1
    assert len(ids) == len(set(ids)), (relative, 'duplicate source id')
    definitions = re.findall(r'^\[\^([^\]]+)\]:', body, re.MULTILINE)
    assert len(definitions) == len(set(definitions)), (relative, 'duplicate footnote definition')
    citations = set(re.findall(r'\[\^([^\]]+)\](?!:)', body))
    assert citations <= set(definitions), (relative, 'undefined citations', citations - set(definitions))
    for sid in ids:
        assert sid in definitions, (relative, 'source without footnote', sid)
    for value in link_pattern.findall(body):
        target = local_target(path, value)
        if target is not None:
            if not target.exists():
                assert value in old_body, (relative, value, target)
                old_target = target.relative_to(ROOT).as_posix()
                assert subprocess.run(['git', 'cat-file', '-e', BASE + ':' + old_target], capture_output=True).returncode != 0, (relative, value, 'path removed by this branch')
                inherited_missing.append((relative, 'body-link', old_target))
            link_count += 1
print(f'References: {len(changed)} changed concepts, {source_count} sources and {link_count} local link paths inspected; new/changed references valid')
print(f'New/changed source metadata: {len(new_sources)} exact hashes verified against current bytes or accepted task snapshots')
for relative, sid, match in new_sources:
    print(f'  verified source: {relative} / {sid} / {match}')
print(f'Historical metadata: preserved {len(historical)} read-time hashes differing from current source bytes; no historical execution refreshed')
print(f'Historical revisions: {len(unavailable)} source entries reference commit objects unavailable locally; these revisions were not verified or refreshed')
for relative, sid, revision in unavailable:
    print(f'  unavailable: {relative} / {sid} / {revision}')
print(f'Inherited missing paths: {len(inherited_missing)} source/body references already missing at the original base; not claimed as valid')
for relative, sid, target in inherited_missing:
    print(f'  inherited missing: {relative} / {sid} / {target}')

seen = set()
pending = [BUNDLE / 'index.md']
while pending:
    path = pending.pop().resolve()
    if path in seen:
        continue
    seen.add(path)
    for value in link_pattern.findall(path.read_text()):
        target = local_target(path, value)
        if target is not None and target.is_file() and target.suffix == '.md' and target.is_relative_to(BUNDLE):
            pending.append(target)
for relative in changed:
    assert (ROOT / relative).resolve() in seen, ('changed concept unreachable', relative)
print('Reachability: all changed concepts reachable from docs/knowledge/index.md')
originals = subprocess.check_output([
    'git', 'diff', '--name-status', BASE, '--',
    'docs/superpowers/specs', 'docs/superpowers/reports', 'docs/audits'
], text=True)
assert not originals, ('new original catalog input requires review', originals)
print('Catalog inputs: no added/moved/deleted originals in specs/reports/audits; plans/maintenance remain outside source catalogs')
print('Limits: local path checks do not validate URL availability or heading fragments; manual claim/evidence review is recorded in task-7-report.md')
