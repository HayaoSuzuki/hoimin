# Task 1 (#1003) independent review

Base: `ccef1941710e20cff8d391283cc21bcfbd7650aa`.
Head: `451a4fdd8f6d91007738798a4f17ead601254b05`.
Scope: Task 1 only. Reviewed the supplied brief, execution report and complete review-package diff once, following `subagent-driven-development/task-reviewer-prompt.md`. No Git commands, Cargo operations, test reruns, product edits, index/ref mutations or subagents were used.

## Spec compliance

**✅ Spec compliant for the Task 1 behavior visible in the diff.**

- Both formerly direct project-source reads now use the same `SourceReader` acquisition boundary: source hashing at `crates/hoimin-cli/src/plan.rs:673`, requested-candidate bytes at `crates/hoimin-cli/src/plan.rs:1012`. The shared reader delegates to `PortableFileReader`, without another filesystem opener (`crates/hoimin-cli/src/plan/source.rs:23`).
- Hashing returns a BLAKE3 hash instead of allocating a source-sized byte vector (`crates/hoimin-cli/src/plan.rs:673`). The narrowly exposed method still invokes the existing safe streaming implementation (`crates/hoimin-cli/src/workspace/reader.rs:56`; unchanged `workspace/root.rs:283`).
- One lazy root capability is retained for the operation, and root opening and file operations execute through `spawn_blocking` (`crates/hoimin-cli/src/plan/source.rs:39`, `:61`). Candidate validation retains one file's bytes/context within its existing per-file scope (`crates/hoimin-cli/src/plan.rs:1021`).
- Source ordering/dedup remains the existing BTreeMap behavior (`crates/hoimin-cli/src/plan.rs:669`). Requested-ID membership and selected-target checks still precede acquisition, and deferred descriptor results retain their prior ordering (`crates/hoimin-cli/src/plan.rs:996`, `:1044`). Caller-specific error variants and source-path context remain explicit (`crates/hoimin-cli/src/plan.rs:673`, `:1012`; `plan/source.rs:53`). The missing-file wording change is visible and documented, rather than hidden behind a compatibility branch (`plan/validation_tests.rs:598`).
- The added source-swap tests use actual target resolution, parent/child pipe synchronization and separate generated cases for both consumers and three replacement types; timeouts only guard failure (`crates/hoimin-cli/src/plan/validation_tests.rs:717`). Normal sorted hashes, changed-source rejection and retained-root reuse are covered in the same added test block.
- The new public-plan integration test checks independently computed raw-byte hashes and bounds the measured source-stage extra live heap for 1/16/64 MiB files (`crates/hoimin-cli/tests/plan_source_heap.rs:45`, `:84`). Guidance and the relevant OKF concept describe the boundary and its evidence limits (`docs/usage.md:254`; added Issue #1003 section in `docs/knowledge/design/selection-plan-verify.md:996`).
- No schema, fingerprint, candidate-identity, exit-policy, command or formal-proof change appears in this task diff. The report supplies five concrete review lenses for each required stage and identifies the implementation commit and RED/GREEN evidence.

**⚠️ Cannot verify from this task diff / review scope:**

- Native Linux/Windows success is not established. The new swap cases are Unix-gated and execution evidence is native macOS; the report and documentation state this limit. The controller should retain native-platform validation as a separate concern, without interpreting these results as native Windows/Linux evidence.
- Branch ancestry/isolation and combined catalogs, release-floor/schema consistency, preservation of #999 outside this task, and full-batch integration are controller-owned checks. The supplied diff does not modify those production concerns; this review did not broaden into a branch audit.
- The report says automatic commit hooks passed but did not explicitly set the task's Cargo environment variables. Their effective environment and Cargo resource limits cannot be established from this diff or the focused logs inspected. The controller should account for this process-evidence limit; it does not identify a source defect.

## Strengths

- The private source module hides asynchronous ownership, lazy root acquisition and error mapping while keeping portable-path and descriptor policy in the existing reader (`crates/hoimin-cli/src/plan/source.rs:10`, `:31`). This is a small boundary with two justified operations.
- Both consumers reuse the proven open policy instead of duplicating no-follow or regular-file checks. Unix `with_read_file` uses a retained parent, no-follow/nonblocking open, and metadata from that same opened descriptor before consumption (unchanged `crates/hoimin-cli/src/workspace/root.rs:293`, `:316`, `:332`).
- The resource regression observes public production stage events and a process-wide allocator, so blocking-thread allocation contributes to the measured peak; source fixture allocation and analyzer work are outside the window (`crates/hoimin-cli/tests/plan_source_heap.rs:17`, `:37`, `:74`; unchanged `tests/support/heap_tracking.rs:5`, `:65`).
- The comments and user guidance correctly disclose that a started blocking filesystem operation can outlive cancellation (`crates/hoimin-cli/src/plan/source.rs:9`; `docs/usage.md:259`).

## Issues

### Critical (must fix)

None found.

### Important (should fix)

None found.

### Minor (nice to have)

None raised. The platform and batch-wide limits above are verification boundaries, not defects introduced by this diff.

## Focused checks beyond the diff

Every extra product-code inspection addressed a named risk:

1. **Risk: forwarding to an existing helper might still permit blocking FIFO opens, symlink traversal or source-sized hash buffering.** Checked unchanged `PortableFileReader::open/read/classify_read` (`workspace/reader.rs:21`, `:25`, `:70`) and `WorkerRoot::read/hash/with_read_file/open_parent` (`workspace/root.rs:274`, `:283`, `:293`, `:1159`). Both operations converge on the same safe descriptor boundary; hashing passes the descriptor to `Hasher::update_reader`. Parent traversal also uses retained capabilities and `open_dir_nofollow`. The `reader.rs` diff hunk cuts off inside path validation; the focused inspection supplied that boundary's omitted context.
2. **Risk: a shared async reader might alter requested-ID error priority or retain several full sources.** The `plan.rs` changed hunk ends inside the validation loop, so inspected the complete `validate_requested_descriptors` function and its immediate caller context (`plan.rs:969` onward). Deferred-result removal and source/context lifetime remain unchanged; reader construction performs no I/O before membership checks.
3. **Risk: the allocation gate might miss allocations on `spawn_blocking` workers.** Checked the existing allocator support only (`tests/support/heap_tracking.rs:5` onward). Its counters are process-wide statics and used by the binary's global allocator, not thread-local counters; the single-test binary avoids competing test measurement windows.
4. **Risk: claimed RED/GREEN evidence might be absent or noisy.** Read result/error/warning lines from the named existing logs, without regenerating results. Final plan units: 97 passed/3 ignored; root safety: 30 passed; public plan: 114 passed/1 ignored; source encoding: 6 passed; missing-source: 3 passed; final heap: one passed with 1,971/1,248/1,248-byte peaks. Symlink RED, FIFO RED, heap RED and FIFO-symlink mutation RED each contain the claimed failures. The inspected successful log sets contain no `warning:` or `error:` entries, including targeted clippy. Expected RED failures are intentional evidence, not outstanding GREEN failures.

A final location-only symbol lookup supplied exact report references; it did not reopen the diff or broaden the review.

No additional execution was needed to resolve a concrete unanswered behavioral doubt. No suites were rerun.

## Assessment

**Task quality: Approved.**

The actual diff closes both unsafe acquisition paths through the existing descriptor boundary and removes whole-source hash retention without changing candidate validation semantics. The added synchronization and heap regressions exercise the relevant production consumers, and the available RED/GREEN evidence supports their sensitivity within the stated macOS scope.
