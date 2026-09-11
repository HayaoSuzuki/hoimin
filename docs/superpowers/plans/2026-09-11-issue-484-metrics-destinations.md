# Issue 484 implementation plan

Spec: ../specs/2026-09-11-issue-484-metrics-destinations-design.md

This worktree starts from4adf809. The controller owns docs/superpowers and docs/knowledge. One architecture implementer owns Rust metrics/shell code, a narrow path helper if needed, tests and current README/development documentation. No merges, delegation or cargo-mutants. Every Cargo command uses `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`; run only one Cargo command at a time. The untracked .venv symlink supplies CPython3.14.7 and must not be committed. Keep logs in /private/tmp/issue484-*. Destructive RED tests use only temporary fixtures created for this task.

## Plan self-review

1. Sequence: establish actual overwrite RED, identify the entry replaced by rename, validate before baseline, then deny final output on collision. An early core failure event alone does not prevent metrics finalization on a modeled nonzero result.
2. Coverage: selected source, explicit fingerprint input and session artifacts; path spellings, aliases, case and safe link controls. Check bytes/hash and absence of a baseline marker, plus failure metrics and late-warning compatibility.
3. Ownership: existing blocking Preflight has configuration, resolved targets and owned workspace. Reuse its completion handoff without constructor IO or new core phases. Keep root documentation separate, use bounded tests and avoid blanket project-output restrictions.

## OKF self-review

1. Read architecture, runtime lifecycle and session/report concepts against actual persist/finalizer code. Original-source verification happens before metrics output and does not protect this late replacement.
2. Add the issue-specific contract to those concepts and the exact source to the design index, preserving historical revisions. Distinguish rename entry identity from the referent of a link.
3. Validate metadata, source hashes, footnote pairs, links, reachability and the161 design entries/display count. Native filesystem observations establish path behavior; abstract state proofs cannot substitute for them. Refresh source hashes after design refinements.

## Task 1: Protect input entries from metrics replacement

Read the spec, /private/tmp/issue484-preflight-notes.md and issue484 in /private/tmp/hoimin-bug-issues.json. Worktree: /Users/hayao/RustroverProjects/hoimin/.worktrees/issue-484; base4adf809; branch fix/issue-484-metrics-destinations. Own relevant metrics/shell/path/session helper code, tests and current README/development docs. The controller owns docs/superpowers and docs/knowledge. Do not spawn agents, push or create a PR. Send concrete evidence and the smallest sound alternative before a material design expansion or compatibility change; the controller will decide within the user's authorization.

- [x] Capture controlled RED with the actual CLI replacing a temporary selected source after a passing baseline. Reproduce session entry replacement if useful without depending on WAL repair. Use portable repository Python and bounded subprocesses; never mutate user files.
- [x] Implement destination identity for NamedTempFile.persist. Protect selected source, explicit fingerprint inputs, configured/actual session DB entries and exact active sidecar/ownership namespaces as justified by the implementation. Handle absolute/CWD-relative paths, dot/dotdot, parent aliases and OS case behavior. Preserve safe replacement of a distinct hardlink or final symlink. Do not substitute inode equality, blind final canonicalization or global lowercasing. Document platform limits and identity-query failure handling.
- [x] Validate before baseline in existing owned blocking Preflight. Retain workspace cleanup and preserve constructor behavior; do not add filesystem IO directly to the async loop. Separate confirmed collision from ordinary output failures such as an existing directory or missing parent, which retain the late warning contract.
- [x] Make final metrics permission depend on collision validation, not the raw configured path or run_result.is_ok. A collision EffectFailed can lead to a modeled nonzero result. Carry a small typed permission/validated path through the owned completion before applying the core event. Preserve metrics after later ordinary failures, shutdown deadlines, owned blocking finalization and warning behavior. Avoid a generic framework or a new core phase.
- [x] Public run tests cover source/fingerprint/new and existing DB collisions, aliases/case and active artifacts. Require explicit nonzero failure before the baseline marker, unchanged bytes/hash, no JSON replacement and normal cleanup. Test the actual collision finalizer, not only a comparator. Controls retain normal in-root metrics, updates, distinct link replacements and failed-baseline metrics; inspect the protected referent after replacement.
- [x] Preserve existing metrics_write_failure_warns_without_changing_run_result and metrics_finish_baseline_timing_before_early_cleanup. Do not weaken them to permit a new fatal error for an ordinary unwritable destination. Inspect earlier target/preflight failure guarantees before changing their output behavior.
- [x] Run focused metrics/public/shell lifecycle tests, then `cargo test --workspace --all-features`, `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`. Use Lean only for a useful new permission-state claim with implementation correspondence. Any Lean command must use the existing serial guard:30 seconds,2048MiB RSS,250ms sampling, -j1 and -DElab.async=false, no limit increase. IDE MCP last reported hoimin not open; use Cargo and do not inspect unrelated projects.
- [x] Perform three implementation and three test self-reviews. Report exact commands, logs, diagnosed failures, platform limits and decisions. Commit only owned files and write .superpowers/sdd/2026-09-11-issue-484-metrics-destinations/task-1-report.md. Stop for review; do not rerun a green full suite without a new concern.

