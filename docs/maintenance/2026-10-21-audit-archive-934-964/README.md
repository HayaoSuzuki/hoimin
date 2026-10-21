# October 16–17, 2026 design-audit source archive

Start with the [design-audit history](../../knowledge/audits/design-history-2026-10.md)
for the findings' disposition and the reasons to retain these records. This
directory preserves historical source changes; it does not define current CLI
behavior or a new backlog.

## Coverage and provenance

On 2026-10-21, eight local audit branches contained nine documentation commits
absent from `main` at `fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4`.
Their tips contained nine Markdown documents missing from that baseline.
The commits also changed existing concepts and indexes. Archiving complete diffs
preserves those changes without overwriting current documentation with old
assessments.

The [manifest](manifest.json) records branch heads, each audit's target revision,
commit parents and trees, changed-file blob IDs and SHA-256 digests, and patch
digests. It also records the GitHub issue/closing-PR snapshot read on 2026-10-21.
Branch names are provenance labels, not required live references. The archived
commits need not remain reachable after the original branches are deleted.

| Original branch (`audit/` prefix) | Audit target | Complete source diff |
| --- | --- | --- |
| `2026-10-16-design-followup` | `0d3fbc1a1821` | [01: follow-up](patches/01-2026-10-16-design-followup-f373e999a19a.patch) |
| `2026-10-16-post934` | `5237d1e53a2c` | [02: after #934](patches/02-2026-10-16-post934-34307704947c.patch) |
| `2026-10-16-post938` | `465ff928c1f2` | [03: after #938](patches/03-2026-10-16-post938-f3fe72ae3bb0.patch) |
| `2026-10-16-post948` | `ddb8ed9504be` | [04: after #948](patches/04-2026-10-16-post948-f6f4b8fc35dd.patch) |
| `2026-10-16-post954` | `578b3657e744` | [05: after #954](patches/05-2026-10-16-post954-9ec2470c4ef5.patch) |
| `2026-10-16-post958` | `30d158c7ead3` | [06: after #958](patches/06-2026-10-16-post958-fe1707ee82fd.patch) |
| `2026-10-17-post962` | `a72ca276efbb` | [07: after #962](patches/07-2026-10-17-post962-40c005433eaa.patch) |
| `2026-10-17-post964` | `229ce3853514` | [08: initial review](patches/08-2026-10-17-post964-ea2fb180a7b5.patch), [09: same-revision follow-up](patches/09-2026-10-17-post964-6c0b9a63fe7a.patch) |

Each patch is a parent-to-commit `git diff --binary --full-index --no-ext-diff
--no-renames`. Text, including original language, revision metadata, limitations
and historical review claims, is unchanged. These are source snapshots, not
patches intended to be applied to current `main`. Read added lines under the
recorded document path; old relative links resolve in the recorded source tree,
not in this directory. Temporary paths mentioned in originals are historical
references; their contents are not included or asserted to remain available.
The local Git attributes preserve patch bytes across checkouts and exempt source
diff data (including whitespace-only context lines) from whitespace cleanup.

## Reconstructing and checking the sources

Run the following from the repository root in a normal clone with the comparison
revision's history available. It checks each patch digest and reconstructs each
original tree using a temporary Git index. The eight first-parent base trees are
from `main` history; the first post964 patch recreates the parent tree needed by
its follow-up. No original audit branch or audit commit object is required.
This writes Git objects but leaves the checkout, real index and references alone.

```sh
python3 - <<'PY'
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

root = Path('docs/maintenance/2026-10-21-audit-archive-934-964')
manifest = json.loads((root / 'manifest.json').read_text())
for entry in manifest['commits']:
    patch = (root / entry['patch']).read_bytes()
    assert hashlib.sha256(patch).hexdigest() == entry['sha256']
    with tempfile.TemporaryDirectory(prefix='hoimin-audit-index-') as tmp:
        env = dict(os.environ, GIT_INDEX_FILE=str(Path(tmp) / 'index'))
        subprocess.run(['git', 'read-tree', entry['parent_tree']], env=env, check=True)
        subprocess.run(['git', 'apply', '--cached', '--whitespace=nowarn', '-'],
                       input=patch, env=env, check=True)
        tree = subprocess.check_output(['git', 'write-tree'], env=env).decode().strip()
        assert tree == entry['tree'], entry['commit']
        for source in entry['postimages']:
            data = subprocess.check_output(['git', 'show', f"{tree}:{source['path']}"])
            assert hashlib.sha256(data).hexdigest() == source['sha256']
print(f"Verified {len(manifest['commits'])} patches and their reconstructed trees")
PY
```

To read an original as plain Markdown after reconstruction, use its tree and path
from the manifest: `git show TREE:PATH`. For the post964 record, choose the second
commit's tree to include the follow-up. Git tree equality verifies file paths,
modes and blob content. It does not certify the historical audit's conclusions,
reproduce old execution results or supply missing temporary artifacts.

## Evidence boundary

The new work checks archive integrity, GitHub disposition, selected current
source boundaries and documentation. It does not rerun the original programs,
tests, benchmarks, host-specific checks or Lean proofs. Existing proof sources
and their evidence classifications remain authoritative in their established
locations; this archive neither replaces nor removes them.

The originals' scores, open-issue lists, self-review statements and proposed tests
describe their own dates and target revisions. Use the linked history page for
later fixes and reassess a decision when its original assumptions change.
