# Operator reference implementation plan

**Goal:** Resolve #1193 by making every public operator and selector family understandable
from existing help and documentation, with trustworthy selection metadata.

**Architecture:** The core registry remains the owner of IDs, defaults and families.
The existing CLI reference generator projects that metadata into a linked index.
A handwritten operator reference owns examples, applicability and test interpretation.

**Tech stack:** Rust, Clap, Markdown, existing MkDocs publication tooling.

**Spec:** Issue #1193 and the design below.

## Inventory and design

The registry contains 126 operator IDs, 107 default runtime IDs and 12 selector
families. `run` and `plan` append one mixed list to help. The usage guide contains
a broad operator table and several detailed paragraphs; exact guards also appear
in the contributing guide and analyzer tests. Many individual IDs have no separate
example or lookup anchor. Existing documentation tests mostly search for words.

Use three entry points:

- `run`/`plan` help: selection rules, distinct default/opt-in/family lists, and
  guidance to the installed release's usage guide or the source checkout's reference.
- Usage guide: choose a set, exclude an ID/family, preview candidates, and interpret
  findings against a specification. Link detailed explanations rather than repeat them.
- Operator reference: one anchored entry per public ID, with a before/after example,
  the behavior it probes, and eligibility/exclusions/limitations. Shared rules have
  their own sections. The generated CLI reference supplies defaults and family members.

The existing `generate_cli_reference` example will generate the selection index
using `valid_names`, `parse_selector`, canonical lookup and default selection.
No parser of Rust source, second registry, JSON API, new command or Git-dependent
build step is needed. Include the new page in normal site publication and navigation.

Release links must lead to an existing, version-specific usage path, not silently
to main. Explain that the hosted site describes main; source builds use the docs
in the same checkout. Older releases may not contain the separate reference page.
Do not invent source-commit provenance for a binary that reports only a package version.

### Design reviews

1. Reader task: distinguish choosing operators from interpreting a survivor; examples
   identify useful observations without asserting a project's intended specification.
2. Ownership: project registry/default/family information through existing public
   selection functions; maintain explanations once in the reference.
3. Discoverability: every advertised ID and family needs a resolvable reference link;
   topic grouping must not become an undocumented selector family.
4. Version provenance: an already-published tag may lack this new page. Link its usage
   guide and distinguish source builds instead of generating broken versioned links.
5. Complexity: extend existing help, generator and publication paths. The design
   scores 9/10 with `software-design-philosophy`; the existing failed diagnostic is
   scattered ownership of applicability explanations. Consolidation and generated
   selection metadata address it without another runtime interface.

## Plan

- [x] Add regression tests for help classification, linked index completeness and
  generated metadata. Confirm failures before implementation.
- [x] Extend existing help and CLI reference generation. Use one source for operator
  identity/default/family membership; preserve CLI selection and execution behavior.
- [x] Write and fact-check all operator entries against analyzer code/tests. Move
  shared user-facing caveats out of the usage guide and preserve developer details.
- [x] Update usage, documentation navigation and the publication mapping. Keep the
  existing type-checker command example and valid old usage anchors.
- [x] Verify representative examples through analysis/preview, generated-doc freshness,
  property tests for selection semantics, focused Rust/Python checks and strict site build.
- [x] Perform independent review and finish implementation/test reviews.

Delivery: open a PR and follow hosted CI through completion; the PR records that
delivery state rather than duplicating mutable CI status here.

### Plan reviews

1. Completeness: inventory all IDs and families from production selection metadata;
   tests reject missing entries, broken links and stale default/member tables.
2. Test quality: do not freeze prose. Parameterize semantic examples and boundary
   inputs; use properties for include/exclude behavior and classification coverage.
3. Publication: check both repository Markdown and staged-site links/anchors. The
   docs source path is not necessarily the published URL.
4. Compatibility: preserve saved plan selections and all operator behavior. Help
   must work without project import, test execution, an LLM or repository access.
5. Scope and privacy: use independent synthetic examples only. Do not introduce
   an API, package-version policy change or a dependency for documentation lookup.

## Implementation reviews

1. Metadata ownership: help classification and the generated linked index use the
   existing canonical lookup, default selection and family expansion. No mutation
   behavior, public API, schema or dependency changes were needed.
2. Semantic accuracy: reviewed every operator entry against existing code and
   documentation. Corrected one-way Sequence-to-Iterable conversion, absent set
   deletion, valued-return erasure exclusions, gather option addition, explicit
   f-string conversions and surrogateescape-only encoding mutation. Distinguished
   source replacements from behavior examples with evaluation-preserving wrappers.
