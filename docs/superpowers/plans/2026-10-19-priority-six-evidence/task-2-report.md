# Task 2 report: shared human-output escaping (#1002)

Status: complete, committed, and ready for the controller’s independent Task 2 review.
Commit: `3d9e33d178a451e39e182495f4a5bb3a4c5ebfa0` (`fix(cli): escape external human output fields`).
Base: `b02ee7d51709de664ee88db68abca34025db3487`.
Worktree: `.worktrees/issue-1003-safe-reads-1002-escape-1004-config-1015-cleanup-1010-status-1005-cycles`.
Platform: native macOS. No subagents, source-acquisition changes, new commands, schema changes, or compatibility adapters.

## Result and interface for Task 5

`crates/hoimin-cli/src/human_text.rs` owns the presentation policy. Import
`crate::human_text::HumanText`; use `HumanText(value)` with `{}` for an external
unquoted string and `{:?}` for a quoted external string. Convert UTF-8 paths using
`.as_str()` and native diagnostic-only paths with `.to_string_lossy()` at the
boundary. The wrapper borrows its input; it never mutates logical data. Do not
apply it twice or to an entire rendered document.

Display escapes all Rust `char::is_control()` characters (C0/C1), plus
U+2028/U+2029. Examples: ESC becomes `\u{1b}`, CR `\r`, LF `\n`, TAB `\t`.
Ordinary Unicode remains readable. Debug adds double quotes and escapes quote and
backslash delimiters, while sharing the same control policy. Display preserves
literal backslashes so already-sanitized diagnostic/transcript text is not
escaped a second time.

`HumanText(value).baseline_output()` is the narrow multiline display entrance:
only LF/TAB are preserved; other controls and Unicode line separators still
escape. Only an event diagnostic whose code is exactly `baseline.output` uses it.
Its level/code still use ordinary single-line HumanText. The baseline capture,
decoding, chunking and existing child-output sanitizer are unchanged. Task 5
should use ordinary HumanText for every external result-row/diagnostic field.

`escape_clap_context(&mut clap::Error)` projects String/Strings contexts and
individual Suggested tip bodies. Pinned local clap_builder 4.6.7 exposes mixed
StyledStr tip bodies but generates tip labels/newlines/Usage separately. Each
inline tip is rendered to plain text and escaped once; the outer layout remains
owned by Clap. There is no raw-substring replacement, so control-only values,
overlapping contexts and replacement ordering do not affect trusted layout.
Controller diagnostic ruling R7 approved this boundary. Its normal plain Display
already strips tip ANSI styling; the headline retains visible escaped user text.

Covered human boundaries: preview ID/path/operator/original/replacement; run and
mutant IDs; finished candidate fields; fingerprint patterns/files/input paths and
hashes; event/session diagnostic level/code/message; review IDs/paths/reasons/
evidence/operator names; progress detail strings and report-path warnings;
preparation/command errors; malformed Clap argument/value/subcommand contexts.
Machine serialization branches and logical candidate data are unchanged.

## Five requirements/design review passes

1. **Boundary coverage:** inspected preview, report, review, progress and command
   entry points. Preview/report Display paths were unsafe; review/progress details
   already quoted strings but now reuse the common policy. Progress warning paths
   were another raw Display route and were included.
2. **Control/Unicode semantics:** chose C0/C1 plus Unicode line separators for
   single-line fields. Retained ordinary Japanese/emoji and literal backslashes;
   quoted text additionally escapes delimiters. No filename-ban branches added.
3. **Logical and machine isolation:** projection belongs only in human formatting;
   preview serialization, report JSON/JSONL, persisted paths and candidate IDs are
   unchanged. Parsed machine values are independently compared with raw fixtures.
4. **Dry-run side effects:** used real control-bearing native files, public verify
   subprocesses and a marker-writing saved test command. Every format must leave
   marker absent, plan bytes unchanged and scratch output empty.
5. **Ownership and separate baseline contract:** one helper owns control policy;
   baseline transcript LF/TAB remain readable through an explicit narrow view.
   Clap is sanitized at context boundaries, preserving generated separators.

