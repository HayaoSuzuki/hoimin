# Issue #600: Target membership review and measurements

Baseline: `4e3bc2a97cad2e4ce8ae2b11c970fc2f5dff9bb2`. Design and plan were committed separately as `fb7f1f1` before implementation. This report distinguishes author self-review from the root agent's independent review and measured evidence from complexity reasoning.

## Design reviews (before implementation)

1. Read `validate_requested_descriptors`, issue #600, and issue #456 design side by side. A preflight of all memberships would reorder errors, so the design places every lookup at the existing check, after ID resolution and before reads or cached descriptor errors.
2. Read camino 1.2.6 `Utf8Path::eq` and `Hash` implementations and checked the locked dependency. Raw-string hashing would lose component equality; selected borrowed `Utf8Path` keys and spelled out accepted interior-dot/repeated-separator/trailing-separator paths and rejected leading-dot/parent/case boundaries.
3. Compared scope and evidence with all issue acceptance criteria. Added expected (rather than worst-case) hashing complexity, duplicate targets, independent C/F values, and explicit separation between operation counters and hash internals. Public API/CLI timing includes other work and cannot prove a general project speedup.

## Implementation-plan reviews (before implementation)

1. Traced every acceptance condition to a task. Added both-order error combinations and interleaved valid-prefix coverage because a successful cost fixture alone cannot establish failure precedence.
2. Checked planned fixtures against the result-cache key contract. Corrected independent map-key assignment: invalid candidates must have matching map keys and IDs, while successful prefix candidates retain real stable IDs. This prevents a test fixture from panicking in cached-result retrieval.
3. Read the revised plan for executable commands, evidence locations and overclaims. Kept old benchmark data intact, bounded Cargo jobs to two, and required a demonstrated RED from actual old-loop visits before replacing it. The plan explains exactly which measured operations exclude hashing internals.

## Execution ledger

Ruling: the root agent supplies the independent review and owns push/PR creation; this implementer does not spawn agents or publish the branch. The authorized user workflow supplies all execution approval; only the explicit root review handoff is awaited.

## Implementation self-reviews

1. Compared the production diff with the baseline around ID lookup, membership, source reading and cached-result removal. The only semantic operation replaced is target membership; all existing error construction and return positions remain. Verified borrowed keys use `Utf8Path` and output iteration never uses `HashSet` order. No correction was needed after the design's ordering fix.
2. Re-read lifetimes, allocation and work counters independently of the tests. The index borrows targets and owns at most F entries; each target iterator step increments the test counter, and each candidate that resolves increments the query counter. Building an index also costs F visits for empty/unknown requests, but performs no filesystem reads. No O(F+C) worst-case or complete-verify guarantee is claimed. The existing source context and results cache remain unchanged.
3. Read the full production/test diff after restoring the raw-string negative control. Confirmed the source contains `HashSet<&Utf8Path>`, no mutation remains, no ranking/schema constants changed, and no new dependencies or target/output order changes were introduced. `git diff --check` passed. Final suite results are recorded separately below.

## Test self-reviews

1. Compared expected diagnostics against the unmodified implementation. The initial pairwise matrix did not cover a later cached error from an already-read file. Added ten cases with a valid first candidate, each of five middle-file failures, and a later descriptor or stable-ID failure from the first file. All three semantic regression tests passed the old linear scan before implementation.
2. Examined whether the path fixture can catch the plausible raw-string optimization mistake. Temporarily replaced the index keys with strings; the path test failed with `candidate target is not selected: src/calc.py` on an equivalent spelling. Restored `Utf8Path` keys and reran the plan unit suite successfully. This negative control demonstrates the test observes the production membership operation, rather than testing camino in isolation.
3. Reviewed the final fixtures against all five failure classes and independent C/F variation. The 25 ordered pairs include both orders and repeated classes; unreadable-source cases use absent files and compare the path prefix without hard-coding OS wording. Unselected files are also absent, making a premature read observable. Existing empty/unknown/unselected tests retain zero-context/zero-byte assertions. Duplicate equivalent targets still return one requested path and build one source context. Found no additional gap requiring code changes.

## RED and GREEN evidence

The legacy scan with instrumentation failed `target_membership_cost_does_not_multiply_candidates_and_targets` at C=4, F=1: actual target visits 4, expected 1. The full test varies C=1/4/16 independently of F=1/8/64, with all candidates in the final target. After indexing, all nine combinations passed with F target visits, C membership queries and one source context. Counters exclude hash-table-internal probes and path-byte hashing.

Logs: `/tmp/hoimin-issue-600-validation-red.log`, `/tmp/hoimin-issue-600-compatibility-before.log`, and `/tmp/hoimin-issue-600-string-key-negative.log`. The final plan unit run passed 33 tests with two existing ignored benchmarks; public plan integration passed 72 tests with one existing ignored test. Logs for final gates and measurements are kept under `/tmp/hoimin-issue-600-benchmark`.

Ruling: root review requested `cargo clippy --workspace --all-targets --all-features -- -D warnings` in addition to the original plan's gates. It is included in the final run. Root separately owns Python packaging/lint checks, independent branch review, and PR publication.

## Final verification

Implementation commit: `5c5dbea`. `cargo test --workspace --offline --locked` completed with 2,269 passed, 22 ignored across 94 result summaries on this branch. This adds four passing regression tests to the root agent's independently observed baseline of 2,265 passed and 22 ignored. `cargo fmt --all --check` passed. The additional gates also passed:

