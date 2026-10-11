# Documentation publication

The documentation site is <https://hayaosuzuki.github.io/hoimin/>. It represents
main, with the source commit shown on each page, rather than a specific release.
The site becomes available after the initial workflow merge and Pages activation.

## Regular updates

Edit the existing Markdown in this repository and open a PR. The documentation
check builds the site and checks internal links and anchors. Download the
`documentation-site` artifact from the workflow to review its HTML. Merge the PR
after checks pass; the main workflow publishes automatically. No local deploy
command, separate content repository or publishing token is required.

The lightweight build runs on every PR, merge group and main push. This also
detects removal of source files referenced by the documentation and keeps the
published source commit current. PRs cannot publish, including fork PRs.

## Local build

Use Python 3.14 and uv from the repository root:

```console
uv sync --frozen --only-group docs --no-install-project
uv run --frozen --no-sync python -m tools.docs_site --output site
```

The helper uses a temporary source tree and builds with `mkdocs build --strict`
equivalent settings. It replaces the dedicated `site/` output on each build;
never store hand-written files there. Serve the output with any local static
server to preview it, for example `python -m http.server --directory site 8000`
and open <http://localhost:8000/>. Existing source Markdown is not rewritten.

`tools/docs_site.py` defines the publication allowlist and `mkdocs.yml` defines
navigation. Update both when adding a page. Research, audit and knowledge pages
remain on GitHub, outside site search. Links to excluded sources are checked for
existence and point to the build's source commit on GitHub. External URL
availability is not checked during builds.

Generated HTML also checks local link targets and fragments, including raw HTML
and absolute URLs within this project site. Raw HTML links must address rendered
output paths. Use Markdown links for repository source references so the helper
can rewrite them to revision-pinned GitHub URLs.

## Initial activation and recovery

After the workflow is merged, select **Settings → Pages → Build and deployment →
Source → GitHub Actions** in the hoimin repository. Use the `github-pages`
environment and allow main deployments; any configured environment approval must
be completed before publication. The existing HayaoSuzuki.github.io site is
managed independently and requires no changes.

Run **Documentation → Run workflow → main** to publish initially or retry a
failed deployment. PRs and non-main manual runs can validate but cannot publish.
A failed build does not replace the published site. Publication runs cancel
obsolete runs to prevent an older build overwriting a newer one.

After activation, check the live landing page, search, usage and resource-limit
sections. For a content problem, correct the Markdown through a PR and merge it.
Historical version switching and rebuilding old release tags are not part of
this initial main-only site.