## RED evidence and corrections

All command invocations below used `CARGO_BUILD_JOBS=2` and
`CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`; tests used
`--test-threads=2`. Commands were sequential, never concurrent Cargo operations.

- `cargo test --locked --offline -p hoimin-cli --test report_handler human_format_escapes -- --test-threads=2`
  failed all four new tests against unchanged production: candidate fields, event
  diagnostics, session diagnostics and run provenance emitted raw ESC.
  Log: `/tmp/hoimin-priority-six-task2-red-report.log`.
- `cargo test --locked --offline -p hoimin-cli --test plan external_ -- --test-threads=2`
  failed preview control filenames, preparation errors, and malformed arguments.
  Preview emitted raw ESC; preparation created an injected extra line; Clap
  emitted raw CR/LF/TAB both in its heading and inline tip.
  Log: `/tmp/hoimin-priority-six-task2-red-plan.log`.
- `cargo test --locked --offline -p hoimin-cli --test progress progress_diagnostics_escape -- --test-threads=2`
  failed because paths split three renderer warnings into seven lines.
  Log: `/tmp/hoimin-priority-six-task2-red-progress.log`.
- `cargo test --locked --offline -p hoimin-cli --test report_handler human_diagnostic_has_no_input_controls -- --test-threads=2`
  failed and shrank to raw NUL. Added NUL/DEL/U+2029 literal cases; final properties
  use arbitrary Unicode Strings with failure persistence disabled. The task-owned
  generated regression file was removed after preserving the minimal literal.
  Log: `/tmp/hoimin-priority-six-task2-red-property.log`.
- After shared escaping, the explicit baseline-layout test failed because generic
  escaping converted intentional transcript LF/TAB to textual escapes. Added the
  narrowly approved multiline projection. Ordinary messages with the same prose
  remain single-line; preescaped child ESC remains escaped exactly once.
  Log: `/tmp/hoimin-priority-six-task2-red-baseline.log`.

Initial GREEN compilation exposed three Utf8PathBuf-vs-str mismatches; changed
only display arguments to `.as_str()`. An early malformed-format test incorrectly
required Usage, but pinned Clap renders only possible-values/help for that case;
its expected framework structure was corrected. An early review test used
`--format human`; the review command's existing value is `text`, so the fixture
was corrected. These were test/setup mistakes, not accepted product behavior.
Both tests passed on rerun. Clippy first rejected write_event at 105 lines; joined
provenance display buffers are now escaped once, preserving the original
structure with fewer allocations. Clippy also requested assert_eq for an empty
output assertion; corrected without lint suppression.

Exact failed test names observed during the work (all resolved):
`human_format_escapes_external_candidate_fields`,
`human_format_escapes_event_diagnostics`,
`human_format_escapes_session_diagnostics`,
`human_format_escapes_run_provenance`,
`verify_preview_external_filenames_are_single_line_without_running_commands`,
`verify_preparation_diagnostic_escapes_external_manifest_path`,
`malformed_argument_diagnostics_escape_external_controls_preserving_usage`,
`progress_diagnostics_escape_external_report_paths`,
`human_diagnostic_has_no_input_controls`,
`human_format_preserves_only_baseline_transcript_line_layout`, and
`review_human_quotes_external_fields_and_json_keeps_logical_values`.

## Five implementation review passes

1. **External-field inventory:** audited all changed human formatting arguments;
   external IDs, paths, operator strings and diagnostics use the helper. Static
   enum names/numbers and renderer-generated separators remain direct.
2. **No double escaping:** Display does not escape literal backslashes. Quoted
   string fields are wrapped once; evidence collections contain wrappers rather
   than preformatted strings. Baseline and literal-backslash cases check this.
3. **ASCII/Unicode:** hand-written expected strings cover ESC/CR/LF/TAB/NUL/DEL,
   C1 NEL, U+2028/U+2029, Japanese/emoji and ordinary ASCII. Shared quoted
   projection tests cover apostrophes, quotes and backslashes; properties cover
   arbitrary Unicode in both Display and Debug modes.
