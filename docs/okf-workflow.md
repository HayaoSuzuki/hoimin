# Development with OKF

OKF (Open Knowledge Format) organizes knowledge as Markdown concepts and indexes. In hoimin, [docs/knowledge/](knowledge/index.md) records design contracts, decisions, and audit results. Read relevant concepts and their sources before starting work, and update affected explanations in the same change.

This page owns the knowledge-update procedure. Use the
[catalog's task routes](knowledge/index.md#start-with-your-task) to choose what
to read and the [guide ownership table](knowledge/overview.md#current-guides-and-original-records)
to decide where a correction belongs. Keep operational commands in their guide;
link to that procedure from concepts instead of maintaining a second copy.

## Before starting work

1. Read the [catalog scope and evidence guide](knowledge/overview.md) to distinguish source revisions and evidence levels.
2. Choose relevant design and audit pages from the [catalog index](knowledge/index.md). For target selection, read plan/verify; for shutdown, read resources and cleanup; for persistence and resume, read sessions and reports.
3. Follow sources to the original designs, reports, and implementation. Compare the audited commit, OS, inputs, and model assumptions with the current task.
4. Identify the contracts to change, open questions, and required validation. If no concept covers the topic, investigate the originals and code, then decide whether the result warrants a new concept.

If a design disagrees with current code, record the difference before deciding how to change it. Do not treat historical audit findings as necessarily unfixed or historical test passes as results of the current work.

## Deciding what to update

| Change or finding | OKF action |
| --- | --- |
| CLI, configuration, persistence, compatibility, or exit-code contract changes | Update conditions and evidence in the relevant design concept |
| Analyzer, ranking, resource-control, or state-transition decisions | Add the rationale and scope to the existing concept; add a concept for an independent topic |
| Fixes or test/Lean-model changes that alter the scope of guarantees | Record the fix and verification scope in the relevant contract or audit concept |
| New audit results, unverified conditions, or evidence contradicting an existing explanation | Add or update an audit concept with the target revision and link it from relevant designs |
| Reusable development or operational procedures | Update the canonical guide and link it from a `Playbook` if useful |
| Added, moved, or deleted designs or audit reports | Update source catalogs and referring concepts and indexes |
| Typos, formatting, or internal cleanup that changes neither contracts nor evidence | No OKF update is required; explain why in one sentence in the PR or completion report |

Update existing pages for an existing topic. Add a concept only when future work will reference it independently; temporary progress does not always need an OKF concept. Keep original designs, implementations, and execution results in their established locations and reference them as evidence.

## Creating concepts and recording sources

Where the `create-okf` skill is available, an example request is: `$create-okf Update docs/knowledge with the session compatibility conditions changed in this work, and check related audits and indexes`. Otherwise, use the following rules directly.

This bundle uses OKF v0.2. Each ordinary concept starts with YAML frontmatter containing a nonempty string `type`. Also record `title`, `description`, `status`, and evidence in `sources`. `Contract`, `Decision`, `Audit`, `Playbook`, and `Reference` are repository conventions, not a fixed enumeration in the OKF standard.

New concepts explicitly use `status: draft`. Include the topic and terminology, contract or decision, evidence, limitations and unverified conditions, and changes that require rechecking, as relevant. Write current guides in English with one topic per paragraph. Distinguish designs, static code inspection, and historical execution results. Editing must not turn speculation or unverified conditions into established facts.

Each source has a `resource` and an `id` unique within the page; attach the corresponding `[^id]` footnote to supported claims. Resolve relative paths from the document's location. Implementations, schemas, and result files can also be evidence. Understand and preserve existing custom metadata.

Use these local revision conventions:

- For committed sources, record the commit actually read in `sources[].revision`. Use `working_tree: clean` if the file read matches that revision.
- For uncommitted edits, use `working_tree: modified`; for untracked files, use `working_tree: untracked`. Record the SHA-256 of the content read. For modified sources, `revision` identifies the comparison commit. Never insert a guessed future commit ID.
- `catalog_revision` identifies the baseline used to organize a concept; `audit_revision` identifies the audited commit. Do not mechanically replace historical audit targets with current HEAD.
- Update a source's revision, state, and hash when rereading it and updating the explanation. Do not refresh unrelated historical sources or change metadata for sources you have not read.
- `working_tree` and hashes record the state at the time of reference. Later commits alone do not require changing them. If the current source differs, inspect the recorded revision and decide whether the explanation needs updating.

Useful commands include `git rev-parse HEAD`, `git status --short -- <source-path>`, `git show <reference-commit>:<repository-path>`, and `shasum -a 256 <source-path>`. Compute hashes after editing the source. When sharing a change that references untracked material, include it in the same PR or use an already shared source.

Use `verified` only after checking the content against evidence and recording the reviewer and date. Format checks, prose edits, and copying past successes do not qualify. State what was checked without extending guarantees to untested implementations or operating systems.

## Updating indexes

Link new concepts from relevant indexes so they are reachable from the [entry point](knowledge/index.md). The root `index.md` frontmatter contains only `okf_version: "0.2"`; child `index.md` files and optional `log.md` files have no frontmatter. Indexes contain headings and annotated links; logs list newer entries first.

The [design catalog](knowledge/references/design-documents.md) covers Markdown under `docs/superpowers/specs/`; the [audit and report catalog](knowledge/references/audit-documents.md) covers Markdown under `docs/superpowers/reports/` and `docs/audits/`. When adding originals there, update the tables, sources, and footnotes. Preserve quoted source headings in their original language. Implementation plans under `docs/superpowers/plans/` are outside these catalogs.

## Completion checks

After changing OKF, check structure, references, and content separately. This is a development and review procedure; there is currently no dedicated OKF CI gate.

1. **Structure:** Parse initial YAML frontmatter and check mappings, `type`, and reserved-file structure. Limit this check to Markdown under `docs/knowledge/`.
2. **References:** Check source IDs and footnotes, link targets, source commits or hashes, catalog updates for added/moved/deleted originals, and reachability from the entry point.
3. **Content:** Read claims against evidence, target revisions, OS/input/model conditions, terminology, and paragraphs. Run implementation checks relevant to the change from the [development guide](development.md).

The following minimal YAML and reserved-file check runs from the repository root with `python3` and PyYAML. If needed, install `PyYAML==6.0.3` in an isolated environment. It does not validate links, sources, content, or full OKF conformance.

```sh
python3 - <<'PY'
from pathlib import Path
import yaml

root = Path('docs/knowledge')
pages = sorted(root.rglob('*.md'))
assert pages, 'OKF bundle is missing or empty'
for path in pages:
    text = path.read_text(encoding='utf-8')
    header = text.startswith('---\n')
    if path.name in ('index.md', 'log.md'):
        if path == root / 'index.md':
            assert header, f'{path}: repository version declaration required'
            front, body = text[4:].split('\n---\n', 1)
            meta = yaml.safe_load(front)
            assert meta == {'okf_version': '0.2'}, path
        else:
            assert not header, f'{path}: reserved file must not have frontmatter'
        continue
    assert header, f'{path}: missing frontmatter'
    front, body = text[4:].split('\n---\n', 1)
    meta = yaml.safe_load(front)
    assert isinstance(meta, dict), f'{path}: mapping required'
    assert isinstance(meta.get('type'), str) and meta['type'].strip(), path
print(f'YAML and reserved-file checks passed: {len(pages)} Markdown files')
PY
```

For a reproducible local-reference inventory, run:

```sh
python3 tools/knowledge_references.py > /tmp/knowledge-references.json
.venv/bin/pytest tests/test_knowledge_references.py
```

The inventory parses YAML `sources[].resource` and Markdown inline links/reference
definitions outside fenced code. It reports missing local targets and concepts
unreachable through Markdown from `docs/knowledge/index.md`, and exits nonzero
for those findings. Bundle-root links beginning with `/` resolve from
`docs/knowledge`; ordinary links resolve from their page. External URLs are
listed but are not fetched, and heading fragments are not validated. This is
separate from structure checks and content review. When files move, compare
both reference inventories and read owner descriptions; an existing path alone
does not prove that a source still supports a claim.

For recovered historical evidence, retain `original_resource`, `revision`,
`working_tree`, and `sha256`. Use `recovered_revision` and `recovered_blob` for
independently recovered bytes, and pin `resource` and its footnote to that commit.
If exact recorded bytes cannot be found, link to an explicit unavailable-evidence
record and retain the original metadata with `evidence_status`. See the
[Issue #1068 repair inventory](maintenance/2026-10-22-issue-1068-knowledge-links.md)
for recovered snapshots, unresolved limits, and Git hash rechecks.

In the PR or completion report, list the consulted concepts, updated pages or reason no update was needed, checks run, and unverified conditions. The [PR template](../.github/pull_request_template.md) has fields for this. When recording counts, include their scope and date, and distinguish current checks from historical audit results.
