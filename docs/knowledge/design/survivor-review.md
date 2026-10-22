---
type: Contract
title: survivorの評価と実行結果の分離
description: 候補の同定、評価履歴、stale判定、実行結果へ影響させない境界の実装契約とモデル証明の範囲。
status: draft
catalog_revision: d46f13a43667323b8d0374d6c5409942f4c3c822
sources:
- id: issue-1089-core
  resource: ../../../crates/hoimin-core/src/candidate.rs
  revision: 5064140df8e8f24df42045664d72dbf887ef8372
  working_tree: modified
  sha256: ce80a074580637f18b9e9b62a3b161ea59dc4c30b36e94230f3d74966c1f1eb2
- id: issue-1089-review
  resource: ../../../crates/hoimin-cli/src/review/mod.rs
  revision: 5064140df8e8f24df42045664d72dbf887ef8372
  working_tree: modified
  sha256: a1989068bd5a9e1d8c48f3eae9037635674f848f7bcb24c9b0dc457b04fcc8fc
- id: issue-1089-input
  resource: ../../../crates/hoimin-cli/src/review/input.rs
  revision: 5064140df8e8f24df42045664d72dbf887ef8372
  working_tree: modified
  sha256: 249dcfb03394849dcaf5952377b9e02c929666893643c2717f15340b2b054f26
- id: issue-1087-rows
  resource: ../../../crates/hoimin-cli/src/review/mod.rs
  revision: 11eb28d3ff43c441c3aabc78cd7fa6d7d0c72493
  working_tree: modified
  sha256: bcf44365e9d8cab492b07d57c6b2529d0b3699e00887282f114e80e3a011e1b6
- id: issue-1087-render
  resource: ../../../crates/hoimin-cli/src/review/render.rs
  revision: 11eb28d3ff43c441c3aabc78cd7fa6d7d0c72493
  working_tree: modified
  sha256: eb160caac884aa692371f93705aaea4c0b045db7731aa2f1bd711565ec3998c8
- id: issues-1050-input
  resource: ../../../crates/hoimin-cli/src/review/input.rs
  revision: 5265aa2cc9f36b6b98ded50c6823039d8999b665
  working_tree: modified
  sha256: ffe19e39affe2ebe4ca68a6cadfe53c8a81503ead04312cb8e2784234c1d20c7
- id: issues-1050-reader
  resource: ../../../crates/hoimin-cli/src/workspace/reader.rs
  revision: 5265aa2cc9f36b6b98ded50c6823039d8999b665
  working_tree: modified
  sha256: 9b4abde6b56fe707e756b44f50fc41a1efdf63a5500cf5eabbfb7ad50530b830
- id: issues-1050-boundary-tests
  resource: ../../../crates/hoimin-cli/src/review/input_boundary_tests.rs
  revision: 5265aa2cc9f36b6b98ded50c6823039d8999b665
  working_tree: untracked
  sha256: 005dbdf4fba6717c4cdd0d92978fe804dfeb9f1c7b4b5058bcec4ca4e0e78eae
- id: issues-1050-store
  resource: ../../../crates/hoimin-cli/src/review/store.rs
  revision: 5265aa2cc9f36b6b98ded50c6823039d8999b665
  working_tree: modified
  sha256: b3a26a2374d42c31de679daaa3790ba5d3eb22fed5b0e378dc79e7d0ba61203e
- id: issues-1050-model
  resource: ../../../crates/hoimin-cli/src/review/model.rs
  revision: 5265aa2cc9f36b6b98ded50c6823039d8999b665
  working_tree: modified
  sha256: abdbe23ec9834dc3756803e3261385fe966c38598605f1b5639e6bd456e82bd0
- id: issues-1050-store-tests
  resource: ../../../crates/hoimin-cli/src/review/store_scaling_tests.rs
  revision: 5265aa2cc9f36b6b98ded50c6823039d8999b665
  working_tree: untracked
  sha256: 2252d9252299a6c66afae208f604bf30f250827e68382a12bd25d70f4efd2160
- id: issue-902-model
  resource: ../../../crates/hoimin-cli/src/review/model.rs
  revision: 51c72b5ec1255959e2d5e072cd9e85d8497ed1c8
  working_tree: clean
  sha256: 700c562060f5ea3a8ae59f316ad90433db6b291cea342c01fa8e4d9b46dc6477
