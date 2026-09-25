# Issue #561 method replacement allocation results

On 2026-09-25, the original allocation probe reproduced the issue at base `65865ea`. Selection guards remove all measured extra large allocation requests for nested unselected append calls. Both after runs exactly match the equal-length `ignore` control at every tested depth.

| Depth | Before append requests | Before append cumulative bytes | After append requests | After append cumulative bytes | Ignore cumulative bytes (before and after) |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 6 | 4,000,148 | 2 | 1,000,022 | 1,000,022 |
| 8 | 34 | 25,003,130 | 2 | 1,000,106 | 1,000,106 |
| 16 | 66 | 49,010,858 | 2 | 1,000,202 | 1,000,202 |
| 32 | 130 | 97,040,138 | 2 | 1,000,394 | 1,000,394 |

These are successful `System` alloc/alloc_zeroed/realloc requests of at least 500,000 bytes, summed over discovery. They are not retained/live bytes or peak RSS. Realloc counts the new requested size, not its delta. The depth-32 reduction is 96,039,744 cumulative requested bytes (about 98.97%) in this thresholded measurement. No elapsed-time speedup or release-build allocation claim is made. Debug elapsed times remain in raw output for transparency; concurrent testing and warm-up affect them.

Environment: macOS arm64, rustc 1.98.1 (`48a229cea 2026-09-01`), debug build with debuginfo disabled, incremental compilation disabled, two build jobs. The fixture generation, filesystem write and Tokio runtime construction occur before resetting counters. The unchanged [probe](alloc_probe.rs) calls public `discover_targets` with only `binary_add_sub` and limit 1, and asserts one `+`→`-` candidate in all eight cases. The Python fixtures compile independently.

Raw records: [before](issue-561-allocations-before.json), [after run 1](issue-561-allocations-after-1.json), [after run 2](issue-561-allocations-after-2.json). The pre-guard public candidate snapshot is in `crates/hoimin-cli/tests/fixtures/method-replacements/candidates.json`; the test checks all 14 candidates' complete descriptors and IDs, and every limit from 0 through 15.

## Reproduction

Run from the issue worktree. For the before measurement use base `65865ea` in its own checkout; use the same build configuration. The historical `measure_allocations.py` assumes a single artifact per dependency under `target`; Cargo's JSON artifact output below selects the actual build's files even if tests have also built Tokio with `test-util`.

```sh
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-561-target \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 \
cargo build --offline -p hoimin-cli --lib --message-format=json \
  > /private/tmp/issue-561-build-artifacts.jsonl

python3 - <<'PY'
import json
import subprocess
import tempfile
from pathlib import Path

root = Path('/private/tmp/hoimin-issue-561-target/debug')
artifacts = {}
for line in Path('/private/tmp/issue-561-build-artifacts.jsonl').read_text().splitlines():
    event = json.loads(line)
    if event['reason'] == 'compiler-artifact':
        for filename in event['filenames']:
            if filename.endswith('.rlib'):
                artifacts[event['target']['name']] = filename
command = [
    'rustc', '--edition=2024',
    'docs/audits/2026-09-15-evaluation-order/alloc_probe.rs',
    '-L', f'dependency={root}/deps',
]
for name in ('hoimin_cli', 'hoimin_core', 'camino', 'tokio'):
    command += ['--extern', f'{name}={artifacts[name]}']
command += ['-o', '/private/tmp/issue-561-alloc-probe']
subprocess.run(command, check=True)
for depth in (1, 8, 16, 32):
    for method in ('ignore', 'append'):
        source = f'obj.{method}(' * depth + "('" + 'a' * 500_000 + "', 1+2)" + ')' * depth + '\n'
        compile(source, '<allocation-fixture>', 'exec')
for _ in range(2):
    with tempfile.TemporaryDirectory() as project:
        subprocess.run(['/private/tmp/issue-561-alloc-probe', project], check=True)
PY
```

Unit tests instrument the actual seven helper entrances and three string-construction boundaries (whole-call copy and the two mapping formats). Direct helper calls prove positive counts; an unsupported extend argument proves entry can occur without construction. These counters measure entry/construction events, while the standalone allocator probe measures successful allocator requests. They deliberately answer different questions.