4. **Logical-data isolation:** the complete diff changes only renderer use sites,
   presentation helper registration, tests and guidance. Preview/report machine
   writers receive the original objects, with no ID/path normalization added.
5. **Module depth and scope:** one borrowed adapter hides control/quote decisions;
   one named narrow transcript method preserves its existing contract. Clap
   adaptation consumes context values, not parser internals or replacement
   matching. Existing baseline sanitization is intentionally separate streaming
   policy, not a copied new command sanitizer. No unrelated abstraction added.

Software-design-philosophy score: **10/10 (8/8 diagnostic rows)** at this task's
scope. Interfaces are smaller than the behavior they hide, control policy lives
in one module, interface comments state ownership and guarantees, and the design
review simplified provenance rendering. Failed rows: none; no further required
change for this task's 10/10 target.

## Final validation

Command:
`cargo test --locked --offline -p hoimin-cli --lib --test plan --test report_handler --test review_cli --test progress --test baseline_output -- --test-threads=2`

Exit 0: CLI library **744 passed, 25 ignored**; baseline_output **4 passed**;
plan **117 passed, 1 ignored**; progress **128 passed**; report_handler **44 passed**;
review_cli **18 passed**. Total top-level tests: **1,055 passed, 26 ignored**.
Ignored cases are the existing platform/subprocess fixtures; they are not claimed
as executed. The library also runs two successful nested fixture checks.
Log: `/tmp/hoimin-priority-six-task2-final-tests.log`.

`cargo clippy --locked --offline -p hoimin-cli --all-targets --all-features -- -D warnings`
passed; mandatory commit hooks also passed for the final complete diff, including
all workspace targets/features and the vendored parser. Log:
`/tmp/hoimin-priority-six-task2-clippy.log`.

`cargo fmt --all` and `git diff --check` passed. Focused OKF checking parsed both
modified concepts, checked unique source IDs, and verified new source files,
SHA-256 hashes and footnotes. Source metadata was refreshed only for newly read
Task 2 sources, preserving historical evidence.

## Five validation review passes

1. **Regression sensitivity:** original unsafe production failed four report,
   three public preview/diagnostic, one progress and one property regression;
   the baseline distinction also failed before its explicit projection existed.
   Expectations are literal escapes/raw paths, not generated by HumanText.
2. **AAA and cases:** tests arrange real input files/events, invoke real public
   boundaries, then compare emitted bytes/parsed values and side effects.
   Cases of one contract are table-driven; property inputs span arbitrary Unicode.
3. **Machine and side-effect invariants:** JSON/JSONL parsed candidates and
   diagnostic messages retain raw controls/Unicode; preview IDs match the saved
   candidate and command markers remain absent across every preview format.
4. **Native filename/evidence limits:** actual ESC/CR/LF/TAB/Japanese filename
   tests are Unix-gated and were executed on macOS. String-only tests cover NUL,
   which native filesystems do not allow in names. No native Linux/Windows claim;
   non-UTF8 path policy and existing Unix backslash restrictions are unchanged.
5. **False-safety claims:** ordinary external fields contain no C0/C1 or Unicode
   line separators; only baseline transcripts intentionally retain LF/TAB. This
   addresses terminal control interpretation, not all Unicode visual spoofing,
   shell execution, complete child-output trust, or arbitrary-code-execution proof.

## Guidance and remaining scope

Updated docs/usage.md and the selection-plan-verify/session-report concepts with
human/machine boundaries, baseline exception, dry-run behavior and evidence
limits. Final integration owns combined catalogs and full-workspace test runs,
per the binding task brief. No full integration suite was rerun here.

Concerns: no unresolved task blocker. Native Linux/Windows remain unverified.
Clap source-error internals are private; current numeric/value-enum parser source
messages are framework text. The adapter handles exposed external String/Strings
contexts and inline tips, without claiming arbitrary future custom Clap error
sources are sanitized. Future custom parsers should keep external message text
behind the same display boundary.


## Commit and hooks

