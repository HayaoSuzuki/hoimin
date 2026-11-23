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

## Shared prose revision

Review identified a missed ownership problem: explanatory paragraphs were embedded
in rendering code and selection rules were restated in the guide and index.
Use Clap doc comments for argument explanations and crate-local external document
fragments for long text. Generate the website's CLI reference from those same
inputs. Keep the operator registry responsible only for identity and selection.
Do not introduce a Markdown parser, template engine or runtime document lookup.

Design reviews:

1. Reader requirement: the CLI and website must use the same sentences, not merely
   independently maintained explanations with similar meaning.
2. Ownership: argument doc comments belong beside their declarations; long text
   belongs in external documents. The renderer owns only composition and metadata.
3. Packaging: crate-local compile-time includes remain available in Cargo and
   Python source distributions without installing documents beside the binary.
4. Presentation: retain short `-h` summaries and detailed `--help`; render the same
   long descriptions in the operator index, and link workflow instructions to it.
5. Complexity: reuse Clap reflection and `include_str!`; no new parser or runtime
   interface. The previous assessment missed diagnostic rows 3 and 6 (independent
   editing and information ownership). Revision starts at 7.5/10; reassess after checks.

Implementation-plan reviews:

1. Add cross-output consistency checks before changing the renderer and observe
   failures where the website does not reuse the argument explanations.
2. Move the root/verify footers, operator navigation and generated-reference prose
   into crate-local documents; preserve version interpolation and generated lists.
3. Expand selection argument doc comments and reuse the reflected long help in
   the generated index; remove repeated policy prose from the usage procedure.
4. Keep includes deterministic across Windows/Unix checkouts; verify generation,
   publication and source-distribution retention, not just local compilation.
5. Run focused CLI, generator, property, site and static checks; request independent
   review, update the existing PR and follow hosted CI.

Implementation reviews:

1. Removed explanatory paragraphs from `format!`/`push_str` in the help and reference
   generator. Root/verify footers and operator navigation use external documents;
   selection/exclusion explanations use argument doc comments.
2. The operator index obtains the same long help that Clap displays. The usage
   procedure links to it instead of restating selection policy. The existing
   full command-help sections continue to use Clap's renderer.
3. Formatting code retains structural headings, table delimiters and generated
   metadata only. Version/command interpolation uses compile-time format strings;
   no runtime parser, file access or additional dependency was introduced.
4. Kept all six fragments within the crate and pinned their checkout newlines to
   LF. Actual source-distribution checks confirmed byte-for-byte retention.
5. Independent review found no blocking issues. Removed a reviewer-noted sentence
   exposing doc-comment ownership in the user workflow; that explanation belongs
   in contributor documentation. Generated HTML was checked for wording, not claimed
   to have undergone a visual-layout review.

Test reviews:

1. All four cross-output selection/exclusion cases failed before the change and
   pass after generation. Assertions compare real Clap descriptions with the
   website index rather than copying the expected paragraph into a test.
2. Seven parameterized real-binary checks run help from an empty temporary working
   directory and compare its wording with the published Markdown. Each case
   covers one command and uses Arrange/Act/Assert phases.
3. A generative test covers Unicode wording and LF/CRLF long-help inputs. Existing
   arbitrary operator-selection properties remain intact; wrapping differences
   are normalized without rewriting the explanation's words.
4. Source-archive and documentation suites passed: 109 Python tests, including
   six parameterized shared-document byte-retention checks. Focused Rust checks
   passed: 234 CLI configuration, 26 reference and 10 generator tests.
5. Strict site construction and links passed. Parsed generated HTML contains the
   actual help wording for all seven commands. Clippy with denied warnings,
   formatting, Ruff and focused ty checks passed. Final generator freshness tests
   passed after the last prose edit; hosted CI is tracked by PR #1207.

Revised software-design assessment: **9/10**, all eight diagnostic rows satisfied
within this change. Rows 3 and 6 now have explicit ownership: edit prose in doc
comments/fragments, metadata in the registry, and presentation in the renderer.
The documentation assessment remains **9/10**, with no blocking or failed Quick
Diagnostic rows. Handwritten applicability still requires factual review when
analyzers change; sharing text does not prove its semantics.

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
