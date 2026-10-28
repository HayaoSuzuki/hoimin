# Implementation review record

This follows the maintainer's request to implement the accepted research scope
on the existing branch and PR #1133. Reviews are by the implementing agent;
they are not independent approval or formal verification.

## Stage 1: operator catalog and binary division

1. Requirements: registered `binary_div_floor`, `structure_slice_bound_omit`
   and `structure_slice_bound_move` as enum variants, defaults and canonical
   names. Extended `structure_ops` with the two slice IDs. Existing families,
   explicit selections and exclusions retain their established ownership.
2. Identity/defaults: appended enum declarations rather than reordering old
   entries. Updated the independent canonical-name and 75-default fixtures;
   the 24 existing catalog/selection tests pass.
3. Syntax/semantics: the token producer first requires an AST-confirmed binary
   operator token outside annotations. Extra `/`/`//` edits share admission
   with existing `*`/`%` mutations; `/=` and `//=` remain separate unchanged IDs.
4. Resources: at most one additional short token edit per division. It uses
   existing cancellation, prefix retention, profile filtering and payload
   charging; no extra traversal, runtime import or third-party dependency.
5. Evidence: initial default-discovery tests failed with zero new candidates.
   The binary test then passed with exact opposite-direction edits and retained
   incumbent arithmetic. Simplified nested optional construction to a match
   and selection filter after reviewing the new token path.

## Stage 2: slice-bound mutation

1. Applicability: both bounds must be names or integer literals (including
   unary-negative integers), with no step value and a load-context slice.
   Calls, attributes, compound bounds, multidimensional indices, direct
   assignment/delete targets and annotations produce no new candidates.
2. Syntax: deletion uses Ruff's complete parenthesized bound range, preventing
   leftover parentheses. Movement retains the complete parenthesized bound
   text, including comments. UTF-8 offsets are source byte ranges.
3. Semantics: omission removes exactly one bound; movement keeps exactly one
   bound evaluation. Both preserve receiver evaluation. No numeric/binding
   inference claims equivalence, and no both-bound/full-range mutation is added.
4. Resources/selection: omission emits at most two candidates, movement one.
   Shared admission handles cancellation, caps and target membership. Movement
   checks selection before copying text. Integer syntax is inspected without
   numeric conversion, so Python-sized literals do not require bounded parsing.
5. Evidence: the parenthesized/UTF-8 positive test failed at zero candidates
   before the collector existed. All five analyzer tests then passed, including
   exact edits, forbidden contexts, a truncated one-candidate prefix and an
   exact line/symbol selection with no false truncation.

## Stage 3: public contracts and regression validation

1. Public behavior: the default/opt-out CLI fixture now exercises all three
   new IDs and checks unaffected candidate descriptors after exclusion. Five
   behavioral scenarios verify weak survivors and justified strong-test kills
   through real `plan`/`verify`, with passing baselines and unchanged originals.
2. Existing coverage: updated independent 75-default/94-catalog expectations,
   structural-family lists and ranking inventory. A former blanket assertion
   against all `structure_*` IDs incorrectly treated newly supported variable
   slices as unsupported; scoped that fixture to its original six families.
3. Formal evidence: registered the three IDs as explicitly deferred from the
   initial Lean cross-producer corpus, with specific reasons and pointers to
   dedicated coverage. Existing Lean corpora/models are unchanged; no new
   formal theorem for these operators is claimed or silently inferred.
4. Independent review: a separate read-only reviewer found one P2 defect:
   movement dropped comments adjacent to the colon. A new regression failed
   with `n:`/`:n` instead of preserved trivia. Moving only the colon fixed it;
   all eight focused analyzer tests passed. No other actionable defect was found.
5. Host/resources: the first broad run exposed the existing Unix-socket test's
   sandbox denial; reran with the same two-job/two-thread limits outside that
   restriction. All-feature/all-target clippy passes with warnings denied.
   Broad runs also exposed fixed-list expectations and the inventory registration;
   final results below distinguish repaired checks from earlier failures.

## Stage 4: documentation and integration review

1. Current contract: usage now states 75 defaults and 94 catalog IDs, all three
   opt-out names, slice applicability and division type/overload behavior.
   Removed stale historical-default restoration lists from current usage and
   dated the older compatibility notice instead of implying it is current.
2. Generated evidence: regenerated CLI help from current command definitions.
   Check mode and all seven generator tests pass. The new names are reflected
   in both run and plan help; there is no hand-edited generated list.
3. Provenance: kept the research JSON files unchanged and explicitly separated
   the implementation from research commit `1efe26f5`. Updated the decision,
   source catalog, links, read source revisions/states and final content hashes;
   no historical successful execution is relabelled as a new product test.
4. Coverage/limits: the final full Rust run passes 3,961 tests across 194 suite
   summaries, with 34 existing ignored probe/descriptive tests and zero failures.
   Python 3.15.0b1 runs four default/opt-out/plan tests plus three division
   execution contracts. These results do not claim a stable Python 3.15 release,
   Linux/Windows execution or new Lean proof coverage.
5. Resources/integration: all build/check commands use two Cargo jobs, no
   incremental output and no debug information; broad tests use two threads.
   Final pre-integration free space is about 186 GiB, with about 4.8 GiB in
   `target`. Commit hooks run with the same build limits before final
   `cargo clean`; cleanup confirmation belongs to the PR/completion report.
   Only intended source/docs/evidence are included, leaving `.idea/` and
   `.serena/` untouched.

## Final commands and results

Unless overridden for the supported-interpreter check, CLI fixtures use the
repository CPython 3.14.9 environment on macOS arm64.

- `cargo test --locked --workspace --no-fail-fast`, with two jobs/test threads:
  exit 0, 3,961 passed / 0 failed / 34 ignored. The run was outside the restricted
  sandbox because an existing test binds a local Unix socket.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`:
  exit 0. No production Rust code changed after that check.
- `HOIMIN_OPERATOR_TEST_PYTHON=<CPython 3.15.0b1> cargo test --locked -p hoimin-cli
  --test default_operator_selection`: 4 passed.
- The same interpreter override with `--test operator_function_contracts
  division_exchange`: 3 passed, 18 unrelated contracts filtered.
- `cargo test --locked -p hoimin-cli --example generate_cli_reference`: 7 passed;
  generated-reference `--check` and `cargo fmt --all -- --check` pass.
- `python -m pytest tests/test_knowledge_references.py tests/test_docs_site.py
  tests/test_boundary_contracts.py -q`: 62 passed.
- OKF frontmatter parses for all 61 pages. Link inventory has no missing
  Markdown/source targets or unreachable pages. Updated source hashes and
  revisions are checked separately before commit.
- `git diff --check`: pass.

The independent review's sole P2 comment-preservation finding was fixed with an
observed failing regression followed by a passing test and the green full suite.
No deferred implementation defect was identified. The explicitly deferred
initial Lean corpus registrations are an evidence limit, not a claim of proof.
