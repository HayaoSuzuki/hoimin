---
okf_version: "0.2"
---

# hoimin design and audit catalog

Use this index to find current maintainer procedures, design rationale and
historical evidence. Begin with the guide for your task, then follow its linked
contract and original sources. See the [scope and evidence guide](overview.md)
before interpreting a historical result as a current guarantee.

- [Production budget arithmetic proofs](creusot-budget-proofs.md) — same production bodies, proof maintenance and evidence boundaries.

## Start with your task

The guides below own reusable procedures; concepts and dated records explain
their rationale, implementation boundaries and observed evidence:

| Task | Current guide | Contract or evidence |
| --- | --- | --- |
| Improve a project's tests | [Plan/verify loop](../usage.md#plan-and-verify-with-an-agent) | [Product direction](product-direction.md) and [selection contract](design/selection-plan-verify.md) |
| Change execution limits or cleanup | [Limits](../usage.md#limits-and-defaults) and [disk safety](../usage.md#disk-safe-execution) | [Runtime lifecycle](design/runtime-lifecycle.md) |
| Change saved plans, reports or sessions | [Compatibility](../compatibility.md) | [Selection](design/selection-plan-verify.md) and [session/report contracts](design/session-report.md) |
| Change or validate the CLI | [Generated CLI procedure](../development.md#generated-cli-reference) and [quality gate](../development.md#local-quality-gate) | [CLI reference scope](cli-reference.md) |
| Diagnose CI or release publication | [CI](../ci.md), [releases](../releases.md) and [PyPI recovery](../pypi-publishing.md) | [CI validation](ci-validation.md), [provenance](release-provenance.md) and [PyPI evidence](pypi-publishing.md) |
| Edit documentation or interpret an audit | [OKF update procedure](../okf-workflow.md) and [site publication](../documentation.md) | [Evidence guide](overview.md) and [Lean evidence limits](audits/lean-evidence.md) |

## Maintainer playbooks and scope

These links explain decisions and scope. Follow their guide links for current
commands rather than copying commands from a dated design or execution report:

- [Product direction](product-direction.md) - Local AI-agent test improvement, score limitations, resource guards, and feature priorities.

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

- [Dependency licenses, sources and updates](dependency-policy.md) - Python/Rust version ranges, locked graphs, scoped license exceptions and build-script review.

- [CI selection and required checks](ci-validation.md) - Conservative diff classification, aggregation, strict workflow checks and publication permissions.
- [CLI reference generation and synchronization](cli-reference.md) - Generation from Rust definitions, handwritten guidance and synchronization checks.

# Designs and contracts

Use these topic summaries to locate rationale and source references. Check their
recorded revisions against the implementation you are changing:

- [Architecture](design/architecture.md) - Isolated copies, state transitions separated from I/O, and the Rust analyzer migration.
- [Python analysis and candidates](design/analyzer.md) - Syntax, name resolution, user-defined exceptions, byte ranges and input scale.
- [Target selection, plan and verify](design/selection-plan-verify.md) - Ranking, subsets, seeded sampling and saved-format revision questions.
- [Resources and cleanup](design/runtime-lifecycle.md) - OS-specific limits, cleanup ownership and report writing.
- [Sessions and reports](design/session-report.md) - Database ownership, migration, read validation, live progress and diagnostic logs.
- [JSON mutant spool](design/json-report-spool.md) - Record-level writing and acknowledgement/poison contracts on failure.

- [Fingerprint input globs](design/fingerprint-inputs.md) - Shared traversal across include globs and input-order errors.
- [Collection annotation references](design/annotation-builtins.md) - Bidirectional concrete types, deferred evaluation and scope checks.
- [Survivor review](design/survivor-review.md) - Candidate identity, review history and separation from execution outcomes.
- [Typed results and executable contracts](typed-results-and-contracts.md) - Expected failures, optional budget checks, and evidence limits.

# Audits and verification scope

Treat these as evidence at their recorded revisions, inputs and environments,
not a claim that every current host or commit passed the same checks:

- [Design and resource audit after PR 1069](audits/design-2026-10-22-post1069.md) - Original manifest read budgets, first-store publication and rejected type/state changes; #1070–#1071 and implementation follow-up.
- [Design and resource audit after PR 1064](audits/design-2026-10-22-post1064.md) - Source scope, finite reads, Windows state and evidence navigation; #1065–#1068 and implementation follow-up.
- [Design and resource audit after PR 1059](audits/design-2026-10-22-post1059.md) - Symbol admission, artifact boundaries and rejected state/compatibility alternatives.

- [October 2026 design-audit history](audits/design-history-2026-10.md) - Eight archived reviews, completed follow-ups and conditions for reconsidering earlier decisions.

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

- [Performance validation by input shape](audits/performance-shapes.md) - Execution gates, release measurements, dependent PRs and evidence limits.
- [Measuring test execution costs](test-execution-costs.md) - Collection/build/runtime separation, measured fixture costs and retained integration coverage.

- [Cross-boundary contract fixtures](audits/boundary-contracts.md) - Strict execution, complete reporting and unverified conditions.

- [September 2026 analysis audits and regression integration](audits/analysis-2026-09.md) - Original evidence, fix PRs and formal tests for #545 through #549.

## Research and adoption decisions

Read the conclusion and limitations before reviving a deferred feature:

- [Runtime performance and maintenance](../performance/2026-10-12-runtime-improvements.md) - Measured decisions for #813–#815 and artifact-reader/release-policy ownership for #816–#817.
- [Second-order mutation adoption](second-order-mutation.md) - Bounded research, first-order comparison and the decision to defer composite infrastructure.
- [Constrained mutation search and differential test generation](mutation-search.md) - Measurements on fixed functions, adopted scope, guided search and CrossHair limits.
- [Fuzzing maintenance adoption](fuzzing-adoption.md) - Corpus replay evidence, property reuse and bounded exploration decisions.

# Finding original sources

Use the source catalogs to find original decisions and execution records. Their
titles and metadata retain the original language and historical scope:

- [Source catalog entry point](references/index.md) - Design, audit and report lists.
- [Design catalog](references/design-documents.md) - Index of all original design Markdown.
- [Audit and report catalog](references/audit-documents.md) - Index of all original audit and report Markdown.
