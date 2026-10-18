### Spec Compliance

- ✅ Spec compliant. `crates/hoimin-cli/src/cli.rs:277` owns the existing shared raw arguments directly; the diff removes the duplicate raw field declaration and full transfer without aliases or forwarding APIs. `cli.rs:560`, `cli.rs:564`, and `cli.rs:695` preserve the shared validation and normalization boundary, including plan's fixed JSON/no metrics/no session/no resume values.
- ✅ Required behavior has direct characterization: shared run/plan normalization (`crates/hoimin-cli/tests/cli_config.rs:1774`), required-input ordering (`:1861`, `:1875`), invalid duration/byte diagnostics (`:1889`, `:1913`), native argv (`:1259`, `:1282`), invalid path/text encoding (`:1305`, `:1339`), and run-only restrictions (`:316`, `:399`, `:1940`). Timeout parsing and declaration syntax are unchanged in this diff.
- ✅ Four stages each record five distinct concrete self-reviews in `task-5-report.md:6`, `:24`, `:43`, and `:64`. The report explicitly records passing characterization before production edits (`:22`) and discloses the corrected test migration error (`:20`).
- ⚠️ Windows execution cannot be verified on this Unix host: the code-unit expectations are explicit, but Windows CI must execute the cfg-gated tests (`crates/hoimin-cli/tests/cli_config.rs:1282`; `task-5-report.md:69`). Candidate ordering, cancellation, and resource ownership implementations are outside this task's diff and receive no new assurance from this task review.

### Strengths

- `crates/hoimin-cli/src/cli.rs:639` isolates only borrowed required-input validation, retaining selector-before-argv precedence without introducing another ownership or conversion layer.
- `crates/hoimin-cli/src/cli.rs:695` keeps normalization expressions and their evaluation order intact; only composition destructuring and the two original raw fingerprint field names change.
- `crates/hoimin-cli/tests/cli_config.rs:371`, `:680`, `:688`, and `:697` replace raw layout assertions with observable behavior and separate formerly combined scenarios. The new finite command/option matrices keep their assertions in each test, label failing cases, and separate arrangement, execution, and assertion.

### Issues

#### Critical (Must Fix)

- None found.

#### Important (Should Fix)

- None found.

#### Minor (Nice to Have)

- None found.

### Assessment

**Task quality:** Approved.

**Reasoning:** The production change removes the requested duplication while preserving raw native values, required-input checks, run-only constraints, and the single core normalization boundary. Behavioral coverage addresses the concrete risks of the ownership refactor without copying the raw structure or introducing compatibility machinery.

- Evidence checked without rerunning suites: `/tmp/hoimin-design-audit-5d3aee95/task-5-baseline.log:83` and `task-5-cli-config.log:83` each record 75 passed; `task-5-core-config.log:18` and `:49` record 11 and 25 passed; `task-5-cli-reference-tests.log:14` records 7 passed. The reference freshness log contains no errors; `task-5-completions.log:1` records four successful nonempty shell outputs with empty stderr; `task-5-commit.log:1` records all four mandatory hooks passing. No warning/error matches appeared in the inspected logs.
- Review scope: reviewed the supplied diff, recovering only the initially truncated output segment; no changed source files were separately read, no broader codebase inspection or git commands were performed, and no unresolved concrete doubt warranted a new test run.
