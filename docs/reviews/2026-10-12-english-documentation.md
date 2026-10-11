# English documentation and site theme

## Scope and design

The user approved English for the published site and current maintainer guides,
while retaining historical design, audit, and investigation records in their
original language. Operational concepts at the knowledge catalog root and its
entry index are included; historical design/audit concepts and source catalogs
are excluded. Source quotations and anchor URLs into historical records retain
their original text.

The user also approved matching the existing user site's Material theme,
indigo palettes, light/dark toggle, and hidden secondary sidebar, with English UI,
search, code copying, and page navigation. Reference configuration:
[user site at 9f78227](https://github.com/HayaoSuzuki/HayaoSuzuki.github.io/blob/9f78227a3b0e4c4d2303461f8fe266f9a091a637/mkdocs.yml).
The matching [CSS](https://github.com/HayaoSuzuki/HayaoSuzuki.github.io/blob/9f78227a3b0e4c4d2303461f8fe266f9a091a637/docs/stylesheets/extra.css)
only hides the secondary sidebar. Keep hoimin's explicit navigation and publication
allowlist; the user site's content hooks, Twitter, and MathJax are not needed.

### Design self-reviews

1. Checked publication scope against `PUBLIC_PAGES`: the CI guide contains Japanese;
   other published prose is English. Preserve historical fragment URLs.
2. Checked the user's scope reply: translate current operational guides and entry
   labels; retain historical reports, source catalogs, and research evidence.
3. Checked commands, numeric measurements, version floors, permissions, and
   unverified conditions: translation must retain these distinctions.
4. Compared the existing site's theme and CSS: match Material, palettes, toggle
   icons, navigation/search features, and sidebar presentation; use English labels.
5. Checked impact: documentation dependencies and helper assets change, but CLI,
   wheel contents, schemas, runtime behavior, and release floor do not.

## Implementation and validation plan

Translate public CI prose, the OKF workflow, operational knowledge concepts, and
catalog entry labels. Preserve source metadata unless a source is actually reread
and its claims updated. Pin Material in the docs group and regenerate the lock.
Copy only the named stylesheet into the temporary source tree. Validate with the
existing docs tests, strict site build, local link/English scans, Python quality
checks, and a browser preview of search, navigation, themes, and code copy.

### Plan self-reviews

1. Enumerated published pages and current root-level maintainer guides; no
   broad translation of historical evidence is necessary.
2. Identified heading changes as a link risk: inspect incoming fragment links
   and preserve compatibility anchors where needed.
3. Kept original source revisions, hashes, footnote IDs, and quoted evidence
   distinct from newly translated explanations.
4. Chose one real filesystem regression for allowlisted CSS copying: an omitted
   stylesheet must fail, and an adjacent unlisted file must remain unpublished.
5. Selected strict build, scoped tests, quality checks, and browser checks rather
   than Rust builds or a new large E2E suite. Monitor disk space; no Cargo build
   outputs are required for this task.

## Results

The user clarified that the agent directories were distribution copies, not
local installation mirrors. Canonical packages now live exclusively under
`skills/`; `.agents/skills` and `.claude/skills` are ignored installation paths.
Each skill carries its own setup reference and Elastic License 2.0 text.
Existing product release tags remain the distribution mechanism; no separate
skill release workflow or synchronization is introduced.

### Implementation self-reviews

1. Compared translated commands, fenced examples, link destinations, version
   floors, and measurements against the original documents. Retained historical
   sources and fragment URLs; translated current operational explanations.
2. Checked the Material configuration against the reference site, including
   English search, palette toggles, navigation, and the hidden secondary sidebar.
   Only the explicitly named stylesheet enters the publication source tree.
3. Reviewed both independently installable skills: fixed the improvement skill's
   sibling setup link, bundled the same license, retained all disk, baseline,
   cleanup, candidate identity, and native-command safeguards.
4. Reviewed packaging and release instructions against the actual GitHub CLI:
   local installation copies files and adds provenance metadata only to installed
   copies. Dry-run validation creates no release; product tags retain ownership.
5. Checked the final diff for unrelated runtime changes, stale mirror references,
   publication allowlist changes, dependency pinning, and source-hash drift.
   Source metadata was refreshed only for reread and updated publication sources.

### Testing self-reviews

1. Observed the CSS-copy regression fail before implementation and pass afterward;
   it verifies copied bytes and exclusion of adjacent unlisted assets.
2. Observed missing canonical skill paths and missing bundled licenses fail before
   relocation/packaging, then pass. Existing workflow and safety contracts remain
   checked against the canonical packages rather than duplicate mirrors.
3. Validated both skill frontmatters with the skill-creator validator, installed
   each independently using GitHub CLI 2.102.0, and ran publish dry-run. A separate
   read-only packaging probe confirmed the formerly broken sibling reference is
   now self-contained. Repository security-setting recommendations are advisory;
   no external settings were changed.
4. Built the site strictly, checked all 15 published pages for English prose, and
   verified publication-source hashes. Browser inspection confirmed indigo, dark
   mode, hidden secondary sidebar, English search, navigation to the skill guide,
   and the copy button's English feedback. Clipboard bytes were not verified.
5. Ran focused Python tests and static checks, then the full Python suite. The
   initial suite had 960 passes, 32 skips, and one existing Lean-workflow harness
   failure (empty output, exit 2304); its isolated rerun passed in 73.70 seconds.
   A full rerun and final environment-complete quality results are recorded below.

No Rust source or runtime behavior changes. No Cargo builds were needed; available
disk space was approximately 190 GB during validation.

### Final verification

- Focused documentation and skill tests: 51 passed.
- Strict site build, scoped Ruff/type checks, repository Ruff checks, skill
  validators, and GitHub CLI publish dry-run passed.
- Full Python suite rerun: 968 passed, 25 skipped in 279.23 seconds. The optional
  fuzz dependencies were installed during this rerun before the optional checks
  ran, reducing skips relative to the first run; no failure recurred.
- Repository Ruff format/check and `ty check` passed with the fuzz dependency
  group installed. Workflow lint passed with the workflow dependency group and
  its executable directory on PATH; existing Python-shell analysis limitations
  were reported, with no findings.
- Fresh strict build and focused 51 tests passed after rerunning outside the
  Windows sandbox's temporary-directory access restrictions. The restricted
  attempts failed with permission errors, not application assertions.
- Changed Markdown frontmatter/local link targets and independent skill
  references passed a one-off check. Independent final diff review had no findings.
- After pushing commit `402167021a94418e44463b5a8ff49bf1a1a153d4`, GitHub CLI
  successfully previewed that revision and installed both skills independently
  from GitHub with that commit pin into separate temporary directories. Each
  installation contained `SKILL.md`, `installation.md`, and `LICENSE`.