Committed `3d9e33d178a451e39e182495f4a5bb3a4c5ebfa0`. All four mandatory hooks
passed: workspace fmt, vendored-parser fmt, workspace all-target/all-feature
clippy and vendored-parser clippy. Commit inherited
`CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`. The initial
sandboxed commit could not create .git/worktrees/.../index.lock; its explicitly
authorized escalated retry succeeded. There was no automatic approval rejection,
no hook bypass, and no extra worktree target directory was created. Hook log:
`/tmp/hoimin-priority-six-task2-commit.log`.

Post-commit `git status --short` is empty. Rechecked both Task 2 OKF source-hash
and reference sets after the commit; all matched. The ignored task report remains
in the plan-owned SDD directory for final integration archival, as requested.


# Task 2 fix round 1/5

Base: `3d9e33d178a451e39e182495f4a5bb3a4c5ebfa0`. Read the complete
`task-2-review.md` and controller R10 before editing. Both Important findings
were verified and addressed. No subagents or broader audit was performed.

## Changes

The shared `root_command` now fixes `.bin_name("hoimin")`, so Usage/help use the
trusted product name even when the executable is renamed. Its API comment states
that contract. This follows R10 and matches completions/documentation. Only this
command-factory setting and comment change production behavior; the existing
HumanText policy and logical/machine-data paths are unchanged.

Public-binary tests supply argv[0] through Unix CommandExt::arg0, exercising ESC,
CR, LF, TAB, mixed controls and a Unicode alias on both error and help routes.
The expectations require real `Usage: hoimin <COMMAND>` structure, the correct
exit/stream, and no controls except renderer-owned LF.

All requested independent example tables now expand into runner-visible named
parameter cases using local test macros and single-scenario assertion helpers:
15 preview filename/format cases; 7 malformed-argument routes; 12 executable-name
error/help cases; 96 report value/format/event cases; 2 baseline-code cases; and
6 review filename/format cases. Total: **138 individually named generated cases**.
The preview/review formats have independent fixtures. No case loop can stop the
other inputs from being executed by the runner. Multiple fields from one outcome
remain asserted together. Existing literal expectations and broad properties are
preserved; no test framework dependency was added.

Only the two active Task 2 test-source metadata entries in affected OKF concepts
were refreshed to the fix base and current hashes. Their contracts and historical
evidence were not rewritten. The generated CLI reference remains current.

## RED/GREEN evidence

Every Cargo invocation below used
`CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`; test
invocations used `--test-threads=2`. No concurrent Cargo operations.

**RED before the production fix:**
`cargo test --locked --offline -p hoimin-cli --test plan hostile_executable_names -- --test-threads=2`
exited 101: **10 failed, 2 passed**. Both error/help cases failed for CR, LF, TAB,
mixed controls and the Unicode alias. Exact failing names:
`hostile_executable_names::{cr,lf,tab,mixed,unicode_alias}::{error,help}`. ESC-only
names already passed because plain Clap styling removed that ANSI sequence; the
other cases independently demonstrated untrusted executable-name interpolation.
Log: `/tmp/hoimin-priority-six-task2-fix1-red-argv0.log`.

**GREEN after trusted bin_name:** the same command exited 0, **12 passed**.
Log: `/tmp/hoimin-priority-six-task2-fix1-green-argv0.log`.

**Focused command-factory/test/refactor checks:**
`cargo test --locked --offline -p hoimin-cli --test plan --test report_handler --test review_cli --test cli_config --example generate_cli_reference -- --test-threads=2`
exited 0: cli_config **82 passed**; plan **149 passed, 1 existing ignored**;
report_handler **135 passed**; review_cli **23 passed**; CLI-reference example
**7 passed**. Total **396 passed, 1 ignored**. The output names the generated
parameter cases separately.
Log: `/tmp/hoimin-priority-six-task2-fix1-focused.log`.

After converting the final baseline pair to explicit macro-generated cases:
`cargo test --locked --offline -p hoimin-cli --test report_handler line_layout -- --test-threads=2`
exited 0: **2 passed**, 133 filtered out.
Log: `/tmp/hoimin-priority-six-task2-fix1-final-layout.log`.

