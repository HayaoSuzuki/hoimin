---
type: Playbook
title: CLIリファレンスの生成と同期
description: Rustのコマンド定義から生成する範囲、手書き文書との分担、同期検査の手順。
status: draft
catalog_revision: 1e60e54813dad14f124635496ed773b299bb4b90
sources:
- id: generator
  resource: ../../crates/hoimin-cli/examples/generate_cli_reference.rs
  revision: 1e60e54813dad14f124635496ed773b299bb4b90
  working_tree: untracked
  sha256: 943ce9254bdbbf69ccb9ee1289d64a216c71dbb5b25251e4da3d1206ca9732c8
- id: development
  resource: ../development.md
  revision: 1e60e54813dad14f124635496ed773b299bb4b90
  working_tree: modified
  sha256: 14cdd6e1b6a28407122e51cc76f6f35f3e220c8ac82d62a118f489f2f0f1b5ae
---

# 生成する範囲と手書き文書の分担

[CLIリファレンス](../cli-reference.md)は、解析と補完で使う`root_command()`から生成する。
公開コマンドを再帰的にたどり、長いヘルプ、既定値、列挙値、引数の個数、反復、必須指定、競合、必須グループを記録する。
隠された項目を掲載せず、端末幅と色を固定して日時や絶対パスを出力に含めない。[^generator]

条件付き必須関係と数値パーサーの範囲は、Clapの安定した公開APIでは一般的に取得できない。
既存のヘルプに書かれた条件は生成されるが、実行時の検査を網羅した仕様にはならない。
使用例、OS別の資源制限、plan/verifyの継承契約は[利用ガイド](../usage.md)で説明する。[^generator][^development]

# 変更後の再生成と検査

公開CLIの定義やヘルプを変えたら、次のコマンドで生成文書を更新し、実装と同じ作業ブランチにコミットする。[^development]

```console
cargo run --locked -p hoimin-cli --example generate_cli_reference
cargo run --locked -p hoimin-cli --example generate_cli_reference -- --check
cargo test --locked -p hoimin-cli --example generate_cli_reference
```

`--check`は文書を書き換えず、欠落や差分があれば失敗する。
WindowsのCRLF checkoutは許容し、生成時はUTF-8とLFを使う。
CIの選択条件と必須チェックは[CIの契約](ci-validation.md)を参照する。[^generator][^development]

# 再確認する変更

Clapの版、コマンド定義、生成処理、CIの差分分類を変更したときは、隠し項目の除外、決定性、生成文書の同期を再検査する。
この手順は文書サイトの構築やJSONスキーマ生成を対象にしない。[^development]

[^generator]: [生成処理とテスト](../../crates/hoimin-cli/examples/generate_cli_reference.rs)。
[^development]: [開発ガイド](../development.md#generated-cli-reference)。
