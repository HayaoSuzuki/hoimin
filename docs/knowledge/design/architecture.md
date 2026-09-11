---
type: Decision
title: 隔離コピーと状態遷移の設計
description: 元ソースへの変異適用を避け、状態遷移と入出力をcrateごとに分離する設計を説明する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: issue-465
  resource: ../../superpowers/specs/2026-09-11-issue-465-retired-discovery-design.md
  working_tree: untracked
  sha256: 7efd3035a37d38c95404f12774387450f94dc0be0a69af1da905056df54cedf6

- id: issue-462
  resource: ../../superpowers/specs/2026-09-11-issue-462-retired-reader-design.md
  working_tree: untracked
  sha256: d60d84425e841eeca29a40a71a9b73a18be18a43e0a306b149add1f7c029b2b0
- id: initial
  resource: ../../superpowers/specs/2026-07-18-python-mutation-tool-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: migration
  resource: ../../superpowers/specs/2026-07-19-remove-python-libcst-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: core
  resource: ../../../crates/hoimin-core/Cargo.toml
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: machine
  resource: ../../../crates/hoimin-core/src/machine.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: analyzer
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
---

# 作業コピーへの変異適用

hoiminの初期設計では、一時領域に作ったテスト実行用のコピー（worker）へソースの変異を適用する方式を採用した。元の作業ツリーを書き換えて後で戻す方式では、強制終了によって復元処理が動かず、変異したソースが残るおそれがある。コピーを使う方式は、この復元処理への依存を避けるための判断だった。[^initial]

別案として、実行時に選べる複数の変異を一つのソースへ埋め込む方式も検討した。こちらは例外発生時の呼出し履歴やモジュール実行の意味への影響が課題となり、初版の対象外とした。[^initial]

# 状態遷移と入出力の分離

制御を担当する `hoimin-core` は、状態と完了通知（event）を受け取り、次の状態と実行要求（effect）を返す。入出力を担当する `hoimin-cli` はその要求を実行し、完了通知をcoreへ返す。この分離によって、状態遷移の判断をファイル操作やプロセス起動から独立して扱う。[^initial]

現在の実装にも状態遷移関数 `transition` があり、coreの通常依存にはTokio・SQLite・OS API用のcrateを置いていない。これはカタログ作成時にコードと依存宣言を読んで確認した結果である。[^core][^machine]

| 構成要素 | 担当する処理 |
| --- | --- |
| core | 状態遷移、実行要求の識別、結果分類、予算と終了判断 |
| target / analyzer | ファイルシステム・Gitからの対象選択、Python構文からの候補生成 |
| workspace / process | コピー、指定バイト範囲の変更、テスト起動、復元、資源制御 |
| report / session | 結果出力、SQLite保存、互換性の判定と再利用 |

この表は初期設計に記された分担を示す。実行順序や例外処理の正しさは、各処理に対応する試験・監査で確認する必要がある。[^initial]

# Rust解析器への移行

2026-07-18の初期設計にはLibCSTヘルパーが登場する。その翌日の移行設計では、解析専用のPythonコードとLibCSTを撤去する方針を定めた。現在の解析器では、Rust内でRuffの構文解析器と抽象構文木（AST）を使う。[^migration][^analyzer]

したがって、初期設計にあるヘルパー起動や版取得は現行手順として使えない。利用者が指定するテストコマンドでPythonを起動することと、hoiminの解析器がPythonに依存することも区別する。[^migration]

# 廃止したRust関数探索

Issue #465 の関数取りこぼしは、Rustの関数を正規表現で列挙してcargo-mutantsの候補を絞るPython製開発ツールの問題だった。コミット `2f27e2a` で探索処理と候補フィルタを含む連携全体が削除されている。今回、その削除が作業対象に含まれること、現行の呼出し元が残っていないこと、開発スキルの契約テスト3件の成功を確認した。既存の機能削除による解消であり、新しいRust構文解析器を実装したものではない。Pythonソースを解析する現行hoiminのRust実装とは対象が異なる。[^issue-465]

# 廃止した開発ツールの不具合

Issue #462 のFIFO読取り停止は、Pythonで実装されていたRust mutation testing用の開発ツールに関するものだった。コミット `2f27e2a` で読み取り関数・探索処理・コマンド入口が削除されており、現在の開発手順もcargo-mutantsを実行しない方針である。今回、削除コミットが作業対象に含まれること、現行ツリーに呼出し元が残っていないこと、現行スキルの契約テスト3件の成功を確認した。これは既存の機能削除による解消であり、現行Rust実装全体のFIFO安全性を示す検証ではない。[^issue-462]

# コピー方式で防げる変更の範囲

コピー方式で避けるのは、元ソースへ変異を適用する操作である。任意の出力先や子プロセスによる変更まで防ぐ設計とは読み取れない。[境界監査](../audits/boundary-2026-09.md)では、metrics出力先と元ソースが重なる場合を別の問題として記録している。

crateの依存、実行要求と完了通知、コピーへの変異適用、解析器の起動方法を変更したら、このページを見直す。終了処理の条件は[状態・終了処理](runtime-lifecycle.md)、検証の根拠は[Leanの証拠範囲](../audits/lean-evidence.md)で確認できる。

[^initial]: [2026-07-18-python-mutation-tool-design.md](../../superpowers/specs/2026-07-18-python-mutation-tool-design.md)。
[^migration]: [2026-07-19-remove-python-libcst-design.md](../../superpowers/specs/2026-07-19-remove-python-libcst-design.md)。
[^core]: [Cargo.toml](../../../crates/hoimin-core/Cargo.toml)。
[^machine]: [machine.rs](../../../crates/hoimin-core/src/machine.rs)。
[^analyzer]: [rust.rs](../../../crates/hoimin-cli/src/analyzer/rust.rs)。

[^issue-465]: [2026-09-11-issue-465-retired-discovery-design.md](../../superpowers/specs/2026-09-11-issue-465-retired-discovery-design.md)。

[^issue-462]: [2026-09-11-issue-462-retired-reader-design.md](../../superpowers/specs/2026-09-11-issue-462-retired-reader-design.md)。