- id: issue-902-input
  resource: ../../../crates/hoimin-cli/src/review/input.rs
  revision: 51c72b5ec1255959e2d5e072cd9e85d8497ed1c8
  working_tree: clean
  sha256: 9f339e0702be57c75776a3e89f41bf8e3e87311444aadaf47e62a334a217437b
- id: issue-902-store
  resource: ../../../crates/hoimin-cli/src/review/store.rs
  revision: 51c72b5ec1255959e2d5e072cd9e85d8497ed1c8
  working_tree: clean
  sha256: bf344f8da739070ce894d6d5adec8f5690499dd96ad7831c4fa78e5c72b5e55c
- id: issue-902-record
  resource: ../../superpowers/plans/2026-10-15-issue-902.md
  revision: 51c72b5ec1255959e2d5e072cd9e85d8497ed1c8
  working_tree: clean
  sha256: 79698341b1048dbef9634d217f925e2385903c8a89eeae64e15335a650e3996a
- id: issue-875-indexes
  resource: ../../../crates/hoimin-cli/src/review/mod.rs
  revision: 5399bc05ade9d4a1651bdbe6d61bfc2dd5cdc8a4
  working_tree: modified
  sha256: 2d825fcc8e68bca8cabb64e9088c19e1eb72ebf74e1f2039c781805f56c3c113
- id: issue-875-input
  resource: ../../../crates/hoimin-cli/src/review/input.rs
  revision: 5399bc05ade9d4a1651bdbe6d61bfc2dd5cdc8a4
  working_tree: modified
  sha256: f3bd085b553a69c3eb35203d0c83a7463eeddbb298d1099aeedf59a5418dd23d
- id: issue-875-record
  resource: ../../superpowers/plans/2026-10-14-issue-875.md
  revision: 5399bc05ade9d4a1651bdbe6d61bfc2dd5cdc8a4
  working_tree: modified
  sha256: c4d57f4fb9963974b673bacbe70960eedac3ff6e275b0e8440ad5baa0d58ae3e
- id: issue-711-survivor-review-design
  resource: ../../superpowers/specs/2026-10-11-issue-711-survivor-review-design.md
  revision: fdf80af
  working_tree: clean
  sha256: de751d275bf32380e52515eee21a1ae364c35c39f0ee984828a7b87d3f67886d
- id: issue-711-review-report
  resource: ../../superpowers/reports/2026-10-11-issue-711-survivor-review.md
  working_tree: untracked
  sha256: 2f388bf417df4054d41765ec4b66d74815880752cfb377956949bc628871c699
- id: issue-874-source-observations
  resource: ../../superpowers/plans/2026-10-14-issue-874.md
  working_tree: untracked
  sha256: 9d6b521515606b342e212481e191bb71ad6d8d9af0446326ab028bd54566d970
- id: issue-975-opener
  resource: ../../../crates/hoimin-cli/src/artifacts/mod.rs
  revision: 4ebed4457519920e9a28f8f653d8ce96985acdad
  working_tree: clean
  sha256: 035e4667bb363fb27ca359befe48cf8ad0541beb5477807542f6588a44de7935

- id: issue-975-evidence
  resource: ../../maintenance/2026-10-18-issues-971-979/task-2-report.md
  working_tree: untracked
  sha256: 8369e0ecec4d7964e47856822f5b354d39c94a21ea947f629ee241b4ef1994e1

- id: issue-976-reader
  resource: ../../../crates/hoimin-cli/src/plan.rs
  revision: 4ebed4457519920e9a28f8f653d8ce96985acdad
  working_tree: clean
  sha256: a4b058fa7710143ed50bcc30ea6b10f28c311d863a8826cdea3b2c7f42fff36d

- id: issue-976-evidence
  resource: ../../maintenance/2026-10-18-issues-971-979/task-3-report.md
  working_tree: untracked
  sha256: 307a571dbcb32919333ea375f082598bfe16708d19465b47b0daafeaed294748

---

# survivorの評価と実行結果の分離

## 対象と契約