| Command (all Cargo builds used `CARGO_BUILD_JOBS=2`) | Result |
| --- | --- |
| `cargo test -p hoimin-core --features contracts --offline --locked` | 320 passed, 2 ignored, 26 result summaries |
| `cargo test -p hoimin-cli --features contracts --offline --locked` | 1,934 passed, 20 ignored, 68 result summaries |
| `cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings` | Passed, no warnings |
| `cargo build --release -p hoimin-cli --offline --locked` | Passed |

The root independently reviewed the implementation and regression diff and reported no blocking findings. This supplements, rather than replaces, the 12 author self-review passes above.

The root agent also ran Python checks in this worktree: Ruff format checked 19 files, Ruff lint passed, and the full pytest suite passed 104 tests in 10.05 seconds. An initial sandboxed pytest run had four process-monitor failures because spawning `ps` was denied; the root reran the entire suite with escalated execution successfully. These are root-run results, distinct from this implementer's Rust runs. Logs: `/tmp/hoimin-600-ruff-format.log`, `/tmp/hoimin-600-ruff-check.log`, `/tmp/hoimin-600-pytest-unsandboxed.log`.

## Public release measurements

The [retained observations](2026-09-25-issue-600-target-membership-measurements.json) contain all 18 preparation-API and 18 CLI measurements plus metadata. The input is the original six [issue #600](https://github.com/tokyogas-tech/hoimin/issues/600) fixtures and their existing plans in `/tmp/hoimin-perf-audit`: 10,000 candidates concentrated in `a.py` or `z.py`, with 1,000/4,000/10,000 total targets. No historical measurement file was overwritten.

Built this worktree with `CARGO_BUILD_JOBS=2 cargo build --release -p hoimin-cli --offline --locked`. The API harness is the issue's `prepare.rs`, linked to this worktree's release library with `rustc --edition=2024 -O -Clto=thin`; it additionally asserts selected count 10,000. The CLI runs `target/release/hoimin verify PLAN --top 10000`. Both alternate first/last order across three repeats at each size. Every API call succeeded; every CLI result had exit 3, baseline `Exit(1)`, selected count 10,000 and zero mutants.

Environment: macOS 15.7.7 arm64, rustc 1.98.1 (48a229cea 2026-09-01), benchmark driver Python 3.12.6. Production code is commit `5c5dbea`; only documentation was modified during timing. Binary SHA-256: `4ad6d8c92cb916c7c1fce9f6075e67957a0afac7b2c01cbb8e842915838c1dd2`. The original binary hash is recorded in the issue as `3a45b3648ec9205b19da9ac5fea39d40a518f6ca72e8a830f018d4178dadd054`.

Medians in seconds; “historical” is the published baseline observation, not a fresh paired run:

| Targets | Position | API historical | API indexed | CLI historical | CLI indexed |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1,000 | first | 0.113 | 0.129 | 0.624 | 0.732 |
| 1,000 | last | 0.247 | 0.113 | 0.839 | 0.686 |
| 4,000 | first | 0.306 | 0.299 | 2.240 | 2.574 |
| 4,000 | last | 0.786 | 0.244 | 2.833 | 2.537 |
| 10,000 | first | 0.470 | 0.541 | 5.549 | 6.506 |
| 10,000 | last | 1.824 | 0.698 | 7.150 | 6.311 |

At 10,000 targets the historical API first/last gap was 1.354 seconds; the new observation is 0.157 seconds. The CLI no longer shows a last-file penalty in these medians, but its first-file median is slower than the historical result. The full CLI includes scanning, copying, managed-root checks, baseline startup and cleanup; it is not a membership microbenchmark. The 10,000-target first CLI observations ranged from 6.337 to 7.386 seconds. Other issue work ran on the same host during the overall verification session; this worktree's compilation was complete before timing, and the root postponed its next wheel build until timing ended. There was no CPU isolation or controlled fresh baseline, so no general speedup ratio is asserted.

The operation-count regression is the deterministic acceptance evidence for eliminating the candidate×target scan. Hash-table collision behavior, allocations/RSS, complete-verify complexity and native Linux/Windows executions were not measured. No Lean theorem is claimed for the Rust implementation or timing. Source reads, descriptor checks, rediscovery and selected order remain covered by the ordinary tests.

Reproduction scripts and raw CLI stdout/stderr are preserved in `/tmp/hoimin-issue-600-benchmark` (`benchmark.py`, `prepare.rs`, `prepare-results.jsonl`, `results.json`, and `verify-*.json`). The linked JSON preserves all timing observations and validated CLI outcome fields in the repository; the issue contains the original fixture generator and API harness for reproducing the experiment independently.

## OKF scope and limits

Consulted `docs/knowledge/overview.md`, `docs/knowledge/index.md`, `docs/knowledge/design/selection-plan-verify.md`, `docs/okf-workflow.md`, the issue #456 design/report, the affected implementation and camino 1.2.6 source. Updated the existing selection concept and the design/report source lists next to their #456 entries. No new duplicate concept or schema/ranking contract was introduced.

Validation uses PyYAML for all 29 knowledge Markdown files and reserved-file rules, checks all 786 local links in the three changed knowledge pages, checks new #600 source IDs/footnotes/hashes, confirms design/report source-list coverage, and traverses the entry index to all 29 pages. The Japanese additions were read for distinct design, implementation and measurement claims; they preserve expected hashing assumptions and exclude complete-verify complexity guarantees. `status: draft` remains unchanged.

The audit source list already contains 12 source IDs without matching footnote definitions: four `evaluation-order-*`, four `annotation-followup-*`, and four `with-finally-*`. `git show 4e3bc2a:docs/knowledge/references/audit-documents.md` confirms all 12 predate this change. They remain outside this issue's edits and are reported as inherited maintenance findings; the source/footnote success claim applies to the new #600 entries. Historical source hashes are provenance records and were not rewritten en masse.
