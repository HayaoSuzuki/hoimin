---
type: Reference
title: JSONレポートのmutant record書込み
description: mutantイベントの固定容量buffer、ack、short write、poisoned状態の契約。
status: deprecated
source_revision: 165a2d284a1af92eb02ffd214ba8c0070c2f3808
sources:
  - id: issue-1012-wire
    resource: ../../../crates/hoimin-core/src/report.rs
    revision: 9d457e322ff1ed91e69e448c1cd40e4a8949baca
    working_tree: modified
    sha256: f3674b4cf88918e63ea4e5ce413f36719d4b0340b61a6ad5f63524674165c54a
  - id: issue-1012-handler-tests
    resource: ../../../crates/hoimin-cli/tests/report_handler.rs
    revision: 9d457e322ff1ed91e69e448c1cd40e4a8949baca
    working_tree: modified
    sha256: 346637e79b342f1da6f5c6b149ce7f2d638306780aee73ec8f060f53ce1e149a
  - id: issue-1010-wire
    resource: ../../../crates/hoimin-core/src/report.rs
    revision: 6ea967d6b1682596d6a2c0dda3d1c458f9ce5e6a
    working_tree: modified
    sha256: b000930d1bd41fe60dd9dc9e459c3947cb667711ceec450c0d8673c95904f993
  - id: design
    resource: ../../superpowers/specs/2026-09-14-issue-463-json-record-write-design.md
    revision: bd8de5bdee2f0e4033623e5b44eff5f62db89b72
    working_tree: clean
    sha256: 76d7312dba2c393de9f8c9b47c0f767df0c4100df53a47a07142ba5897eabd19
  - id: implementation
    resource: https://github.com/tokyogas-tech/hoimin/blob/bd8de5bdee2f0e4033623e5b44eff5f62db89b72/crates/hoimin-cli/src/report/json.rs
    revision: bd8de5bdee2f0e4033623e5b44eff5f62db89b72
    working_tree: clean
    sha256: 3357da3117212874872b3a1c5245bd8b94dcfd58bee4322edab3758503db16f3
  - id: tests
    resource: ../../../crates/hoimin-cli/tests/report_handler.rs
    revision: bd8de5bdee2f0e4033623e5b44eff5f62db89b72
    working_tree: clean
    sha256: 98836b9bec5c5f828ba8763439e33670967c7b616d531f6f8bdbd8fe34adbd15
  - id: issue-1092-implementation
    resource: https://github.com/tokyogas-tech/hoimin/blob/e8f0e7b7f86a849d1305ef1ccaddd80c1df80f0b/crates/hoimin-cli/src/report/json.rs
    revision: 21c5659d4a6a37633bd1a29f691debd0e84f4280
    working_tree: modified
    sha256: 2460bd92ad12bbf8a5781557c5a74fa422bd1084947baf81eef29f60a23b7fff
  - id: issue-1092-tests
    resource: ../../../crates/hoimin-cli/tests/report_handler.rs
    revision: 21c5659d4a6a37633bd1a29f691debd0e84f4280
    working_tree: modified
    sha256: 83f3e3db810040d973cbed6f8ac3d28c8a958b8ff8e78a41f64e519e87e0b663
  - id: issue-1092-heap
    resource: ../../../crates/hoimin-cli/tests/report_heap.rs
    revision: 21c5659d4a6a37633bd1a29f691debd0e84f4280
    working_tree: modified
    sha256: b027e917711cd58b66a1c554519dce5d09c4eca47a02af635afec5b758329ab1
---

# Historical aggregate JSON writer

The aggregate JSON execution writer and its delivery spool have been removed
from the current working tree. `run` and `verify` now emit JSONL events only;
progress and review accept that event stream. The sections below preserve the
contracts at their recorded revisions and do not describe the current writer.
See the [current execution-report contract](session-report.md#current-report-reader-and-diagnostic-boundaries).

# mutant recordの書込み

JSON形式のレポートは、mutantイベント一件と必要な先行カンマを、処理中だけ存在する8 KiBの
BufWriterで直列化する。#463の全量Vecは#1092で置き換えた。小さいrecordは一回、大きいrecordは
複数回のまとまったwriteでspoolへ渡す。履歴全体はdelivery spoolへ保存し、encoded record全体を
一時メモリに保持しない。[^issue-1092-implementation][^issue-1092-heap]

成功応答は明示flushの成功後に返す。短い正常書込みとInterruptedは残りを再試行する。
書込み・flush・直列化の失敗は元のeffectを失敗させ、into_partsで未送信bytesを破棄してDropによる
再送を防ぐ。成功前にhas_mutantsを変更せず、失敗後は既存のpoisoned状態で後続eventと最終化を拒否する。
I/O失敗はwrite mutant recordとして通知する。[^issue-1092-implementation][^issue-1092-tests]

#463が定めた小recordのwrite集約と即時ack/failureの意図は維持する。
通常サイズにも8 KiBを確保するため、小recordの一時heapは増える。変更前後の割当・write回数と
測定範囲は[実装記録](../../maintenance/2026-10-23-issue-1092/README.md)に示す。[^design]

schema 9で導入された`status`は、spoolへ書き込むイベントの直列化時にoutcomeから導出する。spoolから最終JSONへコピーする段階では分類し直さない。JSONLも同じ`MutantFinished`の直列化処理を使い、summaryとresumeはoutcomeを分類根拠にする。[^issue-1010-wire]

最終レポートでは従来どおりspoolをflushして先頭へseekし、stdoutへcopyする。このflushはRustのwriter契約であり、永続媒体への同期完了を意味しない。writer境界または最終出力手順を変更した場合は、ackとエラー時点を再確認する。

[^design]: [Issue 463 design](../../superpowers/specs/2026-09-14-issue-463-json-record-write-design.md)。
[^implementation]: [json.rs](https://github.com/tokyogas-tech/hoimin/blob/bd8de5bdee2f0e4033623e5b44eff5f62db89b72/crates/hoimin-cli/src/report/json.rs)。
[^tests]: [report handler tests](../../../crates/hoimin-cli/tests/report_handler.rs)。

[^issue-1010-wire]: [導出statusのreport境界](../../../crates/hoimin-core/src/report.rs)。

# Final timeout metadata (Issue #1012)

Current report/event schema 10 adds the required nullable
`resolved_mutant_timeout` to the final summary. It preserves the execution
machine's requested cap as canonical `secs` and `nanos`; no request produces
explicit null. JSON and JSONL use the same summary value. This addition changes
neither mutant record buffering and acknowledgement nor outcome classification.
Handler regressions check exact subsecond output and explicit null at both
output boundaries. Recheck the summary serializer when changing report
assembly.[^issue-1012-wire][^issue-1012-handler-tests]

[^issue-1012-wire]: [Current summary wire type](../../../crates/hoimin-core/src/report.rs).
[^issue-1012-handler-tests]: [Summary output tests](../../../crates/hoimin-cli/tests/report_handler.rs).

[^issue-1092-implementation]: [固定容量のrecord書込み](https://github.com/tokyogas-tech/hoimin/blob/e8f0e7b7f86a849d1305ef1ccaddd80c1df80f0b/crates/hoimin-cli/src/report/json.rs)。
[^issue-1092-tests]: [書込み失敗と最終bytesの回帰テスト](../../../crates/hoimin-cli/tests/report_handler.rs)。
[^issue-1092-heap]: [実handlerの割当とwrite回数](../../../crates/hoimin-cli/tests/report_heap.rs)。