`review list/show/record` は保存planの候補に人の評価と理由を紐づけ、独立SQLite sidecarへ履歴を保存する。
project、候補identity、originalを含む完全descriptor、ソースの鮮度が一致する場合だけ評価を適用する。
評価はactionable、not_useful、suspected_equivalent、needs_investigationで、未評価をnot_usefulと混同しない。[^issue-711-survivor-review-design]

## 更新と表示

更新にはexpected revisionを必要とし、履歴追加とrevision更新を同一transactionにする。
source変更時はstaleとして表示し、新しい候補へ評価を移さない。
plan単独では実行結果を推測せず、明示的に付けた検証済みreportからだけstatusを表示する。[^issue-711-survivor-review-design]

## 判断と限界

既存rusqliteを使う独立DBにすることで、実行sessionやスコア計算への依存を避ける。
評価はランキング、kill率、progress判断を変更しない。
Leanでは誤帰属の防止条件、更新拒否時の状態不変、更新成功時の履歴保存を証明した。
168ケースのLean生成corpusでRust純粋処理と比較した。
Rust/SQLite/OSまで形式検証した保証ではなく、ローカルWindows Lean 4.34.1とCI pinned 4.32.2の確認を分ける。[^issue-711-review-report]

## Candidate ID joins

The review command builds borrowed HashMap indexes once for checked plan
candidates, loaded report mutants, and the loaded review snapshot. The same plan
index serves report validation and stale membership; the same report index
serves current and stale execution lookup. The original selected-plan and snapshot
vectors determine display order. Indexes store references to existing IDs and
records, without cloning candidate data or history.[^issue-875-indexes]

Checked plan and report readers reject duplicate IDs before indexing. Store
snapshots validate their key/ID pairs and use unique SQLite primary keys. These
invariants permit direct iterator collection; invalid internal duplicate vectors
have no separate compatibility policy. A stable ID lookup still requires the
existing full-descriptor comparisons. Report decoding, project comparison,
identity validation, descriptor validation, source freshness, and record append
retain their existing error order.[^issue-875-input][^issue-875-record]

Index construction uses expected O(P + R + S) hash-map work and O(P + R + S)
additional key/value references and bucket storage for P plan entries, R report
entries and S snapshot entries. Lookup counts describe logical ID joins, not
bucket probes, elapsed time, total command memory, or source-validation work.
Source validation and checked-reader costs remain outside this ID-join claim.
The implementation record distinguishes real-command work-count tests and review
regressions from the existing Lean applicability/update model, which does not
model hash-map complexity or Rust error ordering.[^issue-875-record]

## 再確認の条件

実装、保存schema、project同定、report読取り、ランキングへの接続を変更する際に設計との対応を再確認する。
実装計画・実装・テストの各段階でも最低5回のセルフレビューを別途行う。

