# Standard-library conversion removal

Base: main `2a493a610398a0ed2759b01c205645d90585461e`.
Worktree: `.worktrees/decimal-conversion`; branch: `feat/decimal-conversion`.
Both origin URLs are `https://github.com/HayaoSuzuki/hoimin.git`; the dedicated
branch is the push destination, never main.

## Intent and investigation

The user reported a separate manual mutation: changing `return Decimal(value)`
to `return value` left 1,668 serializer tests passing, while adding
`isinstance(result.amount, Decimal)` detected the retained float. This report
is user-provided external evidence, not a hoimin run or a locally reproduced
1,668-test suite. Existing `conversion_call_remove` covers eleven unshadowed
builtins; it does not cover Decimal or Fraction. The user requested Fraction
and other standard types, so the first explicit supported set is Decimal and
Fraction. Arbitrary constructors and `typing.cast` remain excluded.

## Design and five self-reviews

1. **Semantics:** replace the whole one-argument call with the original argument
   in parentheses, preserving its single evaluation while deliberately removing
   conversion and allowing its runtime type to change.
2. **Identity:** reuse the conservative import checks already used for `operator`;
   trust only unique unconditional module imports, including aliases. Preserve
   exclusions for rebinding, namespace access, class loads and module escapes.
3. **Selection:** add a distinct `stdlib_conversion_call_remove` operator, enabled
   by default and independently excludable. Do not expand the existing builtin
   operator or historical explicit selections.
4. **Boundaries:** allow only `decimal.Decimal` and `fractions.Fraction`; reject
   keywords, multiple arguments and unsafe argument/syntax roles. Static import
   analysis does not establish absence of path shadowing or external monkeypatching.
5. **Complexity:** share import identity knowledge through `CallableImports`;
   keep operator-function replacement rules and conversion eligibility in their
   own callers. No plugin registry or general constructor inference is added.

## Plan and five self-reviews

1. **Baseline and red:** run existing conversion/operator integration tests;
   add a public CLI test and confirm rejection of the not-yet-registered operator.
2. **Implementation:** extract import checks without changing the operator client,
   add the new emission gate, and register selection/ranking/valid-Python inventory.
3. **Evidence:** exercise exact source spans, CPython compilation, supported alias
   forms, exclusion boundaries, actual saved-plan execution, and bounded properties.
4. **Compatibility:** grow defaults from 53 to 54 and total IDs from 72 to 73;
   raise all five package/lock floors to 0.5.0. Document rollback selections and
   preserve existing IDs, enum order, saved schemas and ranking rule.
5. **Delivery:** use two build/test workers, no incremental/debug artifacts, and
   a task-owned target. Monitor process-group RSS (8 GiB), target size (12 GiB),
   disk free (20 GiB minimum), and per-command time (40 minutes). Obtain independent
   review, run checks, commit with hooks, clean the target, then create a PR.

## Compatibility and model boundary

The 0.5.0 floor follows `docs/compatibility.md`: changing defaults changes the
candidate population and may alter capped prefixes, runtime and score comparisons.
`--exclude-operators stdlib_conversion_call_remove` restores the previous 53
operator selection. Saved plans retain their serialized selections; generate a
new plan to opt into these candidates. Existing plan validation still applies;
this does not promise arbitrary cross-version plan/session acceptance.
No report/schema/default resource limit or package layout is changed.

Existing Lean models `ConversionCallRemove.lean` and
`DefaultOperatorSelection.lean` describe argument evaluation and selection,
respectively. They abstract over conversion types and do not verify Python's
Decimal/Fraction implementations or the Rust import scanner. Real CPython and
hoimin tests check the example correspondence; the arbitrary alias/count/prefix
property is bounded testing, not a proof of Python scope resolution.

## Implementation and five self-reviews

1. **Shared knowledge:** extracted the existing import scanner into
   `CallableImports`. The `operator` client still owns its destination table and
   replacement spelling. Compared the extraction with the base implementation
   and retained the same binding, class-lookup and cancellation checks.
2. **Eligibility:** reviewed exact module/member pairs, source order, aliases,
   rebinding and syntax roles. Added negative cases for other members of the
   allowed modules and a positive case using both modules together.
3. **Observable behavior:** public CLI tests check source spans and CPython
   compilation. Real `plan` then `verify` runs show value equality survives and
   a runtime type assertion kills the Decimal and Fraction mutations. A separate
   execution check establishes that the argument is evaluated once.
