---
type: Contract
title: CIの選択と必須チェック
description: 保守的な差分分類、結果集約、workflow検査の運用条件。
status: draft
catalog_revision: 6ec69f4ac926c887003e5509bb393289f0bc9e71
sources:
- id: guide
  resource: ../ci.md
  revision: 1e60e54813dad14f124635496ed773b299bb4b90
  working_tree: modified
  sha256: a3bff387f9c0b52bee9109ea4c925d48dd06646ed0a3bf1fbe2929958b50723b
- id: planner
  resource: ../../tools/ci_selection.py
  revision: 1e60e54813dad14f124635496ed773b299bb4b90
  working_tree: modified
  sha256: 44f27dd6683f6b362f3124fede5857675836ad93f13fff7ade1cdf793d205d81
- id: ci
  resource: ../../.github/workflows/ci.yml
  revision: 1e60e54813dad14f124635496ed773b299bb4b90
  working_tree: modified
  sha256: 18675c9744ae8606570e0b42c9d54b4dc19fd39c00849f659585157a843c3f08
- id: release
  resource: ../../.github/workflows/release.yml
  revision: a6965910bbc745e74aeb539628b3e2dc66c2a9d3
  working_tree: modified
  sha256: aed6b62488b3c90397739c36e793b1e143356163a9f6b50a2071c9a45a5c605b
---

# 選択と集約の契約

差分が不明な場合は全検証を選ぶ。
文書変更でも変更判定と品質・workflow検査、集約を起動する。
集約は必要なjobのsuccessを要求し、failure・cancelled・skipped・結果欠落を拒否する。
条件付きjobの意図したskipだけを許容する。[^planner][^ci]

生成済みの`docs/cli-reference.md`は文書のみの変更でもRust検証を選ぶ。
Rust jobは生成コマンドの`--check`で同期を検査し、CLI定義と生成文書の差分を拒否する。
生成対象と再生成の手順は[CLIリファレンスの生成と同期](cli-reference.md)を参照する。[^planner][^ci]

# 公開と検査の運用

PRの古い検証とpreviewだけをキャンセルする。
previewは配布入力の変更時と手動実行時に選択し、マージ後のタグ予約と公開は独立した書込jobで実行する。
由来証明の書込jobもマージ後に限定し、公開は証明と照合の成功を要求する。
対象と実機検証の範囲は[リリースの由来証明](release-provenance.md)を参照する。
書込jobでPR headを実行しない。[^release]

全workflowを固定版のactionlint・ShellCheck・zizmorで検査する。
重大度の足切りやShellCheckの一括除外をせず、必要な例外だけを理由付きで該当箇所に記載する。
ローカルの再現コマンド、各例外、測定方法とツールの限界は運用ガイドを正本とする。[^guide]

# 未確認事項と再確認条件

hostedで全検証CIと3platform previewの成功・時間、PR再pushによる旧CIのキャンセルを確認した。
実測の対象はworkflow・配布入力を含むPRに限り、文書のみの削減率やmerge queue実行は未確認である。[^guide]
現移管先の必須チェックは未設定であり、有効化する場合は`CI result`を指定する。
旧組織の設定は対象外とする。
path分類、job名、公開イベント、Runner、lint版を変えた場合はこの契約と負例テストを再確認する。[^guide]

[^guide]: [CIの運用ガイド](../ci.md)。
[^planner]: [変更判定と集約](../../tools/ci_selection.py)。
[^ci]: [CI workflow](../../.github/workflows/ci.yml)。
[^release]: [release workflow](../../.github/workflows/release.yml)。
