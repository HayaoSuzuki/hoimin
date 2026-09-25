# Issue 620: bounded tracked Git diff scoping

Design/plan and three reviews each were committed before implementation as `de41fe7` (rebased to `67a3e61`), based on main `99c27dc`. The initial global name/status inventory keeps Git's original rename decisions while avoiding patch bodies. Ordinary eligible paths then use bounded literal patch/numstat batches with rename detection disabled. Selected rename/copy or unusual statuses and oversized single paths retain the legacy full diff. Unscoped callers retain the original algorithm.

## RED → GREEN and correspondence

The public CLI test forwards real Git output through a subprocess-local recording wrapper. At fixed two-file counts, the old implementation delivered 528,754 and 4,227,446 diff bytes for the two CSV sizes. Candidate identity was the same one candidate. After optimization, both deliver 201 bytes (patch plus numstat), below the unchanged 4-KiB ceiling. This test measures actual transport rather than inferring savings from argv or relying on noisy timing thresholds. Logs: `620-red.log`, `620-focused-complete.log`.

The direct Git rename probe showed why naive path filtering is insufficient: the selected destination became a 31-line addition instead of its original changed row. Public resolver tests compare static results to the unchanged unscoped API for into/out-of-scope and unrelated renames, modifications, deletion, binary files, untracked/unborn, diff-base, supported unusual names and 140 long selected paths. Existing target/context/line oracles additionally protect merge-base, deletion context, CR/CRLF, attributes, candidate identity, public run and saved verify. This is static-file equivalence, not a snapshot-consistency guarantee if inputs change between inventory and patch commands.

## Implementation reviews

1. **Global versus scoped pairing:** inventory uses the same base, rename and hunk-related fixed options as the old patch command, replacing patch output with NUL name/status output. Selected R/C destinations force legacy processing; U/X/B also fall back conservatively. Other statuses use --no-renames so removing competitors cannot manufacture a new pairing. Binary exclusion and physical-row conversion still happen through the old parsers/callers. Unrelated renames do not force legacy processing.
2. **Paths, bounds, and hostile configuration:** actual Git path spelling passes through GitPathScope before portable validation. Batches limit both 128 paths and 8192 UTF-8 path bytes; fixed flags plus at most 128 separators/terminators add bounded overhead. Single oversized paths fall back. `--literal-pathspecs` and the `--` separator protect glob/magic/option-looking names. A real probe found GIT_GLOB_PATHSPECS and GIT_ICASE_PATHSPECS conflict with Git's literal flag; a public RED test reproduced exit 128. Scoped subprocesses now clear conflicting pathspec environment overrides, preserving the resolved target meaning without modifying the parent environment.
3. **Downstream and error flow:** empty scoped inventory skips patch generation only after Git/base validation; invalid base and non-repository errors remain. Unborn and untracked collectors are unchanged. Scope filtering still precedes portable validation, and malformed name/status framing, unknown status codes and invalid UTF-8 are errors. Metadata remains global, so this does not promise constant work in repository size or rename workloads. No snapshot or concurrency claim is made.

Root independently reviewed the production planner/dispatcher and found no blockers, checking global rename options, membership before validation, R/C/U fallback, count/byte bounds, original path spelling, literal arguments and disabled subset rename detection. Root specifically confirmed the static-input boundary and bounded NUL overhead.

## Test reviews and corrected preparation findings

1. **Independent output evidence:** the wrapper invokes real Git and forwards output unchanged. It runs only for the bounded public regression; release measurements use the unwrapped binary. Both source files and their candidate bytes are fixed across fixture sizes. The pre-change failure records actual bytes, not a setup or compilation failure. The hostile-env RED separately pins a discovered integration issue.
2. **Selection and filename premises:** the initial unusual-name fixture incorrectly assumed --source and --file narrow each other; they are additive selectors, so exact-file public resolver selection is used for that test. A second premise assumed colon filenames were supported by WorkerRoot; inspection of normalized_relative_path and comparison to the historical resolver showed both reject them. The test now explicitly preserves that behavior while ensuring the magic-looking literal name cannot exclude other valid paths. No portable-path production contract was broadened. Clippy also caught a similar binding name and unnecessary raw-string hashes; both were corrected. Logs: `620-literal-premise.log` and retained Git probe directories.
3. **Isolation and portability:** real Git setup uses controlled configuration; subprocess calls have 20-second bounds, public CLI 40 seconds, and private TMPDIR/TMP/TEMP. The Git-output wrapper is Unix-only; general resolver and planner tests remain portable. Unix-only helper/import gates avoid Windows dead-code failures. Batched count/byte tests assert the fixed contract limits independently of the implementation constants; the redundant argv-shape assertion was removed in favor of the real-Git literal-name regression. Both independent limits are covered, and a 140-path public test checks that no targets are lost. Selected rename fallback and unsupported-colon behavior are explicit limitations, not successful optimization claims.

