"""Observe an invalid mutant counted as killed by an import-only test."""
import json
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
case = next(row for line in (HERE / 'corpus.jsonl').read_text().splitlines()
            if (row := json.loads(line))['expected_candidate_count'] == 0)
with tempfile.TemporaryDirectory() as project:
    Path(project, 'subject.py').write_text(case['source'])
    result = subprocess.run([str(ROOT / 'target/release/hoimin'), 'run', '--root', project,
        '--file', 'subject.py', '--operators', 'collection_list_tuple',
        '--allow-best-effort-memory', '--min-free-space', '1B', '--',
        str(ROOT / '.venv/bin/python'), '-c', 'import subject'],
        text=True, capture_output=True, timeout=20)
report = json.loads(result.stdout)
observation = {'case': case, 'cli_exit': result.returncode, 'stderr': result.stderr,
               'baseline_termination': report['baseline']['termination'],
               'counts': report['summary']['counts'], 'complete': report['summary']['complete'],
               'mutants': report['mutants']}
(HERE / 'run.json').write_text(json.dumps(observation, indent=2) + '\n')
print(json.dumps({k: v for k, v in observation.items() if k not in ['mutants', 'case']}, indent=2))
