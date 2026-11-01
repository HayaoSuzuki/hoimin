"""Verify the task-owned durable copies/index; does not execute product tests."""
import hashlib
import json
import re
import subprocess
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path.cwd()
PLAN = ROOT / '.superpowers/sdd/2026-10-19-priority-six'
ARCHIVE = ROOT / 'docs/superpowers/plans/2026-10-19-priority-six-evidence'
COMPANION = ARCHIVE.parent / '2026-10-19-priority-six-execution.md'

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

manifest = json.loads((ARCHIVE / 'archive-manifest.json').read_text())
listed = set()
for entry in manifest['entries']:
    path = ARCHIVE / entry['path']
    assert path.is_file() and digest(path) == entry['sha256'], entry['path']
    listed.add(entry['path'])
actual = {p.relative_to(ARCHIVE).as_posix() for p in ARCHIVE.rglob('*') if p.is_file() and p.name != 'archive-manifest.json'}
assert actual == listed, (actual - listed, listed - actual)
for i in range(1, 8):
    assert (ARCHIVE / f'task-{i}-report.md').read_bytes() == (PLAN / f'task-{i}-report.md').read_bytes(), i
for i in range(1, 7):
    assert (ARCHIVE / f'task-{i}-review.md').read_bytes() == (PLAN / f'task-{i}-review.md').read_bytes(), i
for i in (2, 4, 5):
    assert (ARCHIVE / f'task-{i}-rereview-1.md').read_bytes() == (PLAN / f'task-{i}-rereview-1.md').read_bytes(), i
print('Reports: all seven full reports, six initial independent reviews and three scoped re-reviews match their source bytes')

def anchors(text):
    found = {}
    for match in re.finditer(r'^(#{1,6}) (.+)$', text, re.MULTILINE):
        slug = re.sub(r'[^\w\s-]', '', match[2].lower()).replace(' ', '-')
        end = re.search(r'^#{1,6} ', text[match.end():], re.MULTILINE)
        body = text[match.end():match.end() + end.start()] if end else text[match.end():]
        found.setdefault(slug, body)
    return found

stage_links = re.findall(r'\]\((2026-10-19-priority-six-evidence/task-[1-7]-report\.md)#([^)]+)\)', COMPANION.read_text())
assert len(stage_links) == 21, len(stage_links)
for target, anchor in stage_links:
    sections = anchors((COMPANION.parent / target).read_text())
    assert anchor in sections, (target, anchor)
    numbers = [int(x) for x in re.findall(r'^([1-5])\. ', sections[anchor], re.MULTILINE)]
    assert numbers == [1, 2, 3, 4, 5], (target, anchor, numbers)
print('Self-reviews: seven tasks x three stages x five distinct numbered passes = 105 indexed entries; fix appendices retained')

ledger = (PLAN / 'progress.md').read_text()
rulings = {int(m[1]): m[0] for m in re.finditer(r'^- R(\d+) —[^\n]+', ledger, re.MULTILINE)}
assert set(rulings) == set(range(1, max(rulings) + 1))
archived_rulings = (ARCHIVE / 'controller-rulings.md').read_text()
for number, text in rulings.items():
    assert text in archived_rulings, number
print(f'Rulings: R1 through R{max(rulings)} retained verbatim with reason/cost text')

issues = json.loads((ARCHIVE / 'issue-snapshot.json').read_text())
original = json.loads((PLAN / 'issue-snapshot.json').read_text())
assert {x['number'] for x in issues} == {1003, 1002, 1004, 1015, 1010, 1005}
assert all(x in original for x in issues)
checks = json.loads((ARCHIVE / 'checks.json').read_text())['checks']
assert len(checks) == 16 and all(x['exit_code'] == 0 for x in checks)
for check in checks:
    assert digest(ARCHIVE / check['excerpt']) == check['excerpt_sha256'], check['name']
    source_log = Path(check['log'])
    assert source_log.exists() and digest(source_log) == check['log_sha256'], check['name']
    for path, expected in check.get('inputs_sha256', {}).items():
        assert digest(ROOT / path) == expected, path
gates = json.loads((ARCHIVE / 'performance-gates.json').read_text())['gates']
assert len(gates) == 40 and all(x['status'] == 'passed' for x in gates)
print('Checks: 16 successful command receipts, 40 passed registered gates, original log hashes and selected excerpt hashes verified')

for entry in manifest['linked_measurement_files']:
    path = ROOT / entry['path']
    assert digest(path) == entry['sha256'], entry['path']
    subprocess.run(['git', 'ls-files', '--error-unmatch', entry['path']], check=True, stdout=subprocess.DEVNULL)
print(f'Measurements: {len(manifest["linked_measurement_files"])} existing tracked Task 6 artifacts match the manifest; no measurement directory duplicated')

for value in re.findall(r'(?<!!)\[[^\]\n]+\]\(([^\s)]+)\)', COMPANION.read_text()):
    parts = urlsplit(value)
    if parts.scheme or not parts.path:
        continue
    target = (COMPANION.parent / unquote(parts.path)).resolve()
    assert target.exists(), value
    if parts.fragment and target.suffix == '.md':
        assert parts.fragment in anchors(target.read_text()), value
assert not subprocess.check_output(['git', 'diff', '--name-only', 'HEAD', '--', 'crates', 'tests', 'tools', 'formal', 'vendor', 'fuzz', 'Cargo.toml', 'Cargo.lock', 'pyproject.toml', 'uv.lock', '.github'], text=True)
print('Scope: companion links resolve; Task 7 changes no product, test, manifest, workflow or proof input')
print(f'Archive: {len(listed)} file hashes verified; manifest excludes its own content hash')
print('Limits: Task 7 independent review, final whole-branch review, publication and native CI are still pending; no workspace/artifact cleanup performed')