## Lean and external contract boundary

No new Lean model is added: the changed boundary is Git's own rename/pathspec execution and subprocess output volume. Existing Lean-generated changed-target/context/line adapters are exercised unchanged. Direct Git observations and public output byte measurements independently cover the new behavior. No Lean command/lease or Python production mutation campaign is needed. The Git command forms were checked against the official documentation at https://git-scm.com/docs/git-diff; the implementation keeps Git responsible for content conversion and attributes.

## Release measurements

The original issue script ran against retained release binaries from main `99c27dc` and this implementation, with three repetitions of each changed/plain plan, CSV size and state: 36 plans per binary. All 36 matched pairs preserve complete candidate objects. The following are changed-plan medians; RSS is sampled for the Hoimin process alone every 5 ms, expressed in decimal MB.

| CSV | State | Before seconds | After seconds | Before RSS MB | After RSS MB |
| --- | --- | ---: | ---: | ---: | ---: |
| 4 MiB | clean | 0.0627 | 0.0526 | 6.16 | 6.19 |
| 4 MiB | dirty | 0.2795 | 0.0615 | 23.22 | 6.26 |
| 4 MiB | clean repeat | 0.0920 | 0.0552 | 6.55 | 6.59 |
| 16 MiB | clean | 0.1498 | 0.0966 | 6.18 | 6.28 |
| 16 MiB | dirty | 0.7501 | 0.1334 | 56.95 | 6.42 |
| 16 MiB | clean repeat | 0.0333 | 0.1019 | 6.18 | 6.05 |

The initial 16-MiB clean-repeat result was slower after the change. A bounded follow-up alternated the two binaries on the same fixture for six pairs, rewriting identical clean CSV content before each run to exercise stat refresh. Medians were 0.1492 seconds before and 0.0987 after, with identical complete candidate objects in all 12 outputs. This does not establish the cause of the original difference or promise that every clean run improves. Timing is supporting evidence; the stable regression gate measures actual diff-output bytes.

Root-only RSS excludes Git's own memory; retained wait4 observations include children and are kept separately. Short plain-plan processes can finish between RSS samples, so no plain-plan RSS claim is made. Metadata and rename detection remain repository-wide and may still read unrelated content. Selected rename/copy fallback retains the original patch cost. All original scripts, binaries, fixtures, 72 primary manifests, 12 follow-up manifests and raw observations are retained under `/tmp/hoimin-batch-604-632/620-perf`; `620-comparison.log` and `620-paired-clean.log` record the comparisons.

## Validation

Full locked workspace tests: 2,427 passed, 0 failed, 22 ignored across 112 result entries (`620-workspace.log`). The focused 73 public target/changed/oracle tests passed, including all seven new integration tests. The final full run includes the two new inventory/batching unit tests after removal of the redundant argv-shape test. Workspace all-target/all-feature Clippy and the exact vendor Clippy command passed; final format/Clippy checks and any main-integration results are recorded below before publication.

Final pre-integration checks passed: `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`, `cargo fmt --all --check`, `cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check`, and `git diff --check`. The first vendor-format invocation used a nonexistent directory named after the package; rerunning with the CI manifest path succeeded.

The unpublished branch was then rebased without conflicts onto main `c684749`, including the new explicit-source existence validation. The integration check passed 78 related public tests, 22 Git parser/planner unit tests, and all 40 Python CI-workflow contract tests. Both exact Clippy and fmt gates passed again, along with diff whitespace. Logs are `620-main-integration.log`, `620-main-parser.log`, `620-main-python-ci.log`, `620-main-clippy.log` and `620-main-vendor-clippy.log`. No unrelated full-suite rerun was needed after this clean integration.
