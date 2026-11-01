# Documentation layout implementation plan

**Goal:** Separate user documentation from development procedures and records,
following taloon's audience split and grouping content by category.

**Layout:** `docs/user/{guides,reference,about}` owns usage and public contracts.
`docs/development/` owns guides, CI, releases, dependencies, architecture,
performance tooling and the existing OKF catalog. `docs/development/work/` owns
historical designs, plans, reviews, audits, maintenance and measurements.

**Compatibility:** Keep existing published page URLs through an explicit
source-to-site mapping. Preserve raw results, schema contents, historical revisions
and hashes. Update current links, executable paths and CI selection together.

- [x] Move existing documents and preserve links from their original locations.
- [x] Add audience indexes and a placement policy for future documentation.
- [x] Update site source lookup while retaining established public page URLs.
- [x] Check repository Markdown links, OKF structure, sources and reachability.
- [x] Run the strict site build, Python tests, generated CLI reference check,
      formatting, typing and relevant Rust/schema consumers.

This is a layout change. It does not revise public runtime behavior or treat
historical measurements as new verification results.

## Finding historical paths

| Previous location | Current location |
| --- | --- |
| `docs/usage.md`, `docs/agent-skills.md` | `docs/user/guides/` |
| `docs/cli-reference.md`, `docs/compatibility.md`, `docs/json-schema/` | `docs/user/reference/` (CLI page: `cli.md`) |
| Product principles, support and licensing at the docs root | `docs/user/about/` |
| `docs/development.md`, `docs/documentation.md`, `docs/okf-workflow.md` | `docs/development/guides/` (contributor page: `contributing.md`) |
| `docs/ci.md`, `docs/artifact-smoke.md` | `docs/development/ci/` (CI page: `index.md`) |
| `docs/releases.md`, `docs/pypi-publishing.md` | `docs/development/releases/` (release page: `index.md`) |
| `docs/dependency-policy.md`, `docs/dependency-build-scripts.md` | `docs/development/dependencies/` (`policy.md`, `build-scripts.md`) |
| `docs/architecture/`, `docs/knowledge/` | `docs/development/architecture/`, `docs/development/knowledge/` |
| `docs/superpowers/specs/`, `plans/`, `reports/`, `reviews/` | `docs/development/work/designs/`, `plans/`, `reports/`, `reviews/` |
| `docs/audits/`, `docs/maintenance/`, `docs/reviews/` | `docs/development/work/audits/`, `maintenance/`, `reviews/` |
| Dated files and measurement directories under `docs/performance/` | `docs/development/work/performance/` |
| Performance registry, probes and remaining performance material | `docs/development/performance/` |
| `docs/stylesheets/` | `docs/assets/stylesheets/` |

Original paths in this table identify the pre-migration checkout. They are not
current commands or link targets. Historical source revisions and hashes retain
their original meaning; use these mappings when retrieving that evidence from Git.

## Verification (2026-11-01)

- Python full suite: 2,440 passed, 35 skipped; one pre-existing generated-corpus SyntaxWarning.
- Rust workspace with all features: 10,412 passed, 45 ignored across 196 test/doc-test groups.
- Strict site build and generated CLI reference freshness check: passed.
- Ruff format/lint, ty, rustfmt and workspace Clippy with warnings denied: passed.
- OKF YAML and reserved-file checks: 63 pages. Bundle references and navigation have no missing targets or unreachable pages.
- All 3,358 previously valid local Markdown links remain valid; 784 raw artifacts, schemas and assets are byte-identical to the pre-migration checkout.
- Read-only independent review found no blocking issues in layout, publication mapping or executable consumers.

Repository-wide inspection also found 45 pre-existing unresolved links in historical
records, including unavailable original logs and removed source paths. This move
preserves those records and does not claim that the missing evidence was recovered.
Published guide links and anchors pass the strict site build.

Native execution was on macOS. No new Linux/Windows execution or deployment is
claimed by this layout verification.
