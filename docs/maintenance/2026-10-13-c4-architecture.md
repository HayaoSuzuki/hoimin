# C4 architecture documentation review

Inspected source revision: `4e9f8ab7161cece688eecd6e50a0f16fbed3caec`.
Artifact: [hoimin through the C4 model](../architecture/c4-model.md).
Scope: documentation of the existing implementation, with Mermaid sources kept
inside Markdown. No implementation, dependency manifest, test or model changes.

## Design self-review

1. Checked C4's official tooling, diagram, notation and container guidance.
   Used system, container, component and selected code views at distinct scales.
2. Kept the Rust crates inside one executable container; Tokio tasks and blocking
   threads are in-process work rather than separately deployed services.
3. Represented the caller's test toolchain and Git as external software. The
   external coding agent is a caller; the Rust analyzer uses Ruff in-process.
4. Distinguished the original checkout, managed execution/delivery storage,
   exchange artifacts, optional session database and separate review database.
5. Limited the sequence to a successful run and documented deadline, writer,
   cleanup, resource-control and model-evidence limitations separately.

## Plan self-review

1. Chose `docs/architecture/c4-model.md` for the reusable architecture view and
   English to match the current maintainer-guide convention.
2. Kept each diagram's editable Mermaid source in the document without adding
   separately maintained images, a hosted service or runtime dependencies.
3. Added navigation from the development guide; retained the existing publication
   boundary, which links excluded documents to repository source on GitHub.
4. Linked the new view from the existing architecture OKF concept with source
   provenance, preserving historical sources and its existing index reachability.
5. Selected actual Mermaid parsing/rendering, visual inspection, Markdown links,
   OKF checks and the strict documentation build. Rust/Lean execution would not
   validate a documentation-only architecture synthesis.

## Implementation self-review

1. Compared the context/container views with CLI dispatch, analyzer calls, Git
   invocation, process execution and logical storage ownership.
2. Traced plan, verify/preview, progress and review branches; kept these commands
   in the same executable and distinguished their persistence and test execution.
3. Matched core/shell collaboration and final-output ordering to the shell
   contract. Preserved completion-acceptance versus task-termination distinctions.
4. Checked selected class-diagram fields/variants against the Rust definitions.
   Labelled the free-function representation and omitted payloads explicitly;
   start/stop events do not all contain effect IDs.
5. Checked diagram titles, legends, technology labels, edge direction, source
   links and proposal boundaries. No future language separation or pending API
   cleanup was presented as already implemented.

## Verification self-review

1. Parsed and rendered all five diagrams with Mermaid 12.1.0 in local headless
   Chrome via puppeteer-core 25.13.0. A first-pass sequence label used a semicolon
   that Mermaid interpreted as a statement separator; reworded it and rendered
   all five final diagrams successfully, including their titles.
2. Inspected rendered PNG previews of all five views. Adjusted flow direction,
   shortened labels and inspected the class diagram at its natural SVG dimensions.
   Rendered previews are scratch validation outputs, not duplicated repository
   diagram sources. GitHub's particular renderer version was not exercised.
3. Checked all 42 local links in the C4 document, including Markdown anchors.
   All targets exist. Checked the development-guide link to the new document.
4. Parsed all 48 OKF Markdown files for YAML/reserved-file structure. Checked
   source-ID uniqueness in the edited concept, the new source SHA-256, its
   citation and the existing architecture link from the catalog entry point.
5. Ran the strict site builder and generated internal-link/anchor checks with
   `.worktrees/issue-844-shell-contract/.venv/bin/python -m tools.docs_site
   --output site`; passed. The first attempt specified a scratch output path and
   was rejected by the helper's repository-site-only guard; the documented output
   path succeeded. `git diff --check` passed. The new architecture page remains
   outside the site's publication allowlist and is reached through a source link.

Renderer packages and previews were installed/generated only under
`/tmp/hoimin-c4-validation`; repository dependency files were unchanged. The
existing docs Python environment was reused. No Cargo build/test or Lean proof
build ran, and no Cargo target was created. No independent reviewer or runtime
correctness claim is implied by these author self-reviews.

Consulted the existing architecture, runtime, product-direction and evidence
contracts. Updated the architecture concept and development navigation. Current
source inspection and documentation checks do not refresh historical platform
or formal-verification results.
