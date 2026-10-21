# Issues #1065–#1068 implementation and evidence

Work continues on `audit/issues-1065-1068`, the branch that retained the
[post-1064 audit](../../knowledge/audits/design-2026-10-22-post1064.md).
The static audit describes main `d1bf54987cb196b2bcb497fffe763ab06420fd66`;
implementation starts after audit commit `9de5460f5706289a8962845a5873f2e924be2967`.
Those earlier source hashes and findings remain historical evidence.

## Changes and preserved boundaries

- **#1065:** A private `SourceScope` distinguishes unspecified sources, the whole
  root, one source, and an indexed set. Multiple-root queries derive one platform
  equality key and probe component ancestors. The one-root path uses a borrowed
  prefix boundary without constructing a slash-suffixed string. The original
  ordered sources still determine symbol module precedence. Empty-source exact
  file/line selection remains unrestricted, while whole-file admission is empty.
- **#1066:** Descriptor-length `SnapshotReader` moves below artifact and workspace
  policies to `finite_file`. Source read/hash now share its initial-length budget
  and one-byte growth probe. Existing artifact routing/duplicate readers use the
  same primitive. Stable files have no new fixed cap; in-place rewrites, atomic
  snapshots and arbitrary filesystem I/O time bounds remain unsupported. Copy
  streaming and bounded exception-hierarchy input policy are unchanged.
- **#1067:** Windows completion derives from active-registration absence, removing
  the completed UUID set and its interest flag/updates. Signaled fallback retains
  the UUID/PID/owned handle until packet consumption. Both notification drains,
  retry after timeout/error, oldest-PID order, native empty-job confirmation,
  nested caps and attach-failure ownership remain required.
- **#1068:** Current implementation guidance follows actual owners, while recovered
  historical source bytes are pinned separately from original comparison
  revisions. Of 24 source entries, 20 match recorded SHA-256, two are hashless
  recorded-revision matches, and two remain explicitly unavailable. See the
  [recovery inventory](../2026-10-22-issue-1068-knowledge-links.md). The read-only
  reference checker follows used references for reachability; unused definitions
  remain inventoried without making pages navigable.

No new product settings, dependencies, saved-format adapters or migrations are
introduced. Existing proof sources remain; the Windows packet/generation details
are not newly claimed to have formal correspondence.

## Regression evidence

Before the membership change, the 32-root/32-file fixture performed 528 probes
and failed its linear-work bound. The prepared index passes the independently
varied 32/128/512 root and file cases. An additional depth fixture varies path
depth 1/8/32, requiring one equality key per query and a number of ancestor probes
proportional to path depth. Literal and linear-reference cases cover absent/all,
overlapping roots, duplicates, component boundaries and platform case handling;
existing target/symbol/changed suites remain in the workspace suite.

Before finite source I/O, production `SourceReader::read` and fingerprint glob
resolution accepted a finite append and failed the new tests. After the change,
plan read/hash and exact/glob fingerprint tests reject append and truncate with
the expected length-change errors. Primitive tests observe only the original
length plus one byte; stable source fixtures preserve bytes and hash across empty,
single-byte and 128-KiB files. No sustained writer or unbounded fixture is used.

Windows tests now assert repeated signaled fallback retains the identical owned
`Arc`, actual retry after timeout/error, notification arrival during the unlocked
wait, delayed exit with a reused PID, and handle release after confirmed job-empty.
Representation-only completed-set/interest assertions were replaced by these
observable ownership/wait outcomes. Native Windows execution is required; macOS
execution or cross compilation alone does not establish it.

The reference checker was reviewed independently. An unused-definition
reachability defect was reproduced with two failing fixtures, then corrected;
the five small Python fixtures cover sources, local links, missing entry point,
unused definitions and used reference/footnote navigation. Inventory structure
checks do not reverify all historical prose or authenticate external URLs.

## Resource and verification policy

Local Cargo commands use `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_INCREMENTAL=0`, with one Cargo command active at a time and one worktree
`target` directory. Dependencies use the locked versions. Reuse the root Python
environment through the existing `.venv` symlink. Record verification results
below and keep platform execution distinct from static review.

