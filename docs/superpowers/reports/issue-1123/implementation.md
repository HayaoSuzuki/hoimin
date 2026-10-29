# Inheritance operator implementation

`base_order_shuffle` is a default runtime operator, independently removable with
`--exclude-operators base_order_shuffle` and selectable with `--operators`.
The catalog has 104 IDs and 85 default runtime IDs. `override_method_delete`
and `super_call_move` are also default runtime operators with individual opt-out.
Historical experiment receipts are unchanged.

## Source editing and repeatability

The analyzer accepts undecorated, non-generic classes with at least two distinct
simple-name bases and no class keywords. Imported names and nested classes are
eligible; target modules are never imported. It excludes attributes, calls,
subscripts, starred expansions and duplicate normalized identifiers. This syntax
restriction avoids moving evaluation expressions, but does not prove runtime
class identities, MRO or metaclass/subclass-hook behavior.

Two bases swap. For larger lists, Fisher-Yates consumes a BLAKE3 XOF seeded from
the original first-to-last-base span with domain `hoimin/base_order_shuffle/v1`.
Little-endian eight-byte draws determine each swap. An identity permutation
falls back to exchanging the first two indices. There is one candidate, without
runtime entropy, a seed option, uniformity guarantee or factorial enumeration.
The Rust implementation deliberately uses its own stable source-derived scheme;
it does not promise identical permutations to Python's research RNG.

The edit substitutes each original name token in its existing slot. All commas,
grouping, comments, whitespace, trailing commas, bodies and newline styles remain.
Names retain their original source spelling, including Unicode. Scope and line
selection precede replacement construction; candidate identity, ranking, retention
caps, logical payload budgets and cancellation use the existing analyzer paths.
Line selection starts at the first base name, and symbol selection uses the class.

## Behavioral meaning

The cooperative callback fixture preserves event membership but changes event
order. Weak checks can survive while a contract-based order assertion kills the
saved mutation. Correct use of `super()` can therefore produce a kill. Conversely,
explicit base calls may ignore the new MRO. No result establishes correct use of
`super()` by itself.

Reordered dependent bases can raise `TypeError` during class creation, and
runtime aliases, implicit metaclasses or hooks can introduce other failures.
The operator performs no runtime MRO validation. Normal test-process outcomes
apply; a killed mutant can represent an exception rather than a missing
behavioral assertion. Review the failure and API contract before adding tests.

## Verification

- The initial five analyzer tests failed because the new ID was unregistered.
  After implementation, all six analyzer tests passed, including selection before
  construction, cancellation after a completed replacement and payload-limit lease
  release. Permutation checks cover three through twelve bases with 24 distinct
  source layouts each, preserving membership and rejecting unchanged order.
- Five public CLI tests pass with CPython 3.14.9 and 3.15.1: default-on/opt-out,
  stable selected plans, fresh-baseline saved-plan verification (weak survival,
  strong kill), actual execution for three through six bases, Latin-1 and LF/CRLF/
  lone-CR source preservation, and planning without target import. Test setup was
  corrected to use the existing `--source` and qualified `--symbol` CLI contract.
- Related Python documentation/boundary tests: 61 passed, 1 skipped. An initial
  sandbox run had 43 temporary-directory permission errors; rerunning under the
  ordinary execution environment resolved them without changing code.
- Workspace and vendored-parser formatting checks pass. The CLI reference was
  regenerated with its maintained example. The 102-entry corpus catalog is closed
  and unique; the knowledge inventory has 63 pages without missing links/sources
  or unreachable pages.
- A fresh read-only reviewer found no actionable issue in the helper, collector,
  registry, ranking, tests or documentation. It checked normalized-name rejection,
  trivia preservation, deterministic nonidentity edits, selector/scope identity,
  cancellation and the distinction between runtime errors and assertion gaps.

- `cargo test --locked --offline --workspace --all-features -- --test-threads=2`
  passed. The last summaries of 193 top-level test binaries/doc-test sections
  report **3,878 passed, 0 failed, 32 ignored**, without double-counting child
  fixture summaries. Cargo used two build jobs, no incremental compilation and
  disabled debug information; the existing shared target cache was reused.
- `cargo clippy --locked --offline --workspace --all-targets --all-features --
  -D warnings` and the repository's separate vendored-parser Clippy check pass.
  Initial Clippy findings concerned assertion diagnostics and string construction
  in the new tests; all were corrected without changing product behavior.
- The generated CLI reference passes `generate_cli_reference -- --check`.

These are local verification results; remote CI status is a separate result.

## Override deletion and delegation movement

The follow-up implements the two operators recommended by the original research.
A shared index runs only when either ID is selected. It accepts ordinary top-level
same-module single inheritance with an earlier root parent and matching simple
instance-method signatures. It conservatively rejects ambiguous class identities,
class escapes, method aliases/writes, descriptors and dynamic namespace changes.
External monkey-patching remains outside the source model.

Deletion replaces the complete child definition with `pass`. Movement permutes
original statement fragments between their separator slots: first delegation
moves last, otherwise it moves first. Comments stay in their original slots and
the initial docstring remains fixed. Both use existing selection, candidate caps,
payload accounting, cancellation, ranking and saved-plan execution.

See [usage](../../../usage.md#mutation-operators) for eligibility and opt-out.
The earlier verification results above describe the base-order implementation;
the new operators' verification is recorded separately below.

### Follow-up verification

Five public CLI tests pass on CPython 3.14.9 and 3.15.1. They cover default-on
selection, individual opt-out without removing the other ID, stable selected
candidate identities, saved-plan weak survival and strong kills, equivalent
behavior, runtime-failure controls, planning without importing the target, and
Latin-1 plus LF/CRLF/lone-CR source restoration through actual verification.
The existing default-selection tests (5) and CLI configuration tests (84) pass.
Documentation/boundary/reference tests report 61 passed and 1 skipped. The
corpus catalog contains 104 unique IDs; reference inventory covers 63 pages
without missing local links/sources or unreachable pages.

Independent review found class-evaluated bare method captures, named descriptor
bindings and unsupported subclasses passing a tracked parent to an unknown
metaclass. Pre-fix public plans reproduce one unsafe deletion candidate in each
case. The follow-up excludes these visible hazards and adds regression coverage.
Unrelated decorated methods are also excluded because their returned descriptors
can run class-creation hooks. None of these exclusions proves absence of external
monkey-patching. The reused name-resolution builder does not internally poll
cancellation; surrounding index traversals and construction boundaries do.

The final focused analyzer suite passes all 15 tests. It checks method/signature
and binding exclusions, docstrings/comments/grouping/newlines, shared-index work,
selection before replacement construction, retained prefixes, cancellation during
indexing and after a replacement, and released payload leases. A 400-method
fixture records exactly 400 method checks; disabled IDs do no index work.
The analyzer crate's all-target Clippy check passes with warnings denied.

Workspace formatting, all-target/all-feature Clippy with warnings denied, and
the generated CLI reference consistency check pass. An overlapping earlier test
run held a Windows executable open and caused LNK1104 during a rebuild; the old
run was terminated and the final workspace check runs alone.

The final single workspace run, after all review fixes, passes:
`cargo test --locked --offline --workspace --all-features -- --test-threads=2`.
The last summaries of 194 top-level binaries/doc-test sections report
**3,898 passed, 0 failed, 32 ignored**, excluding nested fixture summaries.
The final built public CLI suite also passes all five tests on CPython 3.15.1;
the workspace run uses CPython 3.14.9. No Rust source changed after verification.