3. Applicability preservation: moved the complete conversion-call table and the
   restrictive inheritance/dataclass/capture guards into the reference. Kept
   first-erased-statement selection, side-effect ordering, generator construction
   and diagnostic limitations. Replaced seven older usage sections with links
   while retaining their headings/anchors; no second handwritten default roster.
4. Discoverability and version: generated each canonical ID's default state and
   family links; each family links its exact members. Help points to the existing
   release-tag usage path and the same-checkout reference for source builds. The
   website's main-branch content is explicitly distinguished from a release.
5. Independent review and complexity: a read-only reviewer checked all entries and
   the changed help/generator/tests. Addressed factual errors, duplicate ownership,
   an invented preview command in the draft, invalid developer anchors and CRLF
   test portability. Final review found no blocking issues. Public examples use
   independent synthetic code; the patch contains no external project details.

## Test reviews

1. Regression sensitivity: before implementation, the five help/reference tests
   failed on mixed lists, absent version links and the missing reference page.
   The two generator metadata tests failed before the index was implemented.
2. Registry contracts: exact canonical-entry coverage rejects omissions and
   duplicates. Generated family tests compare complete ordered expansions. The
   property test reads published metadata as its oracle and compares arbitrary
   default/explicit selections and exclusions with both CLI parsers, including
   empty-selection rejection and LF/CRLF inputs.
3. Example boundaries: nine parameterized analyzer cases check actual emitted
   replacements or explicit exclusions, including slices, causes, standalone
   conversions, `!a`, set deletion, ternary conditions, valued returns, existing
   gather options and strict encoding. Each case has one scenario and AAA phases.
   Prose-copy assertions were removed instead of copying the new prose into tests.
4. Execution boundary: native Windows CLI planning and dry-run verification of a
   synthetic regex module retained two candidates. The module raises if imported
   and the configured test executable does not exist; neither ran. The existing
   usage-command integration contract also passed, retaining type-checker examples.
5. Publication and quality: strict documentation build validated staged links and
   anchors; 68 documentation/skill Python tests passed. Focused Rust checks passed
   (234 CLI configuration, 15 operator reference, 9 generator cases), along with
   the usage-command contract, Clippy with denied warnings, formatting, Ruff and
   focused ty checks. The generated-reference freshness check passes. The reviewer
   found a Windows checkout newline issue; normalization and generated LF/CRLF
   property inputs cover the correction.

## Software design assessment

**Score: 9/10.** Applied `software-design-philosophy` to the metadata and document
boundaries. All eight final Quick Diagnostic rows pass for the changed design.
Initial failed rows 3, 6 and 7 concerned scattered operator knowledge and the need
to read implementations to interpret IDs. The registry owns identity/selection,
the existing generator projects it, the reference owns applicability, and the
usage guide owns selection workflow. The contributor guide explains that boundary.
No extra runtime interface or metadata framework was introduced. The remaining
maintenance cost is factual review of handwritten semantics when analyzers change;
automated completeness and link checks cannot prove prose correctness.

## Documentation Audit: operator help and reference

**Score:** 9/10 — ready for review. **Shippable:** yes.
**Diagnostic:** 10/10 final; no remaining failed rows.
**Doc type / reader:** reference and selection how-to for Python test authors and
external agents. **Language:** en.
**Local style guide:** `docs/development/guides/documentation.md`, supplemented by
the `technical-documentation` skill. **Blocking:** none.

| # | Location | Rule | Before | After | Severity |
| --- | --- | --- | --- | --- | --- |
| 1 | Usage/operator sections | Diagnostic 2: task and document type | Repeated inventories and detailed guards in a workflow guide | Linked reference owns per-ID meaning and guards; old anchors preserved | Important |
| 2 | Draft examples | Diagnostic 3: verified behavior | Several inferred directions/shapes and a nonexistent preview command | Source-checked supported shapes and `verify --dry-run` | Blocking, resolved |
| 3 | Help and reference links | Diagnostic 8: descriptive cross-references | Mixed roster; missing detailed entry points | Release/source routes, per-ID anchors and generated families | Important |
| 4 | Defaults and rollout prose | Diagnostic 9: timeless text | Repeated defaults and historical rollout descriptions | Generated selection policy with stable workflow guidance | Minor |

Rewrite order completed: establish document ownership, verify facts and limits,
preserve useful examples, repair navigation/version guidance, then tighten prose.
No diagram is needed for the ID lookup task. Eligibility prose still needs human
review when implementation changes; the checks deliberately do not freeze wording.
