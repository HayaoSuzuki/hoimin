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

## Extension requested on the same PR

Starting from `5a6312f8543d11615ccd48fc190e08ec08187665`, the user requested
Decimal/Fraction factory methods and concrete standard-library integer enums in
PR #859. The initial constructor-only scope and results above describe that
first revision. The final supported list is in `docs/usage.md`.

Added five factories: `Decimal.from_float`, `Decimal.from_number`,
`Fraction.from_float`, `Fraction.from_decimal`, and `Fraction.from_number`.
Added 19 concrete `IntEnum`/`IntFlag` types from `http`, `socket`, `signal`, `ssl`,
`re`, `calendar`, `inspect`, and `annotationlib`. This is an explicit list, not
inference over user-defined subclasses or an exhaustive standard-library enum
registry. `ctypes` is excluded; the user's preference is opt-in if added later.
The same operator ID and opt-out cover all of these calls; counts remain 54
(default) and 73 (total), with the same 0.5.0 floor.

Reviewed the Python 3.14 documentation for [decimal factories](https://docs.python.org/3.14/library/decimal.html#decimal.Decimal.from_number),
[Fraction factories](https://docs.python.org/3.14/library/fractions.html#fractions.Fraction.from_number),
[HTTPStatus](https://docs.python.org/3.14/library/http.html#http.HTTPStatus),
[socket constants](https://docs.python.org/3.14/library/socket.html#constants),
[signal enums](https://docs.python.org/3.14/library/signal.html#module-contents),
[ssl constants](https://docs.python.org/3.14/library/ssl.html#constants),
[RegexFlag](https://docs.python.org/3.14/library/re.html#re.RegexFlag),
[calendar enums](https://docs.python.org/3.14/library/calendar.html#calendar.Day),
[BufferFlags](https://docs.python.org/3.14/library/inspect.html#inspect.BufferFlags),
and [Format](https://docs.python.org/3.14/library/annotationlib.html#annotationlib.Format).
The two `from_number` methods are available in Python 3.14, already the project's
required Python version. Enum members depend on the target platform; `Sigmasks`
is Unix-only. No socket, signal handler or TLS connection is created by the tests.

### Extension design: five self-reviews

1. **Scope:** enumerate the five methods and 19 concrete enum types; reject
   similarly named unsupported members, enum bases, string enums and ctypes.
2. **Identity:** preserve strict candidate resolution. Retain all recognized
   import origins separately for hazard detection, including duplicate, rebound,
   local, pre-import and class-scope references that strict resolution rejects.
3. **Mutation evidence:** block factories if the class object can escape via an
   alias, callback, extracted bound method or annotation, or be written through.
   Constructor/factory callee references alone are allowed; their arguments are
   still traversed. Decimal and Fraction hazards are tracked independently.
4. **Observable result:** test returned type and argument evaluation through
   actual saved-plan verification. Numeric equality alone is deliberately weak;
   type assertions are the distinguishing observation.
5. **Complexity:** `ConversionImports` owns allowed call identities and factory
   stability; `CallableImports` owns import syntax/origins. The collector retains
   arity, source-span and syntax-role rules. Philosophy diagnostics for this new
   boundary: 8/8 (10/10); no failed row in this limited review. This is not a score
   for the entire analyzer or a proof of complete Python provenance tracking.

### Extension plan: five self-reviews

1. **Red:** require the public CLI to emit the new calls through four import
   forms; both new test groups must fail before implementation.
2. **Implementation:** extend the import evidence privately and introduce the
   conversion-specific boundary; keep the existing operator-function client
   unchanged and preserve cancellation checks.
3. **Properties:** extend generated alias/prefix/rebinding cases to factories and
   enums, then exercise actual weak/strong tests for each supported callable.
4. **Regressions:** run the workspace suite, contracts checks, strict Clippy,
   existing operator-function tests, fuzz seed replay and documentation checks.
5. **Delivery:** update this PR's title/body to the final scope, preserve the
   explicit remote branch destination, and record verification after review.

### Extension implementation: five self-reviews

1. **Alias hazards:** independent design review identified class-scope writes,
   writes before textual imports, duplicate/rebound aliases and extracted bound
   methods as cases strict `resolve` cannot detect. `may_reference` retains those
   possible origins but can never make a candidate eligible itself.
2. **Whitelist and roles:** tests cover 20 factory import forms and 76 enum import
   forms, exact original/replacement spans and CPython compilation. Negative
   fixtures cover unsupported classes/methods, keywords, unsafe expressions,
   annotations, assignment targets and local custom enum subclasses.
3. **Evaluation:** all five factories and 19 enums are exercised by real
   `plan`/`verify` weak and strong assertions on this macOS host. Each assertion
   also checks one argument evaluation. The Unix-only Sigmasks execution case
   is excluded on Windows, while its static AST forms remain tested there.
4. **Selection:** existing default/exclusion coverage now requires constructor,
   factory and HTTPStatus candidates and verifies their collective opt-out.
   Extended the existing bounded proptest and the shipping-analyzer fuzz seed.
5. **Compatibility and CI:** the first PR revision's dependency-policy CI failed
   because `deny.toml` still scoped the repository's Elastic-2.0 exceptions to
   `>=0.4.0,<0.5.0`. Updated only hoimin-core/hoimin-cli ranges to `>=0.5.0,<0.6.0`;
   other licenses and third-party exceptions are unchanged. Updated release
   instructions to include this minor-floor step.

Factory safety remains conservative syntactic evidence. It does not track class
recovery through constructed instances (for example `type(Fraction(0))`) or
arbitrary interprocedural dataflow, and cannot exclude external monkeypatching
or import-path shadowing. These cases were explicitly set outside this extension
in review; direct import-alias hazards are covered. Plain class references in
annotations can suppress otherwise safe factory candidates. Direct constructors
retain their earlier eligibility contract.


### Extension verification: five self-reviews

1. **Red/green evidence:** both new import-form tests failed with zero candidates
   before implementation. After implementation, all 13 conversion integration
   tests passed, including real weak/strong verification for every supported
   method/enum, 96 new import forms, exact spans, mutation hazards, exclusion
   boundaries and bounded proptest cases.
2. **Regression scope:** the complete default-feature workspace run passed 2,831
   tests with 29 ignored across 150 top-level test/doc targets. Counts exclude
   child-process summaries. This run preceded the final origin-index refinement.
3. **Complexity review:** the reviewer identified a potential repeated linear
   search through alias origins. Replaced it with deduplicated module/member
   sets and borrowed lookups, preserving strict `resolve` unchanged. Final
   independent review found no remaining issue; instance/dataflow limits were
   explicitly retained. No measured speedup is claimed.
4. **Models and policy:** both existing Lean models passed again. Official
   cargo-deny 0.20.2 (release checksum verified) passed `--workspace --all-features
   --locked --config deny.toml check --deny warnings licenses sources` for both
   Cargo manifests. This confirms the correction of the first PR's CI failures.
5. **Documentation and delivery:** strict documentation build and 16 document
   tests passed. Reviewed the final target table, opt-out/default counts,
   platform restrictions, ctypes exclusion and preserved historical evidence.
   Final source checks and delivery results are recorded below.

After the final index refinement,
`cargo test --locked -p hoimin-cli --features contracts --lib
--test stdlib_conversion_call_remove --test default_operator_selection
--test operator_function_contracts --test analysis_depth --test valid_python_corpus`
passed 1,158 tests with 21 ignored across six targets. An initial invocation used
an incorrect integration-target name and was corrected before this test run.
The complete workspace was not repeated after the index-only refinement.
Strict workspace Clippy (all targets/features), fuzz Clippy, the 11-file seed
replay (12 executions), generated CLI reference check and formatting passed.
The fuzz command and bounds are the same as in the initial verification above;
the updated seed includes factories and enum constructors.

Extension resource observations: full workspace 335.1 s, 1,758,887,936 bytes peak
sampled process-group RSS; final contracts tests 59.5 s, 1,949,286,400 bytes;
final Clippy 12.3 s; fuzz Clippy 16.3 s; replay/build 28.7 s; reference check 6.2 s.
The task-owned target was about 5.0 GiB before commit hooks. No resource guard
fired. Independent final review found no remaining issue in the source, tests,
index refinement or documentation. All results remain local macOS evidence.
