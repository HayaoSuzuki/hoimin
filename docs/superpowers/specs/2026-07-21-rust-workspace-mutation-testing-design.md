# Rust Workspace Mutation Testing Design

## Goal

Use `cargo-mutants` to mutation-test the complete hoimin Rust workspace
(`hoimin-core` and `hoimin-cli`), strengthen tests until no survivor remains,
and document a repeatable developer workflow. A mutation that is demonstrably
equivalent or cannot be safely exercised by the test harness may be excluded,
but every exclusion must be scoped to that exact mutant and explain why.

This is a local quality workflow. It is not a CI check.

## Scope

- Add a checked-in cargo-mutants configuration at `.cargo/mutants.toml`.
- Run mutation testing from the virtual workspace root for every workspace
  package and run every workspace test suite against every mutant.
- Add or improve existing Rust tests to kill every non-equivalent survivor.
- Record individual equivalent or operationally untestable mutants in the
  configuration with a reason.
- Ignore cargo-mutants' generated output directories.
- Document installation, iterative execution, diagnosis, and final
  verification in `docs/development.md`.

## Non-goals

- Adding mutation testing to GitHub Actions or any other CI system.
- Changing hoimin's public CLI, output schemas, or supported runtime behavior
  solely to serve mutation testing.
- Broadly excluding a module, function family, or trait implementation merely
  to make the report green.
- Treating compiler-unviable mutants as test failures. They are retained in
  cargo-mutants output for diagnosis but do not indicate a coverage gap.

## Configuration and Execution

`.cargo/mutants.toml` is the canonical project configuration. It sets
`test_workspace = true`, so each mutant is tested with all workspace tests,
including cross-crate and CLI integration tests. The repository is a virtual
workspace with no default members, so running `cargo mutants` from its root
selects both member crates. The configuration must not set `in_place`; the
tool's scratch-copy default protects the developer's checkout.

The standard commands are:

```console
cargo install --locked cargo-mutants
cargo mutants
```

During an improvement loop, use:

```console
cargo mutants --iterate
```

`--iterate` reuses caught and unviable outcomes from the preceding run to make
the next diagnosis cycle shorter. It is only an iteration aid. The acceptance
run is always `cargo mutants` without `--iterate`, so source edits cannot hide
a regression behind stale outcomes.

`mutants.out`, `mutants.out.old`, and any same-prefix generated output are
ignored through `/mutants.out*`. They are local diagnostics, not versioned
reports.

## Survivor Triage and Improvements

For each entry in `mutants.out/missed.txt`, inspect its corresponding diff and
log, then classify it before changing code:

1. **Test gap**: add the smallest test at the existing unit, integration, or
   E2E layer that observes the changed behavior. Prefer assertions about public
   results, errors, and persisted/report data rather than implementation
   details.
2. **Testability boundary**: make a small, behavior-preserving refactoring
   only when it is necessary to make the observable contract testable. Do not
   expand the public API or change command output only for a test.
3. **Equivalent mutant**: exclude only the exact mutant after documenting why
   no supported input can distinguish it from the original code.
4. **Timeout or harness limitation**: first remove the cause with deterministic
   test control or a safe implementation change. If one exact mutant remains
   inherently unsafe to execute, document and exclude that exact mutant.

An equivalent or untestable case is stored in `.cargo/mutants.toml` under
`exclude_re`. Each rule is an anchored regular expression matching the complete
mutant name emitted by cargo-mutants, immediately preceded by a TOML comment
that states the equivalence or limitation. Anchoring makes exclusions fail
closed: moving or materially changing the mutant causes it to reappear for
review. Broad regular expressions, path-level exclusions, and function-level
skip attributes are not used for individual cases.

## Outcomes and Failure Handling

The final run is successful only when:

- the clean baseline succeeds;
- no `missed` (surviving) mutants remain;
- no mutant times out; and
- cargo-mutants itself reports no execution error.

`caught` is the desired outcome. `unviable` means the generated program did
not compile and is inconclusive about test coverage; it is reported but does
not block completion. A baseline failure, tool error, or timeout is investigated
and corrected before repeating the run rather than being accepted as a green
result.

## Documentation and Verification

`docs/development.md` describes installation, the normal and iterative
commands, how to inspect `mutants.out`, the classification rules, the narrow
exclusion policy, and the required final clean run.

After every group of test or implementation changes, run the affected Rust
tests. Before handoff, run the existing workspace quality gate:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin build --release
uv run python tests/wheel_smoke.py
```

Finally, run `cargo mutants` without `--iterate` from the workspace root and
verify that its missed and timeout result lists are empty. CI remains unchanged;
developers invoke this workflow deliberately because equivalent-mutant review
requires human judgment.
