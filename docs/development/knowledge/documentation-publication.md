---
type: Playbook
title: Documentation publication
description: User and development documentation, record placement, stable public URLs,
  publication permissions and recovery.
status: draft
catalog_revision: 2b29a8630ac8411a0545f17fabdf99f6fb848c00
sources:
- id: guide
  resource: ../guides/documentation.md
  revision: 4c499aeecc91b158df121b5cd39d6a367a2f4e45
  working_tree: modified
  sha256: 8db3e8b902a0b240093fb32547ec11ba180806f3e5d97487e4aaf252e4633b78
- id: builder
  resource: ../../../tools/docs_site.py
  revision: 4c499aeecc91b158df121b5cd39d6a367a2f4e45
  working_tree: modified
  sha256: 3a2ba5b18e59e1969d95fa202ec049063850cc73b17ae278723b802b1319da8a
- id: workflow
  resource: ../../../.github/workflows/documentation.yml
  revision: 9eb8ed0d3690d45f77932afcfd457afbdbf43abd
  working_tree: clean
  sha256: e108d501ea258a8e61b1d5512bd45d55d1c09e4184a987516514da2ec31a59dc
- id: tests
  resource: ../../../tests/test_docs_site.py
  revision: 4c499aeecc91b158df121b5cd39d6a367a2f4e45
  working_tree: modified
  sha256: fd468f4596cbcab295cd68d5eda14a0c6fb4eebf0377e6dec4d16c1017f1ef60
- id: design
  resource: ../work/designs/2026-10-12-issue-749-documentation-pages-design.md
  revision: 9eb8ed0d3690d45f77932afcfd457afbdbf43abd
  working_tree: clean
  sha256: 683ffc6122e2372c7495fd02835de0abfd03ab8e7bd72770e6fc17de4807e56b
---

# Publication scope and updates

The [publication guide](../guides/documentation.md) is the canonical procedure. Edit Markdown in its existing hoimin locations, review the PR Documentation check and HTML artifact, then merge to main. The site represents main and displays the build's source commit on every page. On initial activation after merging the workflow, select GitHub Actions as the Pages source. The URL is `https://hayaosuzuki.github.io/hoimin/`; no files need to be copied into the existing user-site repository.[^guide][^design]

Current user and maintainer guides and theme controls use English. Historical designs, audits, and investigation evidence retain their original language. The Material theme matches the user site's indigo palettes, light/dark toggle, and hidden secondary sidebar. Only the named stylesheet is copied alongside pages; adjacent assets remain unpublished. The [agent skill guide](../../user/guides/agent-skills.md) documents standard `skills/` packaging and GitHub CLI installation.[^guide][^builder]

# Audience and record placement

[User documentation](../../user/index.md) owns workflows and public reference
material. The [development entry point](../index.md) separates contributor guides,
CI, releases, dependency policy, architecture, performance and this knowledge
catalog. Original designs, plans, reviews, audits and measurements live under
[development records](../work/index.md). New task-specific records belong together
in `docs/development/work/<issue-or-topic>/`; reusable procedures stay in their
owning guide.[^guide]

The builder maps reorganized repository sources to established public page paths.
Moving a source therefore preserves its published URL. The rendering plugin maps
each staged page back to its repository location before resolving relative links.
The new development overview uses a separate publication path because the original
contributor guide already occupies `/docs/development/`.[^builder][^tests]

# Permissions and validation

All PRs, merge groups, main pushes, and manual runs perform a lightweight build. Builds have only contents: read. Pages artifact creation and deployment are limited to main pushes or main manual runs in the canonical repository. Only the deployment job receives pages: write and id-token: write. Main builds and deployment share a concurrency group, cancelling obsolete runs.[^workflow]

Published pages are copied from an explicit allowlist to a temporary directory. Research, audit, and OKF pages are outside site search. Markdown references to excluded material are checked for existence and rewritten to commit-pinned GitHub URLs. Missing files cannot be hidden by URL rewriting, and internal anchors are checked with MkDocs strict validation. Generated HTML validation also rejects missing pages and anchors in same-site absolute URLs and HTML href/src attributes. HTML URLs address rendered output paths, so use Markdown links for source references. Code examples and external URLs are not executed or fetched.[^builder][^tests]

# Failures and verification scope

Fix Markdown links and anchors after build failures. Retry publication with a manual Documentation run on main. Local build success, Pages settings, environment approval, and actual publication must be checked separately. After deployment or a settings change, check the landing page, search, plan/verify, results, and resource-limit navigation. The guide describes recovery for the configured Pages site; settings restoration is not a prerequisite for ordinary readers. Historical release-version switching is outside the main-only scope.[^guide][^design]

Recheck links, permissions, and presentation when changing the allowlist, URL rewriting, Markdown extensions, deployment conditions, or concurrency. Source existence checks use repository documents; no arbitrary external-input ingestion feature is added.[^builder][^workflow]

[^guide]: [Publication procedure](../guides/documentation.md).
[^builder]: [Site builder](../../../tools/docs_site.py).
[^workflow]: [Documentation workflow](../../../.github/workflows/documentation.yml).
[^tests]: [Link rewriting regressions](../../../tests/test_docs_site.py).
[^design]: [Approved design](../work/designs/2026-10-12-issue-749-documentation-pages-design.md).
