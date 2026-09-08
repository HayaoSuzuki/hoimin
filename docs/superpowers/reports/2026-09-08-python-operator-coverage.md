# Python operator coverage delivery report

## Delivered scope

Hoimin now exposes 43 default runtime operator selectors and 55 operator IDs
across the full catalog. The added runtime coverage includes nine syntax
selectors for power, matrix multiplication, bitwise xor/invert, and their
augmented forms. The `operator_function` selector covers the Python 3.13
`operator` callable inventory defined in the specification.

The new integration suite contains seven tests and 21 behavior cases. Each case
asks the public `hoimin plan` command to analyze a temporary Python module,
applies the candidate's reported byte span and replacement, and runs the original
and mutated module with CPython. Literal baseline and mutant outputs verify
numeric results, special-method dispatch, argument order and frequency, target
side effects, positional-only failures, and callable use through from-import,
dunder, and higher-order references. The harness lives outside the analyzed
module.

The default helper interpreter is the repository `.venv`. The helper accepts an
absolute `HOIMIN_OPERATOR_TEST_PYTHON` override so the same contracts can run on
Python 3.13 without changing the package's Python version floor.

## Conservative function scope

Hoimin trusts a unique, unmodified, unconditional module-level `operator` import
or absolute from-import. It skips bindings affected by shadowing, deletion,
wildcard or conditional imports, dynamic namespace access, module attribute
writes, and uncertain `__import__` identity. It also skips relative and local
imports.

Stores, annotations, and match patterns do not produce callable candidates;
match guards remain expression sites. Hoimin excludes import aliases beginning
with `__` throughout a class definition, including methods and nested functions,
because Python can mangle private names or supply class names that the AST does
not expose as assignments. Ordinary aliases and member references such as
`op.__add__` remain eligible in class code. The analysis works within one module,
assumes a normal standard-library import, and does not resolve project import
search paths, custom loaders, or external monkey-patching.

## Sensitivity evidence

The executable assertions passed against the completed implementation before
the sensitivity checks. Three temporary production mutations then produced the
expected behavioral failures:

- Changing syntax `**` replacement from `*` to `+` failed
  `syntax_mutants_change_results_and_special_method_dispatch`: the mutant printed
  `[5,[2,3]]` instead of the literal `[6,[2,3]]`.
- Changing `operator.pow` from `mul` to `add` failed
  `operator_function_arithmetic_and_matrix_protocol_replacements_execute` with
  the same independent numeric mismatch.
- Changing the `contains` lambda from `item not in container` to
  `item in container` failed
  `contains_and_setitem_mutants_keep_argument_order_and_remove_effects`: the
  mutant retained `true` instead of producing `false`.

All three commands exited 101. The production files returned to their committed
Task 1 and Task 2 contents before the final passing runs.

## Verification record

Local platform: macOS 15.7.7 (24G720), arm64; rustc 1.98.0; Cargo 1.98.0;
controlled CPython 3.14.7. Every Cargo command used
`CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target`.

| Command | Result |
| --- | --- |
| `cargo test -p hoimin-cli --test operator_function_contracts -- --test-threads=1` | 7 passed, 0 failed |
| `cargo test -p hoimin-cli --test cli_config readme_documents_all_mutation_operator_ids_and_selector_families -- --exact --test-threads=1` | 1 passed, 0 failed, 54 filtered out |
| `cargo test --workspace --all-features -- --test-threads=1` | Exit 0; 1,548 passed, 0 failed, 12 ignored across 1,560 registered top-level tests |
| `cargo test --workspace --all-features -- --list` | Exit 0; 1,560 registered top-level tests |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Exit 0, no warnings |
| `cargo fmt --all -- --check` | Exit 0 |
| `git diff --check` | Exit 0 |

The controller ran `.venv/bin/python -m unittest discover -s tests -q` before
this task. It exited 0 after 862 tests with 56 platform skips in 43.105 seconds.
Task 3 did not rerun that unchanged Python suite.

## Skipped and pending checks

No production Python changed, and the repository mutation-testing policy forbids
mutating test modules, so this task did not run Hoimin against the Python test or
fixture modules. The controller owns the Python 3.13 override run, MSRV check,
independent whole-branch review, push, pull request, and final-head CI
observation. No Windows or GitHub actions ran during this task.

Task 1 and Task 2 received focused implementation review before this delivery
work. The whole-branch review outcome remains pending at this commit. The final
review should examine the documented conservative boundary for custom
metaclasses whose `__prepare__` method supplies a class-namespace binding that
shadows an ordinary module alias.