4. **Registration:** reviewed default selection, independent exclusion, preserved
   legacy selections, behavioral ranking, the operator inventory, CLI reference
   and all five version declarations. The compiler caught the initially missing
   exhaustive ranking arm; it was added before the passing focused checks.
5. **Design cost:** the shared module hides import-evidence checks; each client
   controls its supported calls. This adds a scan when the new operator is
   selected, without a claim of improved analysis speed. Broader constructor
   inference and a configurable registry would add unsupported identity rules.

An independent reviewer found no blocking issues in the implementation and
suggested the non-target-member and mixed-module tests above; both were added.
A final review caught the current floor in `docs/releases.md`; it now says 0.5.0.

## Verification and five self-reviews

1. **Detection:** the new public CLI test first failed because the operator was
   unknown. After implementation, all six dedicated integration tests passed,
   including both real mutation outcomes, eight import forms, 25 excluded input
   fixtures, and the mixed-module case. The proptest runs 24 generated cases for
   aliases, candidate count, capped prefixes and rebinding exclusion.
2. **Regression:** `cargo test --locked --workspace --no-fail-fast` reported
   2,823 passed, one failed and 29 ignored. The failure was the ranking test's
   explicit operator inventory; added the new behavioral ID and changed its
   expected size to 73. The subsequent `cargo test --locked --workspace --lib`
   passed 862 tests with 20 ignored. The production ranking arm needed no change.
3. **Contracts and models:** `cargo test --locked --workspace --features contracts
   --no-fail-fast` passed 2,812 tests with 29 ignored across 150 top-level test/doc
   targets. Counts exclude child-process test summaries. `lake env lean
   ConversionCallRemove.lean` and `lake env lean DefaultOperatorSelection.lean`
   passed in `formal/HoiminOracle`; their abstraction boundary is stated above.
4. **Release and documentation:** the release/version/PyPI tests passed 92 cases;
   documentation tests passed 16. Initial Python collection lacked dependencies,
   and two release subprocess checks could not access uv's cache in the sandbox.
   Installing dependencies in a temporary environment and rerunning with cache
   access resolved those environment failures without test changes. Reviewed
   rollback commands, generated help, version floors and saved-selection wording.
5. **Scope and evidence:** independently reviewed the final diff and checked
   historical claims separately from current results. OKF YAML/reserved-file
   checks cover 48 Markdown pages; the new source and footnote resolve. The
   existing analyzer concept links the record; no new concept/index is needed.
   Validation is local macOS arm64 evidence, not Windows/Linux execution or a
   proof of runtime import identity. No wheel build or external serializer suite
   was performed for this change.

Clippy initially rejected the expanded collector initializer at 104/100 lines.
Replaced the repeated seven-operator OR chain with the same ordered list and
`any(contains)`, preserving short-circuit behavior. An independent review checked
that equivalence. After this final production edit, the contracts-enabled CLI
library and nine relevant integration targets passed 891 tests with 18 ignored:
`stdlib_conversion_call_remove`, `conversion_call_remove`, `string_literal_empty`,
`string_segment_empty`, `optional_keyword_delete`, `method_call_remove`,
`container_element_delete`, `default_operator_selection`, and
`operator_function_contracts`. The entire contracts workspace was run before
this equivalent initializer edit; it was not repeated afterward.

Final static/replay checks passed:

- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`.
- `cargo clippy --locked --manifest-path fuzz/Cargo.toml --bins -- -D warnings`.
- Workspace and fuzz formatting checks.
- `cargo +nightly-2026-10-08 fuzz run --dev --jobs 1 --target-dir target/fuzz
  python_analyzer fuzz/seeds/python_analyzer -- -fork=0 -runs=0 -max_len=4096
  -timeout=5 -rss_limit_mb=1024`: 11 seed files, 12 executions, no failure.
  This is seed replay, not an extended fuzz campaign.
- `cargo run --locked -p hoimin-cli --example generate_cli_reference -- --check`.
- Strict documentation site build and `git diff --check`.

## Resource observations

The monitor stopped no command. Peak sampled process-group RSS was 1,872,789,504
bytes during the first workspace run (334.4 s). The contracts workspace run took
351.1 s with 1,717,043,200 bytes peak sampled RSS. Final targeted tests took 45.1 s;
workspace Clippy 14.3 s; fuzz Clippy 16.4 s; seed replay including compilation
28.8 s; generated-reference check 6.2 s. The task-owned target reached about
8.4 GiB before commit hooks. Samples are observations, not hard memory limits or
complete child-process lifetime measurements.
