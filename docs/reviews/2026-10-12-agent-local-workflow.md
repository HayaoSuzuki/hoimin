# Local AI-agent workflow documentation review

Base: `5613b3cd4c694cf0023b510f7d2bdfa54962ce65` (latest main at the start).
Branch: `docs/agent-first-local-workflow`. The maintainer requested documenting
the preceding discussion, including the local agent workflow, limits of kill
ratios, resource protection, optional CI use, and separate fuzz-tool research.
Current guides and skills remain in English.

## Design self-review: five passes

1. Purpose: centered local test improvement by an external agent; did not imply
   an embedded LLM or require humans to inspect every survivor.
2. Roles: retained human specification decisions and change review, without
   treating the current implementation as the specification.
3. Verdicts: separated useful test gaps, permitted changes, suspected equivalence,
   unresolved assessments, and actual execution outcomes.
4. Resources: made limits and cleanup core requirements while preserving per-root
   memory scope, analyzer exclusions, backend differences, and cooperative disk guards.
5. Boundaries: kept CI optional for product users, retained development CI, and
   separated independent fuzz-tool research from hoimin changes.

## Implementation-plan self-review: five passes

1. Entry points: selected README, usage, agent guide, development, and CI guidance
   so readers encounter the same direction through each supported workflow.
2. Canonical source: chose one public principles page rather than repeating the
   full rationale in every guide; added navigation and publication allowlist entries.
3. Distribution: updated the two canonical `skills/` packages independently;
   created no local `.agents` or `.claude` mirrors.
4. Maintenance: added one draft OKF decision with source hash and an entry-index
   link; preserved historical designs and audit evidence.
5. Validation cost: selected existing skill/site/reference tests, strict site build,
   lint, packaging dry-run, and focused OKF checks; planned no Rust build, mutation
   run, new benchmark, or redundant content tests for this documentation change.

## Implementation self-review: five passes

1. Cross-page consistency: checked purpose, score limitations, optional CI, and
   resource-stop handling across public guides and distribution skills.
2. Survivor loop: corrected the skill's assumption that every survivor identifies
   a contract violation; require investigation before changing a test.
3. Improvement evidence: require success on original code and intended behavioral
   failure on the mutant; a generic killed result alone is insufficient.
4. Operational invariants: preserved plan identity, native test argv, fresh
   baselines, disk reserves, cleanup ownership, and candidate-set comparisons.
5. Scope: changed only documentation, site page registration, and two existing
   skill-contract text expectations. No CLI, schema, defaults, dependency, or version change.

## Validation self-review: five passes

1. Skill/reference tests: existing tests cover frontmatter, standalone references,
   workflow blocks, safety instructions, and ranked-plan documentation.
2. Site publication: strict build checks the allowlisted page, navigation, rewritten
   source references, and internal HTML links/anchors.
3. Code hygiene: Ruff lint and format checks cover the changed Python files;
   `git diff --check` checks whitespace.
4. Knowledge provenance: check YAML, source IDs and footnotes, source hash, relative
   links, and entry-index reachability for the new decision.
5. Evidence limits: this work checks instructions and publication, not resource
   enforcement, LLM correctness, equivalent-mutant detection, or mutation performance.

## Execution evidence

- Python 3.14.6. Existing skill/site/ranked-plan tests: 61 passed.
- Strict documentation build and generated HTML link checks passed.
- Ruff lint and format checks passed for `tools/docs_site.py` and `tests/test_skills.py`.
- GitHub CLI `gh skill publish --dry-run` passed without publishing. It emitted
  repository-setting advice about secret scanning and tag protection; no settings
  were changed.
- OKF YAML/reserved-file structure passed for all 45 catalog pages. The new
  decision's source hash, IDs, footnotes, local links, and entry-index reachability passed.
- The first sandboxed test/build attempts encountered temporary-directory access
  errors; a normal-permission retry passed. No product fix was needed for that failure.
- At the start, C: had about 175 GiB free. No Rust build or new dependency install
  was performed, so no Cargo artifacts were added and no `cargo clean` was needed.

Consulted OKF pages: overview, runtime lifecycle, selection/plan/verify, and
survivor review. Updated: product direction and entry index. Existing historical
contract pages remain unchanged because implementation contracts did not change.

## Independent review

An independent reviewer inspected the complete diff and all three new documents,
checked safety invariants, score claims, source hash, referenced anchors, and
whitespace, and reported no findings requiring changes. A final editorial pass
uses "reported saturation" to avoid implying statistical inference by the progress
command. No runtime enforcement or LLM behavior was claimed as tested.

## Follow-up: remove pre-activation wording from current guides

The maintainer identified the README's "after its initial Pages activation" as
unfinished reader guidance. A follow-up review covered all public allowlisted
pages and the distribution setup references, distinguishing stale setup prose
from legitimate historical evidence, examples, and recovery procedures.

Read-only checks on 2026-10-12 confirmed:

- The live documentation landing page returned HTTP 200 and still contained the
  old activation wording. GitHub Pages uses the workflow build type and configured
  `https://hayaosuzuki.github.io/hoimin/` URL.
- Discussions is enabled, with `bug-reports` and `q-a` categories.
- The latest release was `v0.4.6`; its tree contains both distribution skills.
- The repository variable `PYPI_AUTO_PUBLISH` is `true`. The existing hosted
  verification record separately documents the completed publisher bootstrap.

These checks did not submit discussions, change settings, publish packages,
exercise account authentication, or test search interactions. An exploratory
Pages-deployments API request returned 404; deployment status was not inferred
from it. The HTTP page and supported repository APIs supply the observations above.

### Follow-up self-reviews

- Design, five passes: current reader needs; confirmed service availability;
  user/maintainer separation; historical evidence limits; version-neutral policy.
- Plan, five passes: audit public allowlist; inspect setup references; verify
  external settings read-only; preserve restoration procedures; refresh only
  reread OKF sources.
- Implementation, five passes: remove Pages prerequisites; expose active
  Discussions; explain tagged skill availability; distinguish enabled PyPI from
  replacement-publisher bootstrap; remove stale 0.3-series and tag-trigger claims.
- Validation, five passes: strict site links/anchors; existing skill/reference
  tests; OKF structure and changed source hashes; whitespace and stale-phrase
  scan; independent review of the follow-up diff.

Discussion maintenance moved from the user reporting page to the development
guide. Historical release examples and unmeasured local/hosted timing remain
explicitly qualified. The plan/verify guide now explains report-directory setup
and the initial versus post-improvement report sequence.

Follow-up verification passed: 61 existing focused tests, strict site build and
HTML link/anchor checks, 45-page OKF structure, nine refreshed source hashes and
their footnotes, and whitespace checks. The independent reviewer found the old
"improve tests, then run" comment contradicted the new initial-verification
instructions. It was corrected to verify first and retain a distinct
`batch-a-002.json` report after test improvements; the reviewer reread the change
and confirmed no outstanding findings. No repository setting or published site
was changed by these checks; the revised content will publish after merge.
