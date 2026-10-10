# Issue #748 CLI reference review

Base: `1e60e54813dad14f124635496ed773b299bb4b90`.
Branch: `fix/issue-748-cli-reference`.

## Design self-review

1. Source of truth: use the same `root_command()` as parsing and completions;
   do not duplicate the five public command definitions in a documentation schema.
2. Coverage: recurse through visible commands, include long help and argument
   metadata, and exclude hidden arguments, commands, aliases and enum values.
3. Determinism: fix help width and disable color, preserve definition order,
   use UTF-8/LF and exclude dates, executable locations and invocation directories.
4. Constraints: public Clap reflection exposes arity, required flags/groups,
   conflicts and delimiters. Conditional `requires` and numeric parser bounds are
   not generally introspectable through stable Clap getters; keep their existing
   help and manual contract explanations, and document this limitation.
5. Ownership: keep generated Markdown in hoimin alongside its definitions.
   The suggested personal MkDocs site contains articles/books/works and has no
   hoimin documentation import mechanism. A second generated copy would add
   synchronization work; this issue explicitly requires standalone Markdown.

## Implementation-plan self-review

1. API scope: expose the existing command factory for a Cargo development
   example; keep the generator out of the shipped command/subcommand surface.
2. Sequence: write focused failing renderer tests first, implement rendering,
   then add actual-command and check-mode tests before wiring CI.
3. Drift detection: check committed reference contents, treating CRLF as LF
   because Windows checkout uses `core.autocrlf=true`; never rewrite in check mode.
4. CI selection: include `docs/cli-reference.md` in Rust selection so edits to
   generated output alone cannot bypass the check. Other handwritten docs retain
   their existing selection behavior.
5. Validation and resources: run generator tests, CLI regression tests, strict
   fmt/clippy and workflow lint; prove repeated generation and intentional stale
   input detection. Inspect free space and clean Cargo build output periodically.

## Implementation self-review

1. Shared definition: inspected `parse_from`, `write_completions`, and the Cargo
   example; all use `root_command()`. No shipped subcommand or parser changed.
2. Visibility: tested hidden commands/arguments, hidden enum values, private
   aliases and nested public commands. Added only visible alias metadata.
3. Output and constraints: compared all five real command sections with their
   definitions, including plan disk defaults, verify selection group, policies,
   shell enum, long help and trailing test argv. Documented reflection limits.
4. Check/write separation: missing and stale documents fail without writing;
   Windows CRLF checkout is accepted. The output path is based on the crate
   manifest directory, not the invocation directory. Regeneration writes LF.
5. CI and maintainability: generated-document-only edits select Rust checks.
   Strict Clippy caught a 104-line function; split command/argument/group rendering
   rather than suppressing the lint. Independent review found no correctness
   findings; removed its noted extra blank line in the development guide.
6. Staged-output review found 40 whitespace-only Clap help lines and an extra
   EOF blank line. The earlier unstaged diff check had omitted the untracked
   generated document. Added a failing whitespace regression assertion, then
   normalized line ends in the renderer and regenerated the document.

## Test self-review and evidence

1. Red/green behavior: the initial three renderer tests failed on missing output;
   the check-mode test then failed on its unimplemented stub. All seven final
   generator tests passed after implementation.
2. Freshness regression: tests independently change help, defaults, and add an
   option against the original reference. Each must fail check, then pass after
   regeneration. Missing documents and unchanged CRLF files cover non-writing.
3. Determinism and Markdown: two actual regenerations matched SHA-256
   `3f3ebc61c902b1153bbf12af99bcc56508beb72d6e0ec9c1d41786fa3cd89e75`.
   Tests check absence of CR/ANSI,
   nested command names, hidden values, and literal Markdown fences in help.
4. Integration and lint: CLI library tests: 803 passed, 12 ignored. CI
   classification/workflow and existing ranked documentation tests: 80 passed.
   Strict CLI all-target/all-feature Clippy, Rust formatting, Python Ruff and ty
   passed. Workflow actionlint/ShellCheck/zizmor completed with no findings;
   zizmor reported its existing unsupported-Python-shell warnings and three
   pre-existing scoped ignores.
5. Scope and evidence: regression tests first detected both missing CI wiring
   and the doc-only bypass, then passed after the fix. Windows execution is
   verified locally; Linux execution awaits hosted CI.
6. After whitespace normalization, all seven generator tests passed again,
   repeated actual generation matched the final hash above and `--check` passed.
   The added assertions prohibit trailing whitespace and multiple EOF newlines.

## Mutation and disk evidence

- Current working-tree CLI built with `cargo build --locked -p hoimin-cli --bin hoimin`.
- Focused plan for `tools/ci_selection.py:28-29`: 4 candidates, one equality
  mutation selected (`==` to `!=`). `tests/test_ci_selection.py` killed it:
  killed 1, survived 0, complete true, exit 0. This is a targeted check, not a
  mutation score for all Python code.
- Plan/verify used jobs 1, maximum owned workspace 8 GiB, free-space reserve
  10 GiB. Peak owned bytes 38,566,572; minimum free bytes 183,688,736,768.
  Worker cleanup succeeded; the exact external temporary report directory was
  removed in `finally` and its absence checked.
- First `cargo clean` removed 6,852 files (5.7 GiB); free space afterwards was
  189,259,829,248 bytes. The whitespace regression required a subsequent rebuild.
- After the final tests and strict Clippy passed, the second `cargo clean`
  removed 4,937 files (2.5 GiB), leaving 189,258,199,040 bytes free.

## Document verification

The final local check parsed all 34 OKF Markdown pages, checked reserved-file
structure, changed source hashes, footnotes, relative links and index reachability.
Ruff format checked all 43 Python files. `git diff --cached --check` passed for
all 14 staged files, including the new generated document. Historical
OKF metadata unrelated to this change was preserved. Consulted the overview,
selection/plan/verify and CI concepts; added the CLI reference playbook and
updated the CI selection contract. Public execution contracts did not change.
