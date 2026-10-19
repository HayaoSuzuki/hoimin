---
type: Contract
title: JSONレポートのmutant record書込み
description: mutantイベントの一括書込み、ack、short write、poisoned状態の契約。
status: draft
source_revision: 165a2d284a1af92eb02ffd214ba8c0070c2f3808
sources:
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
    resource: ../../../crates/hoimin-cli/src/report/json.rs
    revision: bd8de5bdee2f0e4033623e5b44eff5f62db89b72
    working_tree: clean
    sha256: 3357da3117212874872b3a1c5245bd8b94dcfd58bee4322edab3758503db16f3
  - id: tests
    resource: ../../../crates/hoimin-cli/tests/report_handler.rs
    revision: bd8de5bdee2f0e4033623e5b44eff5f62db89b72
    working_tree: clean
    sha256: 98836b9bec5c5f828ba8763439e33670967c7b616d531f6f8bdbd8fe34adbd15
---

# mutant recordの書込み

JSON形式のレポートは、mutantイベント一件と必要な先行カンマを一つのrecordとして一時バッファへ直列化し、spoolへ一回の`write_all`で渡す。全イベントをメモリに保持せず、履歴は管理対象のdelivery spoolに保存する。[^design][^implementation]

成功応答は`write_all`の完了後に返す。短い正常書込みは残りを再試行し、途中のI/Oエラーまたは進捗のない書込みは元のeffectを失敗させる。失敗後のreportはpoisoned状態になり、同じhandlerで後続イベントを受理しない。[^design][^tests]

schema 9の`status`は、spoolへ書き込むイベントの直列化時にoutcomeから導出する。spoolから最終JSONへコピーする段階では分類し直さない。JSONLも同じ`MutantFinished`の直列化処理を使い、summaryとresumeはoutcomeを分類根拠にする。[^issue-1010-wire]

最終レポートでは従来どおりspoolをflushして先頭へseekし、stdoutへcopyする。このflushはRustのwriter契約であり、永続媒体への同期完了を意味しない。writer境界または最終出力手順を変更した場合は、ackとエラー時点を再確認する。

[^design]: [Issue 463 design](../../superpowers/specs/2026-09-14-issue-463-json-record-write-design.md)。
[^implementation]: [json.rs](../../../crates/hoimin-cli/src/report/json.rs)。
[^tests]: [report handler tests](../../../crates/hoimin-cli/tests/report_handler.rs)。

[^issue-1010-wire]: [導出statusのreport境界](../../../crates/hoimin-core/src/report.rs)。
