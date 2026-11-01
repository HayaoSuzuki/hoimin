# Issue #431 class comprehension lookup scopes

## Outcome

The `operator_function` analyzer now distinguishes the two lookup scopes in a
class-body comprehension. It visits the leftmost iterable in the enclosing
class scope, then visits the target, filters, later iterables, and produced
expressions in the comprehension's implicit function scope. The same traversal
applies to list, set, and dict comprehensions and generator expressions, and it
restores the inherited scope for nested comprehensions.

`OperatorImports::replacement` and the import identity rules were unchanged.
Module-wide rebinding, private-name, metaclass, dynamic namespace, and import
ordering safeguards therefore continue to reject uncertain references.

## Tests and TDD evidence

Analyzer regressions cover module and from-import aliases, all four
comprehension forms, dict keys and values, filters, later iterables, nested
comprehensions, the class-scoped leftmost iterable, a target that rebinds an
import alias, and private import aliases.

Before the production edit, the targeted analyzer command ran 16 tests. The 13
pre-existing tests passed and the three new tests failed with the expected
missing candidates. The minimal fixture returned only the lambda candidate on
line 4 and omitted the comprehension-body candidate on line 3.

The external contract uses the public plan path and byte-span replacement with
a real Python interpreter. A metaclass supplies an unrelated class-local `op`
binding for a direct class lookup and the leftmost iterable. The plan selects
only the comprehension body resolved through the ordinary module import. With
the implementation temporarily removed, the contract failed because the plan
returned zero candidates. With the implementation restored, the generated
`add` to `sub` replacement changed literal output from `[5]` to `[-1]`.

## Verification

- `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo test --offline -p hoimin-cli --lib operator_function`: 16 passed.
- `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo test --offline -p hoimin-cli --lib analyzer::rust::rust_tests`: 150 passed, 2 ignored benchmarks.
- `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target HOIMIN_OPERATOR_TEST_PYTHON=/Users/hayao/RustroverProjects/hoimin/.venv/bin/python cargo test --offline -p hoimin-cli --test operator_function_contracts`: 9 passed.
- `cargo fmt --all --check`: passed.
- `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo clippy --offline -p hoimin-cli --all-targets -- -D warnings`: passed.

The controller's initial sandboxed Python run had three failures and one error
because process inspection was denied. The five isolated guard tests passed
with elevation, and the elevated full Python gate passed with 862 tests run,
56 skipped, in 43.761 seconds. No source change was needed for that environment
constraint.

Controller verification on the committed implementation:

- `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target HOIMIN_OPERATOR_TEST_PYTHON=/Users/hayao/RustroverProjects/hoimin/.venv/bin/python cargo test --offline --workspace --all-features -- --test-threads=1`: 1,562 passed, 0 failed, 12 ignored across 66 result groups.
- `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-msrv-target cargo +1.88 check --offline --workspace --all-targets --all-features --locked`: passed.
- `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

Local platform is macOS arm64. Linux CI results will be available on the PR;
no Windows workflow was dispatched for this platform-independent visitor change.

## Limitations

The analysis remains intentionally conservative. It does not prove custom
import loaders, external monkey-patching, dynamic namespace mutation, or
metaclass provenance. The leftmost iterable keeps its enclosing lookup scope,
so a trusted operator reference there is still excluded inside a class body.
