# Documentation publication implementation and verification

Issue: [#749](https://github.com/HayaoSuzuki/hoimin/issues/749).
Work branch: `docs/749-automatic-pages`.
The approved [design](../superpowers/specs/2026-10-12-issue-749-documentation-pages-design.md)
and [plan](../superpowers/plans/2026-10-12-issue-749-documentation-pages.md)
each contain five self-review passes for their respective stages.

## Implementation self-review (five passes)

1. Content: checked the 14-page allowlist against the approved scope, including
   README and the new publication guide. Research/OKF/audit pages are outside
   search; their links point to existing sources at the build revision.
2. Markdown: parsed inline/reference links after Markdown inline processing,
   leaving literal and fenced code untouched. Reused native strict anchor
   validation and added emitted-HTML checks after independent review demonstrated
   a concrete gap for raw HTML and absolute project-site links.
3. Filesystem: checked root containment before linking excluded sources. Build
   output is restricted to the dedicated repository `site/` folder because
   MkDocs cleans that folder. Temporary sources never rewrite original Markdown.
4. Publication: checked fork PRs, merge groups, non-main manual runs, main pushes
   and main manual retries. Only trusted-main push/manual runs upload a Pages
   artifact and deploy; Pages/OIDC write permissions belong only to deployment.
   The concurrency group covers both build and deploy, shared by main events.
5. Maintenance: kept dependencies locked and actions pinned, used Python 3.14,
   kept regular edits in this repository and committed the operation guide.
   Four exact docs inputs use the dedicated helper-test/build workflow; quality
   and workflow lint remain enabled. Unknown inputs retain conservative CI.

## Verification self-review (five passes)

1. Regression evidence: seven helper tests failed before implementation and now
   pass. The generated CI-classification case failed on unnecessary product
   checks before the narrow exception; helper/selection/workflow tests now pass
   (82 tests), with eight existing full action-pin contracts also passing.
2. Failure behavior: disposable source copies with a nonexistent page raised
   ValueError; a nonexistent cross-page anchor aborted the strict build. A
   subsequent clean build passed in approximately 0.4 seconds. No broken
   Markdown was committed, no external URLs polled, no examples executed.
3. Environment: a fresh Python 3.14 environment with only the frozen docs group
   passed all seven helper tests and built the site. Strict Ruff, ty and workflow
   lint were run; Pages permission comments explain each write capability.
   Existing workflow lint exclusions/Python-shell warnings remain unchanged.
4. Rendering: inspected the landing page, installation/example text and source
   commit; used actual key events to search for `limits` and followed the result
   to the limits section. Repeated navigation under `/hoimin/` and checked the
   usage page's plan/verify, results and limits sections. Literal examples were
   only displayed. Search index/page allowlist and OKF references are checked
   separately from visual inspection.
5. Limits: local dirty builds identify HEAD rather than claiming dirty changes
   have a committed identity; publication builds use clean checkouts. GitHub
   Pages setup and a real deployment remain post-merge checks. The documentation
   PR is held until release fix PR #798 merges, then latest main must be included.
   Disk free space was about 173 GiB; no Cargo builds were necessary.

## Operational decisions and remaining checks

Run the small documentation build for every PR/main event: deleting a referenced
excluded source must be detected, and each main merge should refresh source
identity. Exact documentation helper/config/workflow paths bypass product jobs
because even Python-only selection otherwise builds a Rust wheel. Their tests
run in the documentation workflow with pytest in the docs dependency group.
Default dev Hypothesis is supplied by release fix #798 before this branch's PR.

After the maintainer merges the documentation workflow, enable Pages with Source
GitHub Actions, dispatch Documentation on main, verify the deployed site and
usage/search navigation, then close #749. A successful local artifact does not
establish public availability. Historical release documentation is deferred.

## Independent whole-branch review and fix pass

A fresh reviewer independently rendered all 14 current pages without warnings
and found two important gaps: valid `/hoimin/` URLs were rejected as source-path
escapes, while same-site absolute URLs bypassed validation; raw HTML links also
bypassed the Markdown tree and native link validator. There were no critical or
minor findings. The review set aside the explicitly pending #798 dependency,
post-merge Pages activation and deferred version history; those limits stand.

Seven new regression tests failed before the fix. The fix preserves project-site
URLs and checks generated local HTML href/src targets and anchor IDs using
stdlib HTMLParser, within the output root. It handles decoded paths, directory
indexes, queries, fragments and same-site absolute URLs without fetching remote
content. Literal escaped code remains text. This follows the plan's conditional
HTML-checker step because native validation proved insufficient.

Fix self-review: checked URL origin/base matching; checked output containment;
checked decoded fragments and directory indexes; checked raw HTML and literal
code separately; checked the build invokes emitted validation and still rejects
bad links. The expanded helper/selection/workflow tests pass (89 tests), including
all 14 helper tests. Raw HTML repository references should use Markdown syntax
for source-to-GitHub rewriting; raw HTML URLs address rendered output paths.

## Integration after the release fix

PR #798 merged as `c5b04a9`; local main was fast-forwarded and the documentation
branch rebased onto it without conflicts. Default-dev Hypothesis is now present.
After rebasing, frozen dependency sync, 137 helper/CI-selection/provenance tests,
eight known action-pin contracts, Ruff format/check, ty, workflow lint and the
strict site build with emitted-link validation passed. Source records were
refreshed against the reachable rebased commits after reading their current
content. Pages activation and live verification still require the documentation
workflow's own merge.