## Controller ruling on prospective identity

Approved narrow native parent sensitivity queries and typed withheld output, preserving lazy session opening. Known collisions reject before baseline; unresolved identity prevents metrics writing and produces the existing late warning while keeping the run result. The cost is platform-specific maintenance and possible omission of otherwise writable ambiguous/unsupported metrics outputs. This substantive ruling and cost must reach the final report; it is not a claim of complete Unicode/native-path equivalence. Require native new-DB case-alias and withheld-output regressions.

Three refinement reviews:1 inspected lazy SessionDispatcher opening and canonical ownership-lock naming;2 compared alternatives against existing late-warning/baseline-failure contracts and prohibited user-visible output probes;3 updated spec hashes and reran OKF validation, keeping prospective uncertainty distinct from confirmed collisions.

## Controller implementation review findings

1. Native entry enumeration must check every exact spelling before deciding aliases are ambiguous. Returning at the second matching inode can hide a later exact entry when three hardlinks exist; require an ordering-independent control. File inode is used only to find a native entry alias, never as the protected-entry equality relation.
2. Session artifact resolution must not hide a confirmed source/configured-DB collision behind an unrelated identity error. Protect both the literal ownership-tree entry and its resolved namespace when the tree is a symlink. The pinned SQLite VFS uses resolved DB sidecars on Unix and configured-entry sidecars on Windows; use that narrow distinction without adding an unsafe VFS query layer.
3. Linux FS_IOC_GETFLAGS writes int, despite the request encoding using long; checked the primary [ioctl_iflags(2) manual](https://man7.org/linux/man-pages/man2/ioctl_iflags.2.html). Interpret casefold flags only for filesystems where they establish directory case behavior. Avoid an input-by-directory quadratic scan by sharing parent enumeration or resolving only potentially colliding entries. These are required implementation corrections; final evidence must demonstrate their resolution.

4. A parent that is absent during validation must not become a writable prospective destination after baseline creates a symlink. Require the destination parent to resolve after confirmed-collision checks; otherwise withhold metrics. This follows the existing uncertainty ruling. Add a real CLI control rather than claiming protection against arbitrary concurrent replacement.

## Final controller self-reviews

Implementation:1 traced the final diff's default-closed destination and owned completion installation before event application;2 checked exact-entry cache and native API scope against the earlier concrete findings, plus configured/resolved SQLite and symlinked ownership namespace;3 checked future locktree collision precedence, missing-parent withholding and preservation of trailing-separator semantics. These reviews complement the implementer's three reviews below.

Tests:1 read original destructive RED and finalizer-fallback sensitivity evidence;2 checked positive candidate IDs/spans/counts and source/session byte preservation, with failed-baseline and ordinary warning contracts retained;3 independently summed the initial log's70 groups to1633 passed/0 failed/13 ignored, then the post-review log to1636 passed/0 failed/13 ignored (including its interleaved child summary), read final fmt/Clippy outcomes and kept superseded full results and native platform limits explicit.

Design and OKF refinement:1 updated the design to final directory caching, Linux int/FS scope and OS-specific SQLite sidecars;2 documented missing-parent/trailing-separator refinements and distinguished known collision from withheld output in all three concepts;3 refreshed all four source hashes and validated16 pages,311 source-footnote pairs,614 local links, reachability and161 source/index/display entries.

PR:1 describe the concrete source overwrite and resulting prebaseline rejection/finalizer permission;2 report final tests separately from intermediate RED/GREEN, with native Windows/Linux locally unexecuted;3 link this issue and current docs, retain the substantive uncertainty ruling and output-omission/platform cost, and preserve OKF checklist scope. PR draft/private/tmp/hoimin-pr-484.md.

## Persisted implementation report

# Task 1 report: protected metrics destinations

Latest completed revision: `76390dcf95f86e736aa9dfa5d51910bd33bb1064`. Follow-up final verification: 1636 passed / 0 failed / 13 ignored; fmt and Clippy clean. See terminal-directory-syntax follow-up below.

Status: implementation and verification complete. Owned commit: `913101a4bfaafdccabac493d765423b52f5e704e` (`fix(metrics): protect input entries from metrics replacement`). Worktree `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-484`, branch `fix/issue-484-metrics-destinations`, base `4adf809`. No push, PR creation, subagents or cargo-mutants. The controller owns docs/superpowers and docs/knowledge; those changes remain outside this commit. The `.venv` symlink is untracked and excluded.

## Result

The existing owned blocking Preflight checks the directory entry that metrics' `NamedTempFile::persist` will replace. Confirmed collisions return `metrics.destination.collision` before baseline with destination/protected-input diagnostics. Protected entries include selected sources, explicit fingerprint inputs, configured and resolved session DB entries, exact active SQLite companion names and literal/resolved session ownership trees. Session ownership supplies its directory naming helper so metrics does not maintain a second naming rule.

The completion returns typed metrics permission alongside workspace ownership; acceptance installs both before applying the core event. Cleanup completion does not overwrite the decision. The finalizer uses only the permitted resolved PathBuf and retains the existing owned blocking shutdown budget. It does not derive permission from raw configuration or `run_result.is_ok()`. A default-closed destination also means a failure before collision validation has no metrics write permission. After authorization, baseline failure still writes metrics; a later preflight budget Rust error still emits `metrics.incomplete` and skips the sidecar.

Native entry spelling preserves distinct hardlinks and final symlinks. Unix directory identity only unifies parents; final-file inode identity only finds native aliases within a directory and never equates distinct replacement names. Each validation caches a directory name index, with a lazy no-follow identity index for inexact spellings. Exact spelling wins even among three hardlinked symlinks. macOS pathconf, Windows directory case information and Linux ext4/f2fs casefold queries handle prospective ASCII case aliases. Linux uses an int ioctl argument despite the long request encoding and does not interpret a zero flag on arbitrary filesystems.

The approved uncertainty policy withholds metrics and emits a late `metrics.write` warning without changing the run result. It covers unresolved destination/input identity, ambiguous inexact hardlink aliases, prospective non-ASCII comparisons, unsupported filesystem case queries and unsupported Windows trailing-dot/space, stream or prospective short-name spellings. An unresolved destination cannot establish all collisions; no claim of complete recognition is made. Ordinary directory and missing-parent output failures retain warning behavior.

Two final syntax/parent refinements have direct RED/GREEN evidence. A missing destination parent cannot grant write permission: a baseline might later create it as a symlink to protected inputs. Confirmed prospective ownership-tree collisions are checked before that withholding decision. A trailing separator requires a directory; dropping it would let persist replace a directory symlink itself. Such output remains a late warning, and a regular-file path followed by `/` retains its ENOTDIR behavior. The finalizer also passes a test where an existing configured parent alias changes during baseline: it writes the authorized resolved directory and preserves source bytes. These checks do not freeze filesystem entries against arbitrary external renames during a run.

## Native/session evidence and limits

Executed locally on macOS arm64 with the repository CPython 3.14.7. Native case aliases were available and tested, including absent `session.db` versus `SESSION.DB`. Windows and Linux platform branches were reviewed but not executed locally. Windows library tests cover native entry/case behavior and configured-symlink sidecar names so the existing manual non-Linux job can obtain evidence even if unrelated integration failures block later tests. Windows symlink tests print an explicit SKIP reason when setup permissions are unavailable. Do not report that skipped setup as exercised symlink coverage.

Pinned SQLite evidence: `libsqlite3-sys-0.38.2/sqlite3/sqlite3.c`, `appendOnePathElement` / `unixFullPathname` around lines 47120–47234 resolves symlink components; `winFullPathnameNoMutex` around lines 54792–54818 calls GetFullPathNameW and preserves the configured final basename. Accordingly, Unix sidecars use the resolved DB basename and Windows sidecars use the configured entry. Both protect configured/resolved DB entries and canonical ownership naming. This avoids a new SQLite FFI layer or earlier database creation/migration. Existing DB byte comparisons run before reopening SQLite and do not depend on WAL repair.

No new Lean model or command was used. The bounded new claim is the typed preflight-permission handoff to the existing finalizer. Existing executable result-lifecycle/shutdown correspondence and direct finalizer/native filesystem regressions provide the relevant evidence; abstract path equality would not prove the native rename semantics.

## Commands and evidence

Every Cargo command below used this exact prefix, serially:

`CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`

- Initial `cargo test -p hoimin-cli --test metrics_destinations -- --nocapture`: expected RED, `/private/tmp/issue484-red.log`. The unmodified CLI returned exit 0, baseline Exit 0, killed 1 and complete true, then replaced a temporary calc.py with JSON. Fixtures only.
- Initial matrix `cargo test -p hoimin-cli --test metrics_destinations`: 2 controls passed, 7 regressions failed, `/private/tmp/issue484-red-matrix.log`.
- Review RED for lock-directory symlink referent and trailing regular-file separator: 15 passed / 2 failed, `/private/tmp/issue484-review-red.log`; subsequent 17 passed, `/private/tmp/issue484-review-green.log`.
- `cargo test -p hoimin-cli metrics`: 47 passed across filtered groups, `/private/tmp/issue484-focused-metrics-final.log`. Includes the then-current 19 CLI tests, library native identity checks, directory write-warning contract, failed-baseline metrics, collector state and shutdown metrics tests. Later added parent/syntax tests are covered by final full verification below.
- `cargo test -p hoimin-cli --test lean_result_lifecycle_oracle --test lean_shutdown_oracle`: 12 passed, `/private/tmp/issue484-focused-lifecycle.log`.
- Controlled manual raw-path-fallback mutation, `cargo test -p hoimin-cli --test metrics_destinations metrics_collision_preserves_selected_source_before_baseline`: expected exit 101, `/private/tmp/issue484-finalizer-gate-red.log`. Despite collision rejection before baseline, the fallback overwrote the source during finalization. A bounded Python try/finally restored shell.rs byte-for-byte. No cargo-mutants and no user files involved.
- First `cargo test --workspace --all-features`: 1630 passed, `/private/tmp/issue484-workspace-tests.log`. Superseded for final evidence by the additional parent/syntax concern below.
- Parent/syntax RED `cargo test -p hoimin-cli --test metrics_destinations`: 19 passed / 2 failed, `/private/tmp/issue484-parent-red.log`. A newly created parent alias overwrote source; dropping the directory-symlink separator replaced that symlink. After fixes plus future-locktree precedence control: 22 passed, `/private/tmp/issue484-parent-green.log`.
- **Final** `cargo test --workspace --all-features`: exit 0, **1633 passed / 0 failed / 13 ignored across 70 groups**, `/private/tmp/issue484-workspace-tests-final.log`. Includes all 22 native CLI destination regressions, existing lifecycle/oracle tests and documentation contract. Subsequent source changes only annotated Windows-specific lint differences; native macOS executable code is unchanged.
- **Final** `cargo fmt --all -- --check`: exit 0, `/private/tmp/issue484-fmt-final.log`.
- **Final** `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0, 4.48 seconds, `/private/tmp/issue484-clippy-final.log`.
- `git diff --check`: clean. No repeated green full suite after final verification.

## Implementation self-reviews

1. Traced permission and ownership from Preflight execution through completion acceptance, modeled nonzero failure, cleanup and metrics finalization. Kept all new filesystem IO in the blocking effect; no constructor IO or new core phase. Manual finalizer fault demonstrates that the denial must survive the actual finalization path.
2. Audited rename identity across relative/dot/dotdot/case/parent aliases, hardlinks, final symlinks, session configured/actual entries and lock trees. Found and repaired lock-directory symlink referent protection and original trailing-separator semantics. Read the pinned SQLite VFS to distinguish Windows sidecars from Unix rather than assuming referent naming universally.
3. Audited failure precedence, prospective namespaces, native API details and cost. Reused directory indices, deferred unrelated identity errors so known collisions win, constrained Linux ioctl semantics to ext4/f2fs, and retained explicit withholding elsewhere. Found the missing-parent baseline-created-alias overwrite and closed it without a new session phase. Added Windows-only lint allowances for an API that is stateful/fallible on Unix; no unsupported native execution claim.

## Test self-reviews

1. Required actual CLI nonzero collision diagnostics, absent baseline marker, preserved bytes/hash and clean execution-root cleanup. Existing DB tests inspect unchanged bytes before reconnecting SQLite. Initial source overwrite RED and manual finalizer fallback RED show these tests detect both missing preflight checking and a reopened finalizer gate.
2. Paired collisions with ordinary in-root creation/update, no-metrics candidate ID/span/status/count comparison, distinct hardlink/final-symlink replacement, inactive similarly named session artifacts, and existing failed-baseline/directory-warning contracts. Corrected an overbroad cleanup assertion: delivery is reported as cleanup_after_delivery while execution must be clean. Corrected the preflight-budget test expectation after observing Rust Err and metrics.incomplete, which is the existing behavior.
3. Checked prospective/native ambiguity, exact-name precedence among three linked symlinks, configured parent retargeting, shared-directory cache reuse, missing-parent withholding and future ownership-tree collision precedence. Corrected a library expectation that had asked for an unambiguous case alias after intentionally adding a same-inode sibling; the approved policy withholds that uncertain Unix alias. Windows setup skips are explicit, and unexecuted native branches remain disclosed.

Owned files: README.md, docs/development.md, crates/hoimin-cli/src/lib.rs, metrics_destination.rs, shell.rs, session/mod.rs, session/ownership.rs and tests/metrics_destinations.rs. Controller documentation remains unstaged by this task.

## Task-review follow-up: terminal directory-only components

Follow-up commit: `76390dcf95f86e736aa9dfa5d51910bd33bb1064`. This follow-up supersedes the preceding final verification totals for the latest code.

The controller identified `directory-symlink/.` as an original-syntax gap. Installed Rust 1.98.0 `std/src/sys/path/unix.rs::absolute` collects normalized components at line 45 and restores only a literal trailing slash at line 53. Terminal `.` disappears before entry inspection, permitting replacement of the symlink rather than retaining the original directory requirement.

Native CLI RED on temporary fixtures confirmed this: `alias/.` completed successfully but replaced the symlink, while `alias/` and `alias/..` preserved it and warned. The original RED is **1 failed / 2 passed**, `/private/tmp/issue484-terminal-syntax-red.log`, from `cargo test -p hoimin-cli --test metrics_destinations metrics_directory_symlink`. Terminal ParentDir is retained by this Unix normalization; its already-safe behavior remains an explicit control.

The small correction examines the raw terminal byte component before normalization. Empty, `.` and `..` terminal components withhold metrics through the existing late-warning permission path. No new filesystem IO, session behavior or finalizer state was added. The helper receives the platform separator policy; a 14-case pure lexical unit test covers mixed Windows slash/backslash paths, dot-prefixed ordinary filenames, interior `..` and a Unix literal-backslash negative control. Those lexical cases do not claim native Windows execution. The development contract records preservation of directory-only syntax.

Follow-up verification used the same exact serial Cargo environment prefix documented above:

- `cargo test -p hoimin-cli --test metrics_destinations`: **24 passed / 0 failed**, `/private/tmp/issue484-terminal-syntax-green.log`.
- `cargo test --workspace --all-features`: exit 0, **1636 passed / 0 failed / 13 ignored across 70 result groups**, `/private/tmp/issue484-terminal-syntax-workspace-final.log`. One bootstrap-child summary is interleaved with another test line; a strict `test result: ok` regex misses that one successful child. The complete `ok. N passed; N failed; N ignored` summaries give the totals above.
- `cargo fmt --all -- --check`: exit 0, `/private/tmp/issue484-terminal-syntax-fmt-final.log`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0, 4.48 seconds, `/private/tmp/issue484-terminal-syntax-clippy-final.log`.
- `git diff --check`: clean. No code changed after these final checks.

Follow-up implementation self-reviews:

1. Traced raw CLI path syntax through the installed Unix absolute implementation and reproduced loss of the terminal directory requirement. The correction runs before any normalization, at the existing inspection boundary.
2. Reviewed separator and component boundaries: only the final raw component controls this withholding decision; ordinary `.metrics`, `...` and interior `..` retain their existing path handling. Windows treats both separators as separators; Unix preserves literal backslashes.
3. Reviewed permission/error behavior and scope: terminal directory-only paths use Unavailable and the existing late `metrics.write` warning, leaving run results and protected bytes unchanged. Owned preflight completion, collision rejection, shutdown deadlines and canonical writer permission are unchanged. Native Windows/Linux limits remain as documented above.

Follow-up test self-reviews:

1. The actual CLI `alias/.` regression requires successful run completion, baseline execution, surviving symlink entry, unchanged source bytes and a metrics warning. Its old-code failure was the lost symlink, not an unrelated setup or timing error.
2. Kept independent `alias/` and `alias/..` controls so the original failed `/.` case could not hide their pre-fix observations. Shared fixture assertions preserve the original late-warning contract without loosening expected outcomes.
3. Added the lexical separator-policy table independently of native filesystem tests, then ran all 24 destination CLI tests and a fresh full all-features workspace suite. Existing collision, safe-link, absent-parent, session, metrics-failure and lifecycle regressions remain covered. No new Lean claim, delegation, push, PR or cargo-mutants.

## Task-review follow-up

Reviewer review484_task found one P2 issue in4adf809..913101a: terminal `directory-link/.` bypasses the literal trailing-slash guard, while std::path::absolute normalizes it to the symlink entry. Installed Rust1.98 sys/path/unix.rs constructs the absolute path from components and restores only a literal trailing separator. This could replace the directory symlink instead of retaining metrics.write. Sent back for a native CLI RED, bounded syntax-preserving fix and committed follow-up; no test or compatibility waiver. Other reviewed permission, entry/cache, prospective and session behavior matched the design.

Follow-up76390dc preserves terminal directory-only syntax before normalization. Controller follow-up reviews:1 read the raw-component diff against the installed std normalization evidence;2 confirmed real alias/. RED1fail2pass,24 CLI GREEN and explicit alias/.. preservation, with14 lexical cases distinct from Windows native execution;3 independently counted the final70 summaries including the interleaved child (1636 pass/0 fail/13 ignore), read final Clippy and refreshed design/OKF hashes. No new ruling or test waiver.

Task review outcome: review484_task initial one P2 finding addressed by76390dc. Scoped re-review913101a..76390dc clean, with no new actionable findings, deferred items or test waivers. Full task and follow-up evidence retained above.

## Hosted Linux lint follow-up

First normal CI34603735403 failed Linux-only Clippy items_after_statements at metrics_destination.rs:432. Move the unchanged FS_CASEFOLD_FL constant to the beginning of its Linux helper, before statements. No behavior, flag value, ioctl storage, filesystem scope or tests change. Local macOS Clippy cannot type-check this cfg branch; hosted Linux quality is the authoritative follow-up gate.

Three follow-up self-reviews:1 read the exact hosted diagnostic and verify this is the only normal-CI failure;2 inspect the diff to confirm only declaration placement changes, keeping the same c_int type/value and lookup;3 run direct rustfmt/check and whitespace validation, then request scoped review and a new hosted Linux check. Repeating the green macOS full suite would not validate this Linux lint.

Manual native run34603740404: macOS quality, Rust and wheel jobs passed. Windows quality stopped at unchanged workspace_recovery.rs WorkspacePlan import; wheel stopped at six unchanged Lean shell-guard tests (same base blockers observed in #495). Windows native Rust/helper results were still pending when this entry was written. These failures do not establish that new Windows tests executed.

## Integration with the preceding fixes (2026-09-12)

User authorized sequential merging of PR494 through512. This branch integrates prepared preceding head e64855b. Issue472 now opens a canonical session database on every platform; keeping the old Windows configured-basename-only metrics sidecar calculation would miss the actual database's sidecars. Normalizing config before metrics inspection would instead lose protection of the configured symlink entry.

Use the existing SessionArtifacts resolver for metrics' actual database, companions and ownership directory, sharing exactly the paths opened by SessionHandler. Retain the original configured database entry in the protected set; on Windows also retain the configured companion namespace. Inspect once against the original configuration, then prepare canonical session exclusions. Preserve both typed metrics_destination and canonical session_path in OwnedWorkspace completion and install both before publishing the core event. Cleanup completions carry neither replacement. Remove the redundant metrics-specific canonical resolver and ownership helper/export. Known collision still outranks unrelated uncertainty, and uncertain validation cannot grant permission to write. Existing-parent prospective databases remain supported; invalid session paths fail through session.path without creating a database.

Integration cost: Windows retains protection of configured-name companions even when SessionHandler opens a differently named canonical database; these names remain unavailable as metrics destinations. Sharing SessionArtifacts avoids a second, platform-dependent reconstruction of active filenames. No new schema or public configuration field is added.

Three design reviews: (1) compared configured symlink entry versus actual database and all three sidecars on Windows/Unix; (2) traced one-pass validation before normalization and both completion fields before core publication; (3) preserved confirmed-collision/uncertainty precedence and prospective path failure semantics. Independent integration reviewer identified the old Windows unit assertion rejecting the canonical sidecar; updated it to require both namespaces.

Three implementation reviews: (1) removed duplicate path/ownership calculations and retained original configured-entry inspection; (2) verified both cleanup and preflight completion field handling, original source fingerprints and session exclusions; (3) checked cfg-specific filename enumeration, exact deduplication and no extra source API. The combined safe metrics plus in-root session CLI test requires successful baseline, one killed mutant, metrics.executed=1, session artifacts absent in the worker, database created outside the worker, and unchanged original source.

Three test reviews: (1) extended the Windows final-symlink test to configured and actual basenames crossed with WAL/SHM/journal; preserve DB bytes, sidecar sentinel, symlink and absent baseline marker; (2) require Windows symlink setup under HOIMIN_REQUIRE_WINDOWS_SYMLINKS in dedicated acceptance, never count a capability skip there; (3) run affected CLI targets, full cumulative workspace, exact workflow contracts and Clippy. Initial targeted metrics25+run65 passed. Full cargo test --workspace --all-features exited0: raw73 summaries total1724 passed,0failed,13ignored (raw includes subprocess harness summaries). Logs /private/tmp/merge508-metrics.log and merge508-workspace.log.

Dedicated native acceptance: windows-metrics-destinations in non-linux-ci.yml runs on windows-latest with25-minute bound, pinned existing setup and explicit required-symlink environment. It executes scoped native Clippy, metrics_destination::tests unit tests and actual metrics_destinations CLI tests with nocapture. Existing shared manual/automatic steps remain exactly equal. Workflow implementer changed only two assigned files, with three self-reviews for exact mapping/order, independence/preservation, and omission/reordering drift. First contract RED27tests/2expectedfailures then GREEN27; adding unit step RED1expectedmappingfailure then GREEN27 in5.853s. Full log /private/tmp/merge508-native-workflow.log. Native execution is pending dispatch after the reviewed integration commit; its exact result will be recorded in PR508 and the merge ledger before merging this PR.

Three OKF/merge reviews: (1) retain both parents' current claims, source hashes and catalog175; (2) cite this integration record separately from the historical issue484 design/platform evidence; (3) require the cumulative CI-tested tree to equal the actual main merge result, with no force push or check bypass. Prior standalone native limits remain historical and are not counted as this new integration acceptance.

Final local integration validation: full workspace exit0 as above, all-target/all-feature Clippy exit0 (6.66s), fmt/diff checks exit0. Updated the stale Windows unit expectation to require configured and canonical companion sets, with required-symlink setup guard. Dedicated workflow includes both native unit and CLI targets. Native run remains pending; whole-branch integration review follows this commit.

Native validation follow-up: run 34621851957 exposed `clippy::used_underscore_binding` on the Windows-only use of the inspector argument. Use the ordinary argument name and explicitly consume it only on non-Windows builds; protection logic and tests are unchanged. Three self-reviews checked the cfg branches, unchanged path behavior, and propagation/renewed exact-SHA native validation. The failed run is diagnostic evidence, not acceptance evidence.

Native test-oracle follow-up: run 34622566182 passed scoped Clippy, all 4 native metrics units (required symlink setup), and 16 of 17 CLI tests. `metrics_withholds_unsupported_windows_entry_spellings` failed its post-run nonexistence assertion. The original log lacks the loop member, so the exact member is inferred from the fixture: creating session-database.db can also create SESSIO~1.DB as its 8.3 alias. Microsoft documents optional short-name creation in [Naming Files, Paths, and Namespaces](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#short-vs-long-names).

Split the prospective short-alias case into its own test. Preserve initial absence, successful run, baseline marker, warning, source bytes, and all three other unsupported-name absence assertions. For the short alias, reject a separate literal directory entry; if it exists after the run, require canonical identity with the real DB. Always verify the SQLite header, read-only integrity check, exactly one persisted run, and its ID matching the report. This distinguishes legitimate session creation from any metrics overwrite and also works when 8.3 aliases are disabled. Native output will confirm whether the alias was created. No production code changes are justified by this failure.

Three self-reviews: (1) fixture lifecycle and optional Windows alias semantics, with the original failed member explicitly an inference; (2) stronger persistence/identity oracle detects both separate metrics files and DB replacement, while all other assertions remain; (3) read-only SQLite inspection cannot create or repair a missing database, and native logs/renewed SHA remain acceptance gates. A temporary edit insertion error was caught by rustfmt parsing and corrected before review/commit; no such source was pushed.

Native compilation follow-up: run 34624247956/job 103345376431 rejected `u64` for SQLite COUNT because the pinned rusqlite does not implement FromSql for that type. The earlier review assumption was incorrect. Use SQLite's signed integer type `i64`. To check the entire new test body locally, temporarily remove only that test's Windows cfg, run cargo check for the integration target without executing Windows semantics, then restore the cfg in a finally block. Three scoped reviews cover SQL type compatibility, unchanged count==1 assertion, and exact restoration of the platform gate. Native acceptance still requires a new successful run; this compile probe does not establish Windows behavior.