`cargo run --locked --offline -p hoimin-cli --example generate_cli_reference -- --check`
exited 0 with no reference rewrite.
Log: `/tmp/hoimin-priority-six-task2-fix1-reference.log`.

`cargo fmt --all -- --check` and `git diff --check` passed. During the mechanical
test refactor, an intermediate assertion helper had an extra closing brace;
`cargo fmt` rejected that syntax. It was corrected before the successful focused
compilation/run. There were no remaining compiler/test failures or warnings in
the focused output. Final mandatory hook outcomes are recorded below.

## Five requirements/design review passes

1. **Executable-name provenance:** the new subprocess regression proves Usage
   includes unsanitized external argv[0] before the fix. R10 explicitly authorizes
   using the trusted product name, including for renamed executables.
2. **Layout ownership:** bin_name is fixed before Clap builds Usage/help; no
   document-wide escaping or StyledStr rewriting was introduced.
3. **Parameter contract:** the review requires runner-visible cases, rather than
   a loop with one test result. Each filename/format/value/event route now has
   its own named expansion and independently arranged state.
4. **Expected values:** hand-written escape/raw-value strings remain literal;
   helper outputs are not used to derive expectations. Public dry-run checks
   retain marker, saved plan-byte and logical ID/path invariants.
5. **Scope/ownership:** the fix is limited to the command factory, requested test
   cases, source metadata for those tests and this report. No HumanText API,
   baseline policy, persistence, schema or source-acquisition change was needed.

## Five implementation review passes

1. **Production diff:** reviewed the single .bin_name setting and its API comment;
   it is shared by parsing, completions and reference generation.
2. **Case inventory:** retained all twelve report strings, five preview names,
   seven malformed routes, two baseline codes and three review filenames;
   machine formats and event kinds are now separate parameter dimensions.
3. **Independent arrangements:** each generated function invokes one helper with
   one scenario. Review IDs derive from the saved plan, so no extra list call
   is needed to derive human-output expectations.
4. **Preserved policy boundaries:** baseline and ordinary diagnostics still use
   independent named cases with their original expected newline/backslash
   distinctions; properties remain broad invariants.
5. **Complexity/scope:** private macros provide named tests without dependency
   additions, runtime case loops, production adapters or parser reconstruction.
   Software-design-philosophy remains **10/10, 8/8 diagnostics**, no failed row
   or additional task-scoped design change required.

## Five validation review passes

1. **Reproduction sensitivity:** the new public argv[0] matrix failed ten named
   error/help cases before the production change and passed all twelve after it.
2. **Runner visibility:** focused output independently reports 138 generated
   cases across the requested tables; failures no longer hide later examples.
3. **Existing behavior:** all requested suites and command-factory configuration
   tests passed, including original machine-value and dry-run side-effect checks.
4. **Reference consistency:** seven generator tests and the actual --check
   command passed; no CLI-reference artifact change was necessary.
5. **Evidence limits:** native macOS subprocess tests do not establish native
   Linux/Windows success. The existing ignored case remains reported as ignored.
   No full integration suite or unrelated suite was rerun.

Concerns: no unresolved fix-round blocker. Native-platform and future custom
parser-source limitations remain as recorded in the original task report.


## Fix round 1 commit outcome

Committed `6d78789eca05fad577b2a8dcce83077f9389ee06`. All four mandatory
hooks passed: workspace fmt, vendored-parser fmt, workspace all-target/all-feature
clippy, and vendored-parser clippy. Git commit inherited
`CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`; no hooks
were skipped. Log: `/tmp/hoimin-priority-six-task2-fix1-commit.log`.
Sandboxed staging could not create shared .git/index.lock; the authorized
escalated staging/commit succeeded without automatic approval rejection.

A focused-log check counted **138 unique independently reported parameter names**;
all passed. Current source hashes for both refreshed OKF test entries match.
Both requested findings are addressed and ready for scoped controller re-review.

Post-commit `git status --short` is empty. No unresolved fix-round concerns.
