# Task 2 independent review

Spec compliance: ❌ Issues found.
Task quality: Needs fixes.
Scope: base `b02ee7d51709de664ee88db68abca34025db3487`, head `3d9e33d178a451e39e182495f4a5bb3a4c5ebfa0`; Task 2 only.

## Strengths

- `crates/hoimin-cli/src/human_text.rs:21` centralizes control/line-separator projection while retaining readable Unicode and separate quoted formatting; the narrow baseline view implements R8 without modifying captured data.
- `crates/hoimin-cli/src/report/human.rs:92` and `crates/hoimin-cli/src/plan/preview.rs:130` project fields at presentation boundaries; serialization branches and logical paths are untouched by the production diff.
- `crates/hoimin-cli/tests/report_handler.rs:2539` supplies literal independent expectations, and `crates/hoimin-cli/src/human_text.rs:155` plus `crates/hoimin-cli/tests/report_handler.rs:2742` exercise broad single-line invariants. Public preview tests check real filenames, command markers, and persisted-plan bytes (`crates/hoimin-cli/tests/plan.rs:5718`).

## Important findings

1. **An external executable name still injects controls into Clap Usage.** `crates/hoimin-cli/src/human_text.rs:69` sanitizes exposed String/Strings and Suggested contexts, but Usage is an already-built StyledStr. The command factory (`crates/hoimin-cli/src/cli.rs:638`) fixes the command name without fixing `bin_name`, so Clap derives Usage's executable name from external argv[0]. A focused check against the existing task binary:

   ```python
   subprocess.run(
       ["hoimin\x1b[31m\r\n\t", "bad"],
       executable="/tmp/hoimin-priority-six-target/debug/hoimin",
       capture_output=True,
       timeout=15,
   )
   ```

   exited 2 and emitted `b"error: unrecognized subcommand 'bad'\n\nUsage: hoimin\r\n\t <COMMAND>\n\nFor more information, try '--help'.\n"`. The CR/LF/TAB inside the executable name are raw input-origin controls, violating the human diagnostic boundary requirement. Sanitize the executable-name input before building Usage, or establish the intended trusted `bin_name`; preserve Clap's generated layout as R7 requires. Add generated cases for hostile executable names covering errors/help as appropriate. Do not escape the complete Usage document.

2. **The new example tables run multiple cases inside a single test rather than generating the required parameter cases.** Examples: `crates/hoimin-cli/tests/plan.rs:5719` and `:5737` nest filename and format loops; `crates/hoimin-cli/tests/plan.rs:5794` loops distinct malformed-argument routes; `crates/hoimin-cli/tests/report_handler.rs:2556`, `:2587`, `:2610`, `:2642`, `:2689`, `:2795` loop value/format/event cases; `crates/hoimin-cli/tests/review_cli.rs:526` loops filenames. The binding global constraint explicitly requires one behavioral case per test function or **generated parameter case**, with parameterization of the same contract. These hand-written loops stop all subsequent cases on the first failure and do not expose separate cases to the runner. Convert them, including the baseline ordinary/special-code table at `crates/hoimin-cli/tests/report_handler.rs:2758`, into generated parameter cases. Preserve the independent expected strings and meaningful Arrange/Act/Assert structure; the property tests are appropriate broad invariants and need not become example tables.

## Focused checks and evidence limits

- Read the supplied diff once; its initial tool response truncated the middle, so only that missing range was recovered. No changed source file was separately opened.
- Named risk: unsanitized parser source messages could bypass context projection. Checked `cli.rs` parser declarations, which use built-in numeric/value-enum Clap parsers, and made one focused `progress --patience` malformed-number call with ESC/CR/LF/TAB. Its value was visibly escaped and its numeric source message was static; no finding for that route.
- Named risk: Usage's executable-name provenance could bypass projected contexts. Checked `cli.rs:14`, `:636`, and `:674` plus the existing Clap command API location; the focused argv[0] call above establishes the remaining raw-control route.
- Inspected existing final-test, clippy and commit logs for warnings/errors/failures and final test counts. Logs show the reported 1,055 passing top-level tests, 26 ignored cases, and no warning/error matches. Did not rerun reported passing tests or invoke Cargo; no heavy/new Cargo validation requested.
- The report records five concrete requirement, implementation and validation passes and RED evidence. Whole-branch isolation, untouched production Creusot behavior, final integration/catalog consistency, and native Linux/Windows execution cannot be established by this task diff; the controller owns those checks. Native macOS results do not establish other platforms. Existing binary checks rely on the task's reported build artifact rather than a reviewer rebuild.

Report: `.superpowers/sdd/2026-10-19-priority-six/task-2-review.md`.
