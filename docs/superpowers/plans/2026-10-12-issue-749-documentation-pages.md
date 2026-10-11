# Automatic documentation publication implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Publish searchable main documentation when a documentation PR is merged.

**Architecture:** Prepare an allowlisted temporary source tree from existing
Markdown, build it with strict MkDocs validation, and deploy only trusted main
artifacts through GitHub Pages. Keep all regular work in this repository.

**Tech Stack:** Python 3.14, MkDocs, uv lockfile, GitHub Actions/Pages.

**Spec:** [Approved design](../specs/2026-10-12-issue-749-documentation-pages-design.md).

## Global constraints

- Site URL: `https://hayaosuzuki.github.io/hoimin/`.
- Main documentation only; show the exact source commit on every page.
- Preserve existing Markdown locations and GitHub links.
- Explicit source allowlist from the spec; no searchable research/OKF/audit pages.
- No cross-repository publishing token, code-example execution or remote ingestion.
- Python runners use 3.14; action SHAs and documentation dependencies are locked.
- Build jobs: `contents: read`; deployment-only `pages: write`, `id-token: write`.
- At least five self-reviews for design, planning, implementation and verification.
- No product binary/version change; do not run Cargo builds for documentation.

## Review focus

1. A link to an excluded but existing source must remain accessible on GitHub;
   a nonexistent source must fail, rather than be hidden by URL rewriting.
2. Broken same-page and cross-page fragments must fail the strict build.
3. Project-site `/hoimin/` URLs, encoded paths and query strings must resolve
   correctly; unrelated external URLs are not fetched.
4. Markdown code examples must survive source preparation without link rewriting
   inside literal code; existing GitHub README links must remain usable.
5. PR/manual non-main events cannot deploy, and overlapping main runs cannot
   publish older content after newer content.

## Task 1: Build an allowlisted searchable site

**Files:** create `mkdocs.yml`, `tools/docs_site.py`, `tests/test_docs_site.py`;
modify `pyproject.toml`, `uv.lock`, `.gitignore`.

**Interfaces:**
- `PUBLIC_PAGES: tuple[str, ...]` contains the spec's exact repository paths,
  with README rendered as `index.md` and other pages retaining `docs/` paths.
- `prepare_sources(root: Path, destination: Path, revision: str) -> None` copies
  only selected pages, adapts source-checked links and adds a commit notice.
- `build_site(root: Path, output: Path) -> None` reads Git HEAD, prepares sources
  in a `TemporaryDirectory`, calls MkDocs strict build and leaves only HTML in
  output. An existing output must never cause deletion outside that output.
- CLI: `uv run --frozen --only-group docs --no-project python -m tools.docs_site
  --output site`. If uv's group-only invocation requires synchronization first,
  document and use `uv sync --frozen --only-group docs --no-install-project`
  followed by `uv run --frozen --no-sync python -m tools.docs_site --output site`.

- [x] Inspect MkDocs's installed validation behavior and pin its stable version
  in a separate `docs` dependency group. Resolve the lock with uv.
- [x] Write focused AAA tests before custom source-preparation logic: existing
  excluded source becomes a revision-pinned repository link; a missing source
  raises an error; published-page links stay local; literal code is preserved.
  Use separate tests for errors, without conditional expected-result logic.
- [x] Run the focused tests and confirm they fail because the helper is absent.
- [x] Implement source preparation with the smallest supported Markdown-aware
  mechanism. Inspect actual document link forms first; avoid a new generic
  Markdown parser or command DSL. Resolve relative paths against original source
  locations and verify containment/existence before making repository links.
- [x] Configure built-in MkDocs search and navigation for every allowlisted page.
  Enable `validation.links.not_found`, `unrecognized_links`, `absolute_links`
  and `anchors` diagnostics as appropriate to enforce local link correctness
  under `--strict`. Enable footnotes/fenced-code/table extensions used by sources.
- [x] Prefer native MkDocs validation. Add an HTML link checker only if negative
  build probes establish a concrete missing guarantee; use stdlib HTMLParser,
  URL parsing and a bounded local-file resolver with focused tests for that gap.
- [x] Run focused tests, strict site build, Ruff and ty; inspect the search index
  for exclusion of research pages and the HTML for correct revision links.
