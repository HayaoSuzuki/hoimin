"""Static final integration checks; no build, proof or native platform claims."""
import json
import re
import subprocess
import tomllib
from pathlib import Path

BASE = '1e6200d71237c6e445f337babf02f5f0354b610f'
for name in ['Cargo.toml', 'pyproject.toml']:
    data = tomllib.loads(Path(name).read_text())
    version = data['workspace']['package']['version'] if name == 'Cargo.toml' else data['project']['version']
    assert version == '0.14.0', (name, version)
    print(f'{name}: package floor {version}')
for name in ['Cargo.lock', 'fuzz/Cargo.lock', 'uv.lock']:
    data = tomllib.loads(Path(name).read_text())
    local = [(p['name'], p['version']) for p in data['package'] if p['name'].startswith('hoimin') and p['name'] != 'hoimin-fuzz']
    assert local and all(v == '0.14.0' for _, v in local), (name, local)
    print(f'{name}: shipping local packages {local}')
fuzz = tomllib.loads(Path('fuzz/Cargo.toml').read_text())['package']
assert fuzz['version'] == '0.0.0' and fuzz['publish'] is False
print('Unpublished separate hoimin-fuzz harness retains its 0.0.0 placeholder')

for name, key, expected in [
    ('crates/hoimin-core/src/report.rs', 'REPORT_SCHEMA_VERSION', 9),
    ('crates/hoimin-cli/src/plan.rs', 'PLAN_SCHEMA_VERSION', 10),
    ('crates/hoimin-core/src/resume.rs', 'FINGERPRINT_SCHEMA_VERSION', 13),
    ('crates/hoimin-cli/src/session/schema.rs', 'SCHEMA_VERSION', 5),
    ('crates/hoimin-cli/src/plan/ranking.rs', 'RANKING_RULE_VERSION', 4),
    ('crates/hoimin-core/src/candidate.rs', 'CANDIDATE_SCHEMA_VERSION', 1),
]:
    found = re.search(r'const ' + key + r': [^=]+ = (\d+);', Path(name).read_text())
    assert found and int(found[1]) == expected, (name, key)
    print(f'{key}={expected}: {name}')
result = json.loads(Path('docs/json-schema/run-result.schema.json').read_text())
event = json.loads(Path('docs/json-schema/run-event.schema.json').read_text())
assert result['properties']['schema_version']['const'] == 9
for name, definition in event['$defs'].items():
    props = definition.get('properties', {})
    if 'schema_version' in props:
        version = props['schema_version']
        if '$ref' in version:
            assert version['$ref'] == '#/$defs/schemaVersion', (name, version)
            version = event['$defs']['schemaVersion']
        assert version['const'] == 9, name
assert 'status' in event['$defs']['mutantFinished']['required']
assert result['properties']['mutants']['items']['$ref'] == 'run-event.schema.json#/$defs/mutantFinished'
print('Both published schemas route schema9 and require projected status')

def changes(*paths):
    return subprocess.check_output(['git', 'diff', '--name-only', BASE, '--', *paths], text=True).splitlines()

assert not changes('formal/creusot', '.github/workflows/ci.yml', '.github/workflows/non-linux-ci.yml', 'tools/verify_creusot.py', 'rust-toolchain.toml', 'crates/hoimin-budget-math', 'fuzz/fuzz_targets', 'fuzz/Cargo.toml')
print('Protected #999 proof/tooling/workflows/math and separate fuzz target/harness sources unchanged')
goldens = changes('crates/hoimin-cli/tests/golden')
assert goldens and all('schema-v9-' in p for p in goldens), goldens
print('Only schema-v9 positive goldens changed; earlier golden bytes preserved')
assert not changes('docs/superpowers/specs', 'docs/superpowers/reports', 'docs/audits')
print('No original-source catalog inputs added/moved/deleted; plans/maintenance remain outside those catalogs')
print('Limits: static consistency only; no local proof rerun, fuzz run or Linux/Windows execution')