## Local validation (macOS arm64, Rust 1.99.0, Python 3.14)

- `cargo test --locked --workspace`: 3,948 passed, 34 ignored across 188
  libtest summaries; no failures. Later additions were checked separately:
  target unit tests 11 passed/3 ignored, target heap test 1 passed, and plan source
  tests 3 passed. Ignored cases include explicit release measurements, not failed
  or implicitly executed cases.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`:
  pass. Workspace and vendored parser `cargo fmt -- --check`: pass.
- Full Python suite: 1,231 passed, 11 skipped. The initial sandboxed run had six
  environment failures: release preparation could not write uv cache (two), and
  process-resource guard tests could not use process monitoring (four). Repeating
  outside the sandbox passed. Full ty, Ruff format and Ruff lint passed after
  syncing the locked optional fuzz group used by CI quality checks. No dependency
  definitions or lockfiles changed; the root development CLI was restored after
  the tooling-only sync.
- Safe YAML/reserved-file structure: 53 knowledge pages. The reference inventory
  has zero missing local Markdown targets, zero missing local source resources
  and zero unreachable pages. All 22 recovered commit/blob identities and all 20
  recorded recovery hashes were independently checked. New tool fixtures: five
  passed. The allowlisted public docs site generated successfully; it does not
  validate all knowledge claims or external URLs.
- A bounded hoimin check planned three `boolean_literal` candidates in the new
  reference tool and verified only the missing-Markdown filter mutation
  `m1_8ed12e332f3dc9a0cfe2a78dab66efee3ff7ac8126bb6e67e9772a77c56b3299`.
  Fresh baseline passed, the mutant was killed, and the complete verification
  exited 0. This is one selected contract check, not a saturation or full-score
  claim. Limits were one worker, 8-GiB owned workspace cap and 10-GiB free-space
  reserve, with macOS best-effort memory explicitly enabled. Peak owned workspace
  bytes were 73,046,763; execution cleanup was clean. Temporary plan/report files
  were removed by the exact-directory cleanup trap.

### Source membership measurements

The ignored release unit fixture repeats each membership batch 100 times. The
linear control retains the former prefix/case-key work; prepared timing includes
building the source-scope index each repetition. Inputs and expected admissions
are identical. These are small local component measurements (including unit-test
counters), not CLI end-to-end improvement percentages or a broad benchmark.

| Roots | Files | Linear control (µs) | Prepared, including build (µs) |
| --- | --- | --- | --- |
| 1 | 16 | 123 | 17 |
| 32 | 512 | 45,955 | 1,250 |
| 128 | 512 | 169,562 | 1,413 |
| 512 | 512 | 686,136 | 1,918 |

Reproduce with `cargo test --locked --release -p hoimin-core --lib
measure_source_membership_scaling -- --ignored --exact
target::tests::measure_source_membership_scaling --nocapture` and the resource
settings above. Normal variation is expected; the normal regression gate checks
operation counts, not timing thresholds.

The isolated `target_scope_heap` test meters actual requested bytes on the calling
thread, with input fixture construction excluded. Its production resolver window
includes normalized sources, all lookup indexes and target output. It observes
retained output bytes before dropping targets; it is not process RSS or an
isolated index measurement. No memory-reduction claim is made.

| Roots | Files | Resolver peak additional requested bytes | Retained output bytes |
| --- | --- | --- | --- |
| 1 | 16 | 8,775 | 1,456 |
| 32 | 512 | 240,264 | 46,592 |
| 128 | 512 | 248,040 | 46,592 |
| 512 | 512 | 279,144 | 46,592 |

Reproduce with `cargo test --locked -p hoimin-core --test target_scope_heap --
--nocapture`. The multi-root index trades O(S) keys for fewer probes and prefix
allocations; these bytes are distinct from the fixture's reported hash-table
capacity.

### Native Windows validation

Pending at implementation publication. Run the existing `non-linux-ci.yml`
workflow on this branch and record the code revision and native results before
claiming Windows validation. This follow-up has no new Windows proof claim.
