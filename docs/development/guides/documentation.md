# Documentation publication

The documentation site is <https://hayaosuzuki.github.io/hoimin/>. It represents
main, with the source commit shown on each page, rather than a specific release.
The site is published through GitHub Pages by the Documentation workflow.

Current user and maintainer guides, navigation, and theme controls use English.
Historical design, audit, and investigation records retain their original language.
The site uses the same Material theme, indigo palettes, light/dark toggle, and
hidden secondary sidebar as the [user site](https://hayaosuzuki.github.io/).
Search, code-copy controls, and navigation labels remain in English.

## Where documents belong

Start at the [documentation index](../../index.md). Keep user workflows and
public references separate from contributor procedures and development records.

| Category | Location | Content |
| --- | --- | --- |
| User guides | `docs/user/guides/` | Installation, workflows, configuration and agent integration |
| Public reference | `docs/user/reference/` | Generated CLI help, compatibility and JSON schemas |
| Product and support | `docs/user/about/` | Product principles, licensing and reporting problems |
| Contributor guides | `docs/development/guides/` | Setup, checks, test conventions, documentation and OKF maintenance |
| CI, releases and dependencies | `docs/development/ci/`, `releases/`, `dependencies/` | Maintained development procedures, grouped by responsibility |
| Architecture and performance | `docs/development/architecture/`, `performance/` | Maintained views, performance registry and executable probes |
| Knowledge | `docs/development/knowledge/` | Existing OKF contracts, decisions and evidence summaries |
| Development records | `docs/development/work/<issue-or-topic>/` | A task's design, plan, reviews and durable evidence |
| Site assets | `docs/assets/` | Explicitly published styles and other site assets |

Update the owning page for an existing topic. Add a category directory when its
content needs a separate entry point, and link it from the audience index. Keep
current procedures in guides; use the knowledge catalog to explain decisions and
link to evidence. Historical category directories under `work/` retain earlier
records. Preserve their scope, revisions and raw measurement contents when moving
them; paths inside raw outputs remain evidence of the original execution.

When moving a file, update Markdown links, OKF source resources, generated-file
checks, schema consumers, CI selection and tool defaults that address it. Preserve
recorded source revisions and hashes: a path-only reorganization does not verify
historical claims again.

Published URLs are independent of repository locations. `PUBLICATION_PATHS` in
`tools/docs_site.py` maps reorganized sources to established site paths, and
`mkdocs.yml` addresses those staged publication paths. Keep existing mappings when
moving a published source. New pages without a legacy path use their source path.
The rendering plugin resolves each staged page back to its repository source so
relative links still resolve from the source's actual directory.

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
navigation. Update both when adding a page. The helper also copies the explicitly
named `docs/assets/stylesheets/extra.css`; adjacent files are not published automatically.
Research, audit and knowledge pages
remain on GitHub, outside site search. Links to excluded sources are checked for
existence and point to the build's source commit on GitHub. External URL
availability is not checked during builds.

Generated HTML also checks local link targets and fragments, including raw HTML
and absolute URLs within this project site. Raw HTML links must address rendered
output paths. Use Markdown links for repository source references so the helper
can rewrite them to revision-pinned GitHub URLs.

## Publication recovery

Run **Documentation → Run workflow → main** to retry a failed deployment.
PRs and non-main manual runs can validate but cannot publish. A failed build does
not replace the published site. Publication runs cancel obsolete runs to prevent
an older build overwriting a newer one.

If repository settings need to be restored, select **Settings → Pages → Build
and deployment → Source → GitHub Actions**. Use the `github-pages` environment
and allow main deployments; any configured environment approval must be completed
before publication. The existing HayaoSuzuki.github.io site is managed independently
and requires no changes.

After a deployment or settings change, check the live landing page, search, usage and resource-limit
sections. For a content problem, correct the Markdown through a PR and merge it.
Historical version switching and rebuilding old release tags are not part of
this main-only site.
