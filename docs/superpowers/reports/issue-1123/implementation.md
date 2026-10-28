# Base-order operator implementation

`base_order_shuffle` is a default runtime operator, independently removable with
`--exclude-operators base_order_shuffle` and selectable with `--operators`.
The catalog has 102 IDs and 83 default runtime IDs. The other two inheritance
operators remain research proposals. Historical experiment receipts are unchanged.

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
