# Automatic documentation publication (#749)

## Approved direction

The maintainer approved a single documentation site on 2026-10-12:
`https://hayaosuzuki.github.io/hoimin/`. Documentation stays in this repository.
Merging a documentation PR into main publishes it automatically, without copying
files into HayaoSuzuki.github.io or running a local deployment command.
The initial site represents main and displays its exact source commit; release
versions and historical version switching are deferred. This supersedes that
part of the original Issue's acceptance criteria.

## Site and content

Use MkDocs with built-in static search. Pin and lock the documentation dependency
group independently of the product build. Python runners use 3.14. Reuse existing
Markdown without moving its GitHub paths. Product binaries and package versions
are unchanged.

Explicitly select README as the landing page and these source documents:
usage, cli-reference, compatibility, support, licensing, development, releases,
pypi-publishing, dependency-policy, dependency-build-scripts, artifact-smoke and
ci, all from `docs/`. Add `docs/documentation.md` for publication instructions.
Group navigation into Getting started, Usage/reference, and Maintainer development.
Link directly to usage sections for plan/verify, results and OS/resource limits.
The landing page preserves installation and the short mutation command.

Do not publish knowledge catalogs, research reports, audits, plans or this spec
as searchable site pages. Referenced source files, excluded documents and assets
must either be deliberately included as static downloads or link to their exact
GitHub source revision. Never silently turn a missing local page into a repository
link: its source target must exist. Existing GitHub document URLs remain valid.

Build a temporary site source tree from that allowlist; do not modify source
Markdown in place. A small repository-owned build helper may prepare the tree,
adapt repository links, expose revision metadata and inspect generated HTML.
Do not execute document code examples or fetch external content during the build.

## Publication and permissions

Create a dedicated documentation workflow. PRs (including forks), merge groups,
main pushes and manual dispatch can validate/build the site. Skip unrelated
push/PR changes when safe; include the docs sources, helpers, configuration,
dependency lock and workflow in relevant paths. Use pinned action SHAs and
`persist-credentials: false` for checkout.

Build jobs have `contents: read` only. Upload a rendered site artifact for review.
Only a successful build for a main push or manual main dispatch may deploy.
The deployment job uses the protected `github-pages` environment, with
`pages: write` and `id-token: write` scoped to that job. Never use
`pull_request_target` or deploy artifacts from PR runs. Serialize deployments;
an older build must not overwrite a newer deployment due to concurrent execution.

Use GitHub's Pages artifact and deployment actions. Initial hosting setup selects
GitHub Actions as the hoimin repository's Pages source. Do this after the workflow
is merged. Existing HayaoSuzuki.github.io hosting is unaffected. No PAT or
cross-repository credential is needed. A failed build leaves the published site
unchanged. Manual dispatch on main republishes its current content.

## Verification

Run `mkdocs build --strict`; elevate missing page, missing file and unresolved
anchor diagnostics to errors. Independently inspect generated HTML local links
and fragment IDs if MkDocs cannot fully enforce them. Respect the `/hoimin/`
base path, URL decoding, directory indexes, query strings and fragments.
Do not routinely check external URL availability.

Demonstrate both a deliberately broken internal page link and a broken anchor
failing validation, then restore the inputs and obtain a successful build.
Check the rendered landing page, navigation, search index and source revision.
Use representative browser checks rather than screenshots or E2E of every page.
Apply strict Python and workflow lint to helpers/configuration. Add focused tests
only for meaningful link resolution failures if a custom checker is necessary;
do not create configuration snapshot tests or large product test runs for docs.
Any tests follow AAA, avoid conditional cases and prefer properties where useful.

## Reviews and completion

Record at least five self-reviews for each of design, implementation planning,
implementation and verification. Keep documentation, review evidence and workflow
changes committed on `docs/749-automatic-pages`, based on latest main.
Monitor disk space; Cargo builds are unnecessary for this change. Clean build
artifacts when needed without interfering with active builds.

The implementation PR is reviewable after local build/link/lint checks pass.
Public activation and actual deployment are verified after merge; do not claim
the live site exists merely because its artifact builds. Close #749 only after
the site is deployed and the basic usage navigation works.

## Design self-review (five passes)

1. Operational simplicity: main merge is the regular publication action; no
   separate content repository, local deploy, token or version selector is required.
2. Content scope: an explicit allowlist preserves source locations and keeps
   research/knowledge catalogs out of static search; excluded references retain
   source-checked GitHub links.
3. Security: PR execution has read-only credentials; only trusted main runs can
   obtain the deployment permissions, and no examples or remote input are executed.
4. Correctness: strict build plus generated-link checks must detect broken pages
   and anchors, including project-site paths; commit identity avoids claiming the
   current site is a versioned release manual.
5. Cost and recovery: documentation-only validation avoids Rust builds, exhaustive
   browser tests and external link polling; failed builds preserve the live site
   and manual main dispatch provides a documented retry path.
