# Generator mutation implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline; one fresh
> whole-branch reviewer after the targeted tests and before the full-suite run.

**Goal:** Detect missing assertions on values, reuse and evaluation of generators.
**Architecture:** Shared function_entry layout/own-scope utilities; generator
source builders and four opt-out AST operators. A noncolliding construction-site
resolution proof guards builtin next.
**Tech Stack:** Rust/Ruff AST, CPython3.14/3.15.
**Spec:** docs/superpowers/specs/2026-10-25-generator-mutations-design.md

## Global constraints

Four IDs/default61/catalog80 as the spec. No full drain, alias inference or generic
raise/return insertion. Retain original generator kind, construction evaluation,
comments/encodings/newline spellings and existing budgets. Five reviews each stage.

## Review focus

- An implicit Generator in a sole call argument must remain valid Python.
- next proof cannot overwrite existing first-element resolutions or mix nested
  generators sharing one end offset or ignore uncertain deferred bindings.
- Own-scope yield inspection must preserve both generator kinds and nested scopes.
- Async classification includes root conditions/targets/subsequent iterables and
  nested async computations, but root leftmost await is construction-time.
- Overlapping candidates must retain identical capped prefixes; selection and
  cancellation must avoid unnecessary large copies.

## Task1: Entry utilities and generator return

Files: analyzer/rust/function_entry.rs (new); iterator_stop.rs (reuse utilities);
generator_mutations.rs (new); analyzer rust.rs; core registry/default fixtures.
Interfaces: function_entry::inspect(definition, introduced_name, cancelled) returns
Option<ScopeFacts> with has_yield/declares_global; first_statement(definition)
returns Option<&Stmt>; insert(definition, source, tokens, inserted) returns
Option<(TextRange,String)>; insert also accepts cancelled.

- [x] Add missing-name RED and generator-kind/nested/default-yield/entry-layout tests.
- [x] Extract existing entry layout/inspection with its regression suite unchanged.
- [x] Register all four IDs; implement bare return insertion only for own generators.
- [x] Run new and inherited iterator/loop/core tests GREEN; five implementation reviews.

## Task2: Comprehension and expression mutations

Files: generator_mutations.rs, NameResolutionBuilder and AST collector in rust.rs;
pure analyzer tests and CLI ranking fixtures.
Interfaces: builders retain source ranges; synchronous-generator classification
inspects its executing scope. Construction facts are keyed by full range; end offsets retain conservative loop-back uncertainty.

- [x] Write RED for list→gen and skip/empty execution, implicit arguments,
  shadowing/walrus/marker collisions, async classification and known-empty inputs.
- [x] Implement guarded source builders; preserve original evaluation once.
- [x] Check selection before large copies and stop emission on admission failure.
- [x] Verify every cap against full ordered results and cancellation/payload failure.

## Task3: Public behavior and delivery

Files: CLI config/default/valid-python tests and registry; usage/compatibility/CLI
reference; spec catalog/analyzer OKF; maintenance evidence and fuzz seed.

- [x] Add all four operators to public opt-out fixture and registry.
- [x] Execute truthiness/reuse/lazy-evaluation, generator-kind, first-item/empty and
  factory-once fixtures with both CPython versions, including async returns.
- [x] Update docs/OKF/reference and run their checks.
- [x] Run targeted tests and five validation reviews, then one fresh whole-branch
  review; fix defects RED→GREEN before the full workspace and Clippy checks.
- [ ] Complete five delivery reviews, cleanup owned artifacts, commit/push and
  create/attach the stacked PR. Preserve the primary checkout's local .gitignore.