[^issue-711-survivor-review-design]: [#711 survivorの評価と履歴の保存設計](../../superpowers/specs/2026-10-11-issue-711-survivor-review-design.md)。2026-10-11の実装前設計。

[^issue-711-review-report]: [#711 survivor評価の実装と検証](../../superpowers/reports/2026-10-11-issue-711-survivor-review.md)。実装、試験、証明の条件と限界。

[^issue-875-indexes]: [Review command indexes](../../../crates/hoimin-cli/src/review/mod.rs).
[^issue-875-record]: [Issue #875 design, implementation and validation](../../superpowers/plans/2026-10-14-issue-875.md).

[^issue-875-input]: [Checked plan and report matching](../../../crates/hoimin-cli/src/review/input.rs).

## Source observations during a command

Review display validates selected candidates in file groups. Each path has one
bounded source read and one validation context per command; every candidate
still receives its own hash, descriptor, encoding and identity checks.
Results and errors are consumed in the original candidate order, including
interleaved paths. Source bytes and context are released before reading the next
file. This bounds retained source storage to one file rather than all sources.
[^issue-874-source-observations]

Candidates for a path share its single observation, so an edit during display
does not cause later candidates for that path to observe another version.
Different files do not form an atomic snapshot. A subsequent command reads
again, and record retains an independent freshness check before writing.
No source cache crosses commands; this source-observation optimization does not
change CLI arguments or saved schemas.
[^issue-874-source-observations]

The issue 874 checks cover source work counts, interleaved result/error order,
stale and unavailable states, validation precedence, and the existing review
integration and Lean corpus adapter. The adapter checks the unchanged pure
model, not source I/O or SQLite/OS behavior. Executed results and unverified
platform cases are recorded separately in the implementation plan.
[^issue-874-source-observations]

[^issue-874-source-observations]: [Issue 874 implementation and validation](../../superpowers/plans/2026-10-14-issue-874.md).

## Artifact reference responsibilities

For review maintainers, `ArtifactReference` identifies a plan or report by path
and content hash. `ReportReference` composes that reference under `artifact`
with a required run ID. `ReviewDraft.plan` cannot carry a run ID, and a supplied
`ReviewDraft.report` cannot omit its run ID. The report itself remains optional.
Validation checks nonempty paths, lowercase 64-digit hashes, and nonblank run
IDs of at most 4096 UTF-8 bytes without control characters. Reason and evidence
retain their separate bounded text rules.[^issue-902-model][^issue-902-input]

Review JSON and SQLite store versions are 2. Older stores are rejected without
migration; old plan `run_id` fields and missing or null report IDs fail strict
JSON decoding. Human rendering and CLI arguments retain their existing behavior.[^issue-902-store]
See the [Issue 902 implementation record](../../superpowers/plans/2026-10-15-issue-902.md)
for design decisions, review passes and executed checks.[^issue-902-record]

[^issue-902-model]: [Artifact and report reference types](../../../crates/hoimin-cli/src/review/model.rs).

[^issue-902-input]: [Bounded review reference validation](../../../crates/hoimin-cli/src/review/input.rs).

[^issue-902-store]: [Versioned review store validation](../../../crates/hoimin-cli/src/review/store.rs).

[^issue-902-record]: [Issue 902 design and verification record](../../superpowers/plans/2026-10-15-issue-902.md).

## Regular inputs and source membership (Issues #975 and #976)

Artifact reads and existing-store protected-input comparison share a descriptor
opener. Unix `O_NONBLOCK` prevents waiting for a FIFO writer; descriptor metadata
then rejects nonregular inputs. The 64 MiB initial/read-growth limits remain.
Review treats nonregular candidate sources as unavailable. Input symlinks to
regular files remain valid, while the store keeps its separate symlink rejection.
Callers retain missing-input and alias diagnostics. This establishes no general
filesystem I/O deadline.[^issue-975-opener][^issue-975-evidence]

Read-only plan validation builds one borrowed `BTreeMap` of source path to hash
after header validation rejects duplicate paths. Candidate ID validation precedes
each source lookup. Candidate output order follows the original vector; source
permutations do not change acceptance. Construction costs O(S log S), followed
by O(C log S) lookups, without cloning source or candidate payload. The regression
records one build, 257 source visits and 256 queries instead of 65,792 visits.
The archived aligned default-feature CLI timings are observations, not test
thresholds or a general performance guarantee.[^issue-976-reader][^issue-976-evidence]

[^issue-975-opener]: [Regular-file opening and bounded reads](../../../crates/hoimin-cli/src/artifacts/mod.rs).
[^issue-975-evidence]: [FIFO child deadlines/reaping and preserved policies](../../maintenance/2026-10-18-issues-971-979/task-2-report.md).
[^issue-976-reader]: [Read-only plan reader](../../../crates/hoimin-cli/src/plan.rs).
[^issue-976-evidence]: [Source-work counters and aligned CLI comparison](../../maintenance/2026-10-18-issues-971-979/task-3-report.md).

## Capability-relative sources and selected history retention (Issues #1050 and #1053)

Review source observations now share `PortableFileReader` with plan/verify.
The reader pins a root handle and uses bounded no-follow regular-file reads.
Internal and external symlink/reparse-point components are unavailable, as are
missing, unreadable, oversized and nonregular sources; lexical invalid paths
remain input errors. This supersedes review's older canonicalize/containment
check followed by a path-based reopen. Artifact symlinks have their separate,
unchanged regular-file input policy.[^issues-1050-input][^issues-1050-reader]

Opened-handle metadata rejects an initially oversized source before allocation;
limit+1 reading still detects growth. One read and validation context serves all
candidates for a path in original output order. Record takes a new freshness
observation. Deterministic final-entry/parent swaps and internal links are
covered alongside limits, FIFO and grouped observations. These tests establish
those boundaries, not a deadline for every filesystem operation.[^issues-1050-boundary-tests]

Append and selected show validate the full store in one ordered joined scan
and an event count, retaining only requested history. Unrelated malformed keys
or drafts, empty/noncontiguous histories and orphan events still reject the
operation. Full snapshot uses the same scan without a selection. Owned append
shares rejection rules with the immutable model operation but moves its already
loaded history, avoiding a second copy. Revision conflicts, rollback, event/DB
limits and schema 2 remain.[^issues-1050-store][^issues-1050-model]

The bounded regression measures data SELECTs, decoded keys/drafts and additional
live Rust heap. It excludes schema/PRAGMA queries, SQLite's C heap and process
RSS. All stored history is still validated; the change does not make append
constant-time. The [implementation record](../../maintenance/2026-10-21-issues-1050-1051-1052-1053/README.md)
separates before/after observations and platform limits. Recheck path policy,
corruption detection and retention when changing these readers.[^issues-1050-store-tests]

[^issues-1050-input]: [Review source observations](../../../crates/hoimin-cli/src/review/input.rs).
[^issues-1050-reader]: [Shared bounded root-relative reader](../../../crates/hoimin-cli/src/workspace/reader.rs).
[^issues-1050-boundary-tests]: [Source boundary regressions](../../../crates/hoimin-cli/src/review/input_boundary_tests.rs).
[^issues-1050-store]: [Global validation and selected retention](../../../crates/hoimin-cli/src/review/store.rs).
[^issues-1050-model]: [Shared revision rules and owned append](../../../crates/hoimin-cli/src/review/model.rs).
[^issues-1050-store-tests]: [Query, heap and corruption regressions](../../../crates/hoimin-cli/src/review/store_scaling_tests.rs).

# Candidate ownership during rendering (Issue #1087)

Private ReviewRow uses Cow for its candidate. Current rows borrow descriptors
from the loaded plan, which remains alive through synchronous rendering. Stale
rows own descriptors reconstructed from stored keys, including sequence zero,
and can outlive the temporary reconstruction. Both continue borrowing history
from the snapshot. This changes internal ownership without changing serialized
candidate fields, human output, stale matching or persisted keys.
[^issue-1087-rows][^issue-1087-render]

The [implementation evidence](../../maintenance/2026-10-22-issues-1085-1086-1087/README.md)
measures live Rust allocations at the public JSON command's first actual write,
when a decoder peak would no longer obscure a second retained candidate copy.
SQLite's C heap, RSS and end-to-end latency are outside that measurement.

[^issue-1087-rows]: [Borrow current candidates and own stale descriptors](../../../crates/hoimin-cli/src/review/mod.rs).
[^issue-1087-render]: [Candidate and history lifetimes in ReviewRow](../../../crates/hoimin-cli/src/review/render.rs).

# Borrowed descriptor equality and source validation (Issue #1089)

CandidateDescriptorRef compares the complete mutation metadata while borrowing
strings from a current candidate or stored descriptor. Review's report/execution
matching uses this view. It excludes assigned ID and sequence, and retains every
other descriptor field and Utf8Path equality. Identity-only comparison would
omit original/location/symbol; full MutationCandidate equality would include
bookkeeping. Neither is an equivalent replacement.[^issue-1089-core][^issue-1089-review]

Source-state validation passes the current candidate to the same core validator
and compares the stable ID it returns, eliminating the second identity hash.
Stale source, original/encoding/location errors and identity conflict keep their
existing priority. Owned persisted keys, stale candidates and history retain
their current lifetimes.[^issue-1089-input]

The [implementation evidence](../../maintenance/2026-10-22-issue-1089/README.md)
records allocation windows around actual execution comparison and warm source
validation. It does not claim zero allocations for the entire review command.

[^issue-1089-core]: [crates/hoimin-core/src/candidate.rs](../../../crates/hoimin-core/src/candidate.rs).

[^issue-1089-review]: [crates/hoimin-cli/src/review/mod.rs](../../../crates/hoimin-cli/src/review/mod.rs).

[^issue-1089-input]: [crates/hoimin-cli/src/review/input.rs](../../../crates/hoimin-cli/src/review/input.rs).
