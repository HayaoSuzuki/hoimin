# Python Exception Mutation Operators — Design

Date: 2026-08-06  
Status: Approved design

## Goal

Add Python exception mutations that expose incorrect exception handling while
avoiding broad, accidental swallowing of termination and cancellation signals.
The first implementation targets `except` clauses. Mutation of `raise` expressions
will use the same infrastructure in a later issue.

## Scope

The default operator mutates only simple, unqualified exception names in an
`except <name>:` clause. It replaces a name with its curated counterpart and
does not rewrite the handler body, comments, whitespace, or unrelated syntax.

The default curated pairs are:

| Failure category | Pair |
| --- | --- |
| invalid input | `ValueError` ↔ `TypeError` |
| missing lookup | `KeyError` ↔ `IndexError` |
| missing object/key member | `AttributeError` ↔ `KeyError` |
| filesystem | `FileNotFoundError` ↔ `PermissionError` |
| connection | `ConnectionError` ↔ `TimeoutError` |
| imports | `ImportError` ↔ `ModuleNotFoundError` |
| numeric failure | `ZeroDivisionError` ↔ `OverflowError` |

The safe implementation skips exception tuples, qualified names such as
`module.Error`, calls and other dynamic expressions, and all `except*` clauses.
The opt-in tuple operators support only tuples made entirely of simple,
unqualified exception names. Every operator skips names that may be shadowed by
an assignment, parameter, import, pattern capture, or other file-level binding.
This conservative policy avoids turning a user-defined exception into a
built-in exception by accident.

## Operators and configuration

The safe default operator is `exception_type_pair`. It is part of the runtime
default selection and is exposed through the `exception_ops` selector.

The following operators are opt-in only and are grouped under
`exception_risky`:

- `exception_bare_to_exception`: `except:` → `except Exception:`
- `exception_exception_to_bare`: `except Exception:` → `except:`
- `exception_base_boundary`: `except Exception:` ↔ `except BaseException:`
- `exception_tuple_add_pair`: add a missing curated counterpart to a simple
  exception tuple
- `exception_tuple_remove_member`: remove one member from a tuple containing at
  least two simple exception names

The risky family is never part of the runtime default. Its documentation and
CLI help explicitly warn that the BaseException boundary can catch
`SystemExit`, `KeyboardInterrupt`, and `GeneratorExit`, and that broadening a
handler can change cancellation and shutdown behavior. Individual risky
operators remain selectable by their canonical IDs.

No operator generates an individual `SystemExit`, `KeyboardInterrupt`, or
`GeneratorExit` candidate. Dynamic tuple members and unsupported `except*`
syntax remain excluded even when a risky selector is requested.

## Architecture and data flow

The Rust Python analyzer will extend its AST candidate pass with an
`ExceptHandler` visitor. It will inspect the handler's `type_` expression and
emit a candidate only when the expression matches the supported shape and the
requested operator is selected. The safe replacement spans only the exception
name, so source trivia remains untouched.

Bare-handler and tuple mutations use token-aware source ranges to preserve
parentheses, commas, comments, and line endings. Candidate construction must
continue to use the existing one-span replacement and parseability checks.

Exception names are tracked with the analyzer's conservative file-wide binding
facts. A possible shadowing binding suppresses only the affected exception name;
it does not disable unrelated exception candidates in the same file.

Candidates flow through the existing selection, line/symbol filtering, limit,
ordering, cancellation, persistence, and JSON reporting paths. No new runtime
execution or report schema is required.

## Error handling and safety

- Parse errors produce no exception candidates, as with other AST mutations.
- Unsupported handler shapes are silently skipped rather than guessed.
- Replacements are validated by applying them and reparsing the resulting
  Python source in analyzer tests.
- No source-wide reparsing is added to the production candidate traversal.
- The risky family is explicit-only and documented as behavior-changing.

## Testing

Rust analyzer tests will cover:

- every curated pair in both directions;
- exact source spans and source-order candidate output;
- reparsing every emitted safe and risky candidate;
- shadowed exception names and independent candidates in the same file;
- exclusion of tuples, qualified/dynamic expressions, and `except*`;
- bare, BaseException-boundary, tuple-add, and tuple-remove selection gates;
- line/symbol filters, candidate limits, cancellation, and default-vs-explicit
  operator selection.

CLI/configuration contract tests will cover canonical IDs, selectors, defaults,
unknown-name diagnostics, help output, and JSON candidate records. README and
development documentation will describe the safe default, risky opt-ins,
curated pairs, exclusions, and the rationale for never generating individual
termination exceptions.

## Non-goals

- Mutating `raise` statements in this implementation.
- Inferring user-defined exception inheritance or runtime control flow.
- Mutating arbitrary expressions in `except` clauses.
- Enabling risky exception operators by default.
