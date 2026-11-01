# Claude Code Fable5 Issue Triage

## Scope

This report originally evaluated the 75 open issues in
[`tokyogas-tech/hoimin`](https://github.com/tokyogas-tech/hoimin/issues) as of
2026-07-31. Issues #96 through #168 were filed as one large investigation
batch; #64 and #79 were already open and were included because they affect the
same prioritization decision.

The current issue state below was rechecked on 2026-08-05. “Resolved” means the
GitHub issue is closed; it is a queue-status update, not a second audit of the
implementation.

The goal is to distinguish actionable product bugs from performance work,
test gaps, documentation, and feature requests, then put correctness bugs
first.

The review used:

- issue bodies and their stated failure scenarios;
- the current `main` tree at commit `1903e0d`;
- code-path tracing against the cited implementation;
- relevant OS and database API semantics;
- the existing automated test suite.

Platform-specific races were not promoted to reproduced defects unless the
code path and platform semantics were sufficient to establish the failure.

## Assessment Legend

| Assessment | Meaning |
| --- | --- |
| Confirmed | The current code establishes the reported cause and effect, or the issue contains a concrete reproduction consistent with that code. |
| Likely | The failure mechanism is credible, but an OS-specific, timing-sensitive, or injected reproduction should be added before changing behavior. |
| Policy | The current behavior is real, but whether it is a defect depends partly on a product or diagnostic-policy decision. |
| Valid improvement | The issue identifies useful work but not a current product correctness defect. |
| Needs scope | The concern is useful, but the proposed issue is too broad or costly as written. |

## Executive Summary

The 75 issues break down as follows:

| Kind | Count | Disposition |
| --- | ---: | --- |
| Bugs | 44 | 23 P1, 21 P2 |
| Performance | 6 | Valid; address after P1 correctness work |
| Tests | 16 | Mostly fold into the related bug fixes |
| Features | 4 | P3 |
| Documentation | 2 | P3 |
| Refactoring | 1 | P3 defense in depth |
| Release policy | 2 | #79 is P1 before tagging; #64 is P3 |

No bug report was found to be clearly fabricated or invalid. Several need
fault injection or platform reproduction, and some proposed fixes need
adjustment, but their underlying concerns remain credible.

There is no P0 incident in the current repository state. P0 should be reserved
for active compromise, current data corruption, or an ongoing irreversible
release. Issue #79 becomes release-blocking before any `v*` tag is pushed.

## Resolved vs outstanding issues (2026-08-05)

The original 75-issue triage set now contains 65 closed issues and 10 open
issues.

### Resolved (65)

| Group | Issues |
| --- | --- |
| P1 and release safety | #79, #96–#98, #101–#102, #109, #111–#114, #116, #119, #121, #140–#146, #149–#151 |
| P2 bugs | #99–#100, #103–#108, #110, #117–#118, #120, #122–#123, #130–#133, #147–#148, #152 |
| Performance | #124–#129 |
| Test issues | #153–#156, #158–#161, #163–#168 |

### Outstanding (10)

| Group | Issues | Current disposition |
| --- | --- | --- |
| P3 release/refactoring/docs/features | #64, #115, #134–#139 | Keep in the lower-priority queue; decide and implement separately from the completed correctness/performance work. |
| Test follow-up | #157, #162 | Keep open for the remaining end-to-end and fault-injection coverage. |

Issues outside this report’s original 75-issue set are not included in these
counts.

## P1: Fix First

P1 covers release safety, false success, wrong mutation selection, hangs,
session concurrency, report integrity, and realistic platform failures.

| Issue | Assessment | Reason for P1 |
| ---: | --- | --- |
| #79 | Confirmed | Every matching `v*` tag reaches a PyPI publisher with `id-token: write`, while the documented policy says publication must first be enabled and protected. The `pypi` environment was not visible through the repository API during review. |
| #96 | Confirmed | Raw token-text matching admits f-string middle tokens, producing arithmetic and boolean mutants inside string content. |
| #97 | Confirmed | The Windows Job Object backend handles normal exit notification 7 but not abnormal exit notification 8, so crashed roots can time out during classification. |
| #98 | Confirmed | `memory.max` readback is compared byte-for-byte even though cgroup v2 rounds non-page-aligned values such as decimal `1GB`. |
| #101 | Confirmed | Diff parsing treats any `--- `/`+++ ` line as a file header, including content within a zero-context hunk, and can misattribute or drop changed lines. |
| #102 | Confirmed | Progress comparison excludes duplicate content keys even when identical candidate-ID sets provide exact pairing, allowing a real regression to be reported as stalled. |
| #109 | Confirmed | After the first Ctrl+C is consumed, cleanup and drain paths do not poll another signal future, so a second Ctrl+C cannot force termination. |
| #111 | Confirmed | Cancellation during ordered collection resumes spool reads and later checks an unpopulated unordered match set, producing `SelectedCandidateMissing` instead of exit 130. |
| #112 | Confirmed | Ordered verification finalizes successfully when analysis returns zero records, unlike the unordered path, and can report that selected candidates were verified when none ran. |
| #113 | Confirmed | A worker in `Persisting` already has a classified result but cancellation drains it as `NotRun`, allowing the report and session database to disagree. |
| #114 | Confirmed | `ReportSequence` does not validate coherence between mutation status and process termination; this also weakens detection of #151. |
| #116 | Confirmed | A root-equal source selector can normalize to an empty prefix and silently select no Python files. |
| #119 | Confirmed | Analyzer diagnostics are not surfaced through the run path, making analysis failures look like an ordinary lack of candidates. |
| #121 | Confirmed | The include-glob manifest walk does not consistently apply built-in exclusions and can copy `.git`, virtual environments, and caches. |
| #140 | Confirmed | Session write transactions are deferred and read before writing, which permits un-retryable `SQLITE_BUSY_SNAPSHOT` failures under WAL contention. |
| #141 | Confirmed | Verify freezes fingerprint-input hashes before rediscovery and copy; the run then trusts those records without re-reading the executed configuration files. |
| #142 | Confirmed | An incomplete session row has no ownership or liveness marker, so another process can resume a run that is still active. |
| #143 | Confirmed | Schema version is read before a deferred, non-idempotent migration, so concurrent opens can both select the same migration and race. |
| #144 | Confirmed | Worker-tree file opening does not protect against a planted POSIX FIFO blocking indefinitely. |
| #145 | Confirmed | Recursive hostile-tree walks have no depth bound or explicit heap stack and can overflow the process stack. |
| #146 | Likely | Legal non-UTF-8 directory entries pass into cleanup paths that require UTF-8 conversion; reproduce on Unix before selecting the exact removal strategy. |
| #149 | Confirmed | Serialized `verificationSelection.policy` is absent from a published schema that rejects additional properties. |
| #150 | Confirmed | A typed current `RunConfig` rejects older schema-v2 reports that lack fields added under the same schema version. |
| #151 | Confirmed | The session-backed result path drops `termination`, making its public report differ from an equivalent sessionless run. |

### Recommended P1 Work Order

1. Disable implicit publication in #79 before any release tag.
2. Fix false success and wrong selection: #112, #96, #101, #116, #119, #121,
   and #102.
3. Fix worker-tree hangs and unrecoverable cleanup: #144, #145, and #146.
4. Fix session concurrency and provenance: #142, #140, #143, and #141.
5. Fix report/schema integrity: #151, #114, #149, and #150.
6. Fix cancellation and persisted-result handling: #109, #111, and #113.
7. Fix platform classification and enforcement: #97 and #98.

Independent items within a step can be implemented in parallel. Related items
should share regression fixtures where that reduces duplicate setup.

## P2: Valid Bugs After P1

| Issue | Assessment | Rationale |
| ---: | --- | --- |
| #99 | Likely | PID reuse can confuse Windows root tracking, but the exact eviction interleaving needs stress or fault injection. |
| #100 | Confirmed | `waitpid(WUNTRACED | WNOHANG)` may reap a Tokio-owned launcher. The suggested fix must use an ownership-safe API such as `waitid(WNOWAIT)`; simply adding `WNOWAIT` to `waitpid` is not a valid portable fix. |
| #103 | Confirmed | Explicit-line ranking compares user-shaped paths with normalized candidate paths and can lose the ranking boost. |
| #104 | Confirmed | Human progress output can combine the latest indeterminate state with statistics from an older usable comparison. |
| #105 | Confirmed | Splitting a binary-diff sentence with `rsplit_once(" and ")` is ambiguous for filenames containing that text. |
| #106 | Confirmed | `CandidateStore::finish` keeps the temporary spool, but normal, failure, and cancellation cleanup have no owner that removes it. |
| #107 | Confirmed | Help and version output are routed through stderr even on successful exits. |
| #108 | Confirmed | One transition-error shutdown branch lets a drain failure replace the original transition failure instead of combining them. |
| #110 | Confirmed | Hard run failures can omit both the requested metrics file and the diagnostic explaining why it was not written. |
| #117 | Confirmed | Plan serialization uses `type_mapping` where the public operator ID is `type_dict_mapping`. Compatibility handling is required. |
| #118 | Confirmed | Operator normalization can produce an empty selection that is accepted as a high-cost no-op run. |
| #120 | Policy | Head-only output retention is implemented, but changing to head-plus-tail is primarily a diagnostic policy choice. |
| #122 | Likely | A failed Windows worker copy can leave read-only files that `TempDir` cannot remove; verify using failure injection. |
| #123 | Confirmed | Root-level package extraction in `focused_mutation.py` can index a missing path component and leave records pending. |
| #130 | Confirmed | Focused-mutation baselines use the discovery deadline even though they run in the mutation phase. |
| #131 | Confirmed | Windows focused-mutation termination kills the cargo root rather than the full descendant tree. |
| #132 | Likely | Locale decoding of Git output is unsafe on Windows code pages; reproduce with a non-ASCII path under the target locale. |
| #133 | Confirmed | In-band focused-mutation failures set a terminal run state without marking pending candidates `not_run`. |
| #147 | Likely | Case-colliding manifest paths can collapse on case-insensitive snapshots; reproduce with the supported Windows filesystem configuration. |
| #148 | Likely | An error after charging a successful copy but before constructing the owning workspace can strand the allowance; confirm through injected construction failure. |
| #152 | Confirmed | Out-of-range octal escapes can overflow or wrap during quoted Git path decoding. |

## Performance Work

All six performance reports describe credible algorithmic costs. They should
be benchmarked before and after changes, and should follow correctness P1 work.

| Order | Issue | Finding |
| ---: | ---: | --- |
| 1 | #124 | Two full original-tree hash passes per mutant. |
| 2 | #125 | Multiple full worker/snapshot reads per reset despite one sanctioned dirty path. |
| 3 | #126 | Synchronous workspace and spool I/O blocks the asynchronous dispatch loop. |
| 4 | #127 | Candidate replay re-reads and parses up to 2 MiB behind every offset. |
| 5 | #128 | Worker materialization repeats full-tree hashing already performed during preflight. |
| 6 | #129 | Line/column calculation rescans source prefixes for every candidate. |

Issue #166 should accompany #127 as a format and offset regression guard.

## Test-Issue Consolidation

Most test issues should become acceptance criteria of the related bug rather
than run as independent high-priority projects.

| Test issue | Fold into |
| ---: | --- |
| #153 | #109, #111, and #113 |
| #154 | #150 |
| #155 | #142 and #151 |
| #156 | #101 and #105 |
| #158 | #107 |
| #161 | #140 and #143 |
| #162 | #97 and #99 |
| #164 | #111, #112, and #113 |
| #165 | #101, #105, and #152 |
| #168 | #102 |

The following test issues remain useful as independent follow-up work:

- #157: full-stack timeout, OOM, process-limit, and exit-code coverage;
- #160: checked-in compatibility artifacts for public schema eras;
- #166: candidate-spool round-trip and boundary properties;
- #167: broader fingerprint canonicalization properties.

Issue #159 is useful as an umbrella for adversarial fixtures but is too broad
for a single implementation change. Keep it as an epic or split it only when a
concrete consumer needs a shared fixture.

Issue #163 needs narrower scope. Requiring provenance comments on every exact
assertion has a high maintenance cost; comments should be added only where a
test intentionally pins a surprising policy or a former defect.

## Remaining P3 Work

| Issue | Assessment | Disposition |
| ---: | --- | --- |
| #64 | Valid improvement | Decide the macOS binary-distribution policy before enabling public wheel publication. |
| #115 | Valid improvement | Enforce `max_mutants` in the state machine as defense in depth; the current CLI already gates the ordered path. |
| #134 | Valid improvement | Replace the non-executable Rust-file documentation example. |
| #135 | Valid improvement | Improve human output with actionable mutation details. |
| #136 | Valid improvement | Make runtime operator IDs discoverable. |
| #137 | Valid improvement | Correct macOS memory-enforcement documentation and the related skill step. |
| #138 | Valid improvement | Improve validation diagnostics with flag names and accepted formats. |
| #139 | Valid improvement | Add shell completions as a new feature. |

## Suggested Repository Labels

No labels were applied during this review. A small label set would make the
queue actionable:

- `priority/P1`, `priority/P2`, `priority/P3`;
- `kind/bug`, `kind/performance`, `kind/test`, `kind/docs`, `kind/feature`;
- `status/confirmed`, `status/needs-reproduction`, `status/umbrella`;
- platform labels for `windows`, `linux-cgroup`, `unix-filesystem`, and
  `sqlite`.

Test issues folded into bug fixes can either be closed as duplicates after
their acceptance criteria are copied, or left linked until the regression test
lands.

## Verification and Limitations

A fresh local verification completed successfully:

```console
cargo test --workspace -q
```

All executed tests passed; the suite reported one intentionally ignored
subprocess fixture. This does not disprove the issues above: most describe
input classes, process interleavings, old serialized artifacts, or operating
systems that the current suite does not exercise.

No Windows Job Object, delegated Linux cgroup, case-insensitive filesystem, or
concurrent cross-process session reproduction was run during this triage.
Issues marked Likely should gain a focused failing test or fault-injection
reproduction before their production fix.

The review did not modify issue labels, issue bodies, milestones, or production
code.
