---
okf_version: "0.2"
---

# hoimin design and audit catalog

- [Product direction](product-direction.md) - Local AI-agent test improvement, score limitations, resource guards, and feature priorities.
- [Second-order mutation adoption](second-order-mutation.md) - Bounded research, first-order comparison and the decision to defer composite infrastructure.

- [Agent skill distribution](../agent-skills.md) - Standard skills packages, GitHub CLI installation and release validation.

- [Documentation publication](documentation-publication.md) - Searchable main site, permissions, activation and reruns.

- [Catalog scope and evidence guide](overview.md) - Source revisions, evidence levels, untracked material and updates.
- [Development reference and update procedure](../okf-workflow.md) - Starting work, creating concepts, updating sources/indexes and completion checks.
- [Discussion intake and activation](discussion-intake.md) - Report forms, minimal reproductions, category setup and post-merge verification.
- [AAA test structure](test-structure.md) - Readable arrangement, subject operations and assertions in Python and Rust tests.
- [Updating the Rust pin and minimum version](rust-toolchains.md) - Stable updates, nightly validation, update PRs and distribution checks.
- [Publishing wheels to PyPI](pypi-publishing.md) - Automatic publication through TestPyPI, Trusted Publisher registration, attestations and reruns.
- [Compatibility and change notices](compatibility.md) - Public interfaces, version floors, migration guidance and review.

- [Archive and wheel execution checks](artifact-smoke.md) - Byte identity, Linux ABI, evidence and publication gates.

- [Release artifact SBOMs](release-sbom.md) - Cargo dependency graph scope, artifact mapping and pre-publication validation.
- [GitHub Release provenance](release-provenance.md) - Attested/published file identity, source identity, validation and publication gates.

- [Rust dependency licenses and sources](dependency-policy.md) - Both workspaces, narrow exceptions and build-script diff review.

- [CI selection and required checks](ci-validation.md) - Conservative diff classification, aggregation, strict workflow checks and publication permissions.
- [CLI reference generation and synchronization](cli-reference.md) - Generation from Rust definitions, handwritten guidance and synchronization checks.

# Designs and contracts

- [Architecture](design/architecture.md) - Isolated copies, state transitions separated from I/O, and the Rust analyzer migration.
- [Python analysis and candidates](design/analyzer.md) - Syntax, name resolution, user-defined exceptions, byte ranges and input scale.
- [Target selection, plan and verify](design/selection-plan-verify.md) - Ranking, subsets, seeded sampling and saved-format revision questions.
- [Resources and cleanup](design/runtime-lifecycle.md) - OS-specific limits, cleanup ownership and report writing.
- [Sessions and reports](design/session-report.md) - Database ownership, migration, read validation, live progress and diagnostic logs.
- [JSON mutant spool](design/json-report-spool.md) - Record-level writing and acknowledgement/poison contracts on failure.

- [Fingerprint input globs](design/fingerprint-inputs.md) - Shared traversal across include globs and input-order errors.

# Audits and verification scope

- [Additional audit Lean integration report](../superpowers/reports/2026-09-15-followup-lean-promotion.md) - Six models, 33 theorems, 58 inputs and unresolved implementation correspondence.

- [Nullable mutation eligibility](audits/nullable-gates-2026-09.md) - #564/#565 reproductions and Lean validation of name resolution and type-argument trees.

- [Copy policy and session resume](audits/resume-copy-2026-09.md) - #563 verdict reuse and Lean/public-CLI/SQLite comparisons.

- [Declaration-only annotations and candidate precision](audits/declaration-only-2026-09.md) - Binding preservation, function-local declarations and 14 public-plan/CPython regression inputs.

- [Evaluation order and unselected method allocation](audits/evaluation-order-2026-09.md) - #560 evaluation order, #561 skipped replacement allocation and cumulative allocation checks.

- [Deferred annotations and collections.abc.Set audit](audits/annotation-followup-2026-09.md) - #558/#559 reproductions, cache retention proofs and type-name comparisons.

- [with suppression and finally analysis cost](audits/with-finally-2026-09.md) - Suppression fixes and #557 duplicate-traversal reduction/visit-count gates.

- [Implicit exceptions and finally](audits/implicit-finally.md) - Typing-derived merges, small Lean models and public-plan correspondence.

- [Reading Lean audit evidence](audits/lean-evidence.md) - Model proofs, finite search, Rust comparisons and hosted checks.
- [2026-09-11 boundary audit](audits/boundary-2026-09.md) - Cross-cutting analysis, selection, execution and persistence results and unverified conditions.
- [Progress-input follow-up](audits/progress-input.md) - Historical 368/564 observations, JSONL reading and verification scope.
- [July 2026 Rust audit](audits/rust-2026-07.md) - Initial finding classification and host constraints.

# Finding original sources

- [Source catalog entry point](references/index.md) - Design, audit and report lists.
- [Design catalog](references/design-documents.md) - Index of all original design Markdown.
- [Audit and report catalog](references/audit-documents.md) - Index of all original audit and report Markdown.

- [Performance validation by input shape](audits/performance-shapes.md) - Execution gates, release measurements, dependent PRs and evidence limits.

- [Cross-boundary contract fixtures](audits/boundary-contracts.md) - Strict execution, complete reporting and unverified conditions.

- [Collection annotation references](design/annotation-builtins.md) - Bidirectional concrete types, deferred evaluation and scope checks.

- [September 2026 analysis audits and regression integration](audits/analysis-2026-09.md) - Original evidence, fix PRs and formal tests for #545 through #549.

- [Constrained mutation search and differential test generation](mutation-search.md) - Measurements on fixed functions, adopted scope, guided search and CrossHair limits.

- [Typed results and executable contracts](typed-results-and-contracts.md) - Expected failures, optional budget checks, and evidence limits.

- [Fuzzing maintenance adoption](fuzzing-adoption.md) - Corpus replay evidence, property reuse and bounded exploration decisions.