- [x] Commit the build helper, configuration, dependencies and tests together.

## Task 2: Validate PRs and publish main automatically

**Files:** create `.github/workflows/documentation.yml`; modify CI selection
and its existing tests only if the new documentation-only paths unnecessarily
trigger Rust, formal or distribution jobs.

**Interfaces:** consume Task 1's single build command and `site/` output;
produce a review artifact for PRs and a Pages artifact for eligible main runs.

- [x] Read the existing workflow lint and CI path classification contracts.
  Resolve official Pages/upload action SHAs against upstream releases.
- [x] Define PR and main push path filters covering published Markdown, helper,
  config, dependency files and workflow. Include merge-group validation and
  manual dispatch. Do not accept an arbitrary ref/command input.
- [x] Build on Ubuntu with Python 3.14, frozen docs dependencies and read-only
  permissions. Checkout with credentials disabled. Upload `site/` for PR review.
- [x] Add deployment gated on a successful build, the canonical repository,
  main ref, and push/manual event. Scope write/OIDC permissions to deployment
  and use the `github-pages` environment. Upload Pages artifacts only on trusted
  eligible runs. Keep PR artifacts out of the deployment path.
- [x] Serialize the whole trusted publication pipeline, cancelling obsolete
  runs so an older build cannot queue a later deployment. Give PR runs distinct
  concurrency groups. Set finite job timeouts.
- [x] If CI selection needs narrow docs-only exceptions, add behavioral tests
  before those exceptions; preserve conservative fallback for unknown paths.
- [x] Run strict workflow lint and relevant existing CI-selection tests. Review
  fork PR, merge group, main push and non-main manual event conditions explicitly.
- [x] Commit the workflow and any necessary CI classification changes.

## Task 3: Explain operation and verify failure behavior

**Files:** create `docs/documentation.md`,
`docs/knowledge/documentation-publication.md`,
`docs/reviews/2026-10-12-issue-749-documentation-pages.md`;
modify `README.md`, `docs/development.md`, `docs/knowledge/index.md`,
`docs/knowledge/references/design-documents.md`.

**Interfaces:** document Task 1's build command and Task 2's deployment triggers;
expose the site URL while distinguishing pending activation from a live site.

- [x] Write the short regular procedure: edit Markdown, open PR, review checks
  and artifact, merge. Document optional local build and manual-main retry.
- [x] Document one-time Pages Source=GitHub Actions setup after merge, expected
  `github-pages` environment behavior, commit identity and main-only scope.
- [x] Link the site and publication guide from README/development; retain GitHub
  reference URLs. Add the reusable OKF Playbook, source provenance and indexes.
  Include the design in the design-document catalog with source metadata.
- [x] Create disposable copies with one broken page link, then one broken anchor;
  verify each build fails. Restore/discard the copies and obtain a clean build.
  Never commit intentionally broken content or deploy experimental artifacts.
- [x] Inspect representative landing, usage/resource/plan sections and search
  in a local browser. Do not execute the displayed mutation commands.
- [x] Record five implementation and five verification self-reviews with actual
  command results, plus outstanding post-merge activation checks. Validate
  OKF provenance/links, `git diff --check`, strict build and all affected linters.
- [x] Commit the guide and local review evidence on the work branch.
- [ ] After release fix PR #798 merges, include latest main, repeat affected checks,
  push the work branch and create/attach the documentation PR. Explain deferred
  versioned documentation and post-merge activation.
- [ ] After the maintainer merges, enable Pages, verify successful deployment and
  the live site's installation/plan/results navigation; only then close #749.

## Planning self-review (five passes)

1. Spec coverage: all selected documents, publication identity and operational
   recovery have owners; live deployment is explicitly post-merge.
2. Failure coverage: absent files, absent fragments, excluded sources and literal
   code each have a focused probe/test; no blanket external-link polling is added.
3. Interface consistency: one helper command owns the temporary sources and
   site output; workflow and guide consume the same command.
4. Permission review: PRs cannot reach a deploy job or Pages upload; publication
   conditions include repository/ref/event and the full pipeline is serialized.
5. Proportion/cost: native strict validation is preferred over custom checking,
   build dependencies are separated, and unrelated Rust/formal/release checks
   are avoided only where narrow existing CI rules permit it.
