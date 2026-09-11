# Python semantics and Rust mutation operators

## Scope and conclusion

This audit covers seven existing runtime operators selected from the Python
language-feature investigation. Their generation remains in Rust; validation
remains an external command. The investigation supports concrete behavioral
fixtures without adding audit hooks, monitoring callbacks, Python loaders, or
an embedded interpreter to hoimin.

No production defect was found in this bounded review. The missing evidence was
an end-to-end connection between generated candidates and protocol-sensitive
test assertions. `protocol_contracts_distinguish_generated_mutants_in_external_tests`
in `crates/hoimin-cli/tests/run_e2e.rs` now exercises that connection.

## Implementation and existing coverage

The implementation is in `crates/hoimin-cli/src/analyzer/rust.rs`; generation
regressions are in the adjacent `rust_tests.rs`. The documented generation
contract is in `docs/development.md`.

| Operator | Rust generation boundary | Existing regression coverage | Added external observation |
| --- | --- | --- | --- |
| `boolean_and_or` | Replace an AST-approved runtime token; exclude annotations | Exact token replacements and compound-expression spans | Both versions return `False`; only the mutant calls the right operand |
| `collection_any_all` | Supported bare builtin call; source and replacement must resolve to builtins | Exact parseable candidates; local and destination shadowing | Both return `True`; remaining iterator contents differ |
| `structure_mapping_get_subscript` | Simple receiver, supported key, one positional argument without default; subscript must be a load | Exact replacements, nested source preservation, unsupported forms | Present key survives; missing key on a dict subclass distinguishes `get` from `__missing__` |
| `structure_append_extend` | Supported append argument; inverse requires one non-starred list element | Exact replacements and preservation of nested expressions/comments | Plain list contents survive; explicit append/extend overrides distinguish dispatch |
| `structure_sorted_reversed` | One positional argument; builtin resolution required for both names | Exact replacements and shadowing exclusions | Descending input survives; mixed order distinguishes output |
| `structure_index_neighbor` | Decimal literal load index; increment and positive decrement | Decimal-only/load-only cases | Repeated elements survive; distinct adjacent elements distinguish both mutants |
| `structure_slice_neighbor` | Decimal bounds/step; never generate a zero step | Bound/step cases and zero-step exclusion | Empty input survives; nonempty prefix distinguishes both mutants |

Method selection is syntax-directed, without receiver type inference. These
fixtures do not establish that every matching call has the assumed protocol.
Existing tests cover generation restrictions; the new test covers selected
forward transformations through the real CLI, worker mutation, external
process, and JSON result. It does not exhaust inverse directions or all inputs.

## Why the observations matter

Python specifies short-circuit evaluation and operand return for `and`/`or`.
It also specifies that dictionary subscription can call `__missing__`, whereas
other dictionary operations do not. These supply observable contracts beyond
the usual present-key/value examples. See [Boolean operations](https://docs.python.org/3.14/library/stdtypes.html#boolean-operations-and-or-not)
and [dictionary operations](https://docs.python.org/3.14/library/stdtypes.html#mapping-types-dict).

The documented early returns of [any](https://docs.python.org/3.14/library/functions.html#any)
and [all](https://docs.python.org/3.14/library/functions.html#all) motivate the
remaining-iterator assertion. The fixture uses only true elements so the return
value alone cannot distinguish the mutation. Sorting and reversing likewise
need an input whose orders differ; compare [sorted](https://docs.python.org/3.14/library/functions.html#sorted)
and [reversed](https://docs.python.org/3.14/library/functions.html#reversed).

The subclass fixture deliberately defines a dispatch-sensitive API. Such a
test is appropriate only if that API is within the application's accepted input
contract. An API restricted to ordinary lists and resulting contents does not
gain a missing-test diagnosis from this example. A surviving append/extend
mutant in that domain may be equivalent with respect to its contract.

[PEP 578](https://peps.python.org/pep-0578/) defines runtime auditing, not a
mutation correctness oracle. Audit and monitoring remain outside this operator
implementation. They are unnecessary for the assertions above. Syntax parsing
alone also does not establish execution or type correctness: those belong to
the configured external Python/test/type-checker command.

## Validation and limits

Each fixture runs twice: a weak external assertion must report all generated
candidates as `survived`, and a stronger assertion must report them as `killed`.
Both runs require a complete report and the expected CLI exit status. There
are seven fixtures, nine candidates, and eighteen mutant executions across
fourteen runs. The original project source must remain unchanged.

Verified locally with the repository's CPython 3.14.7 interpreter:

| Check | Result |
| --- | --- |
| `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests` | 125 passed, 2 benchmark tests ignored |
| `cargo test -p hoimin-cli --test run_e2e` | 55 passed, including the new protocol cases |
| `cargo clippy -p hoimin-cli --test run_e2e -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `git diff --check` | Passed |

These are characterization and integration regressions for existing production
behavior, not a production fix with a failing-before/fixed-after claim.
The weak/strong pair checks that the observation actually discriminates the
generated mutation rather than merely testing Python in isolation.

This is not an audit of every runtime/type operator, a mypy validation, a
measurement of real-project equivalent-mutant rates, or evidence for changing
default operator selection. Those decisions still need representative project
data: accepted input contracts, candidate counts, invalid mutants, survivor
classification, and external-command cost.
