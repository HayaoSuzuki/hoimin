---
type: Contract
title: CIの選択と必須チェック
description: 保守的な差分分類、結果集約、workflow検査の運用条件。
status: draft
catalog_revision: 6ec69f4ac926c887003e5509bb393289f0bc9e71
sources:
- id: guide
  resource: ../ci.md
  revision: a9f9d424cd7fc0c21ece1c89c233448e531a5b64
  working_tree: modified
  sha256: e39e1ca23612fc82c4e8a7f1aef276b28a91553a9df9339645400b4fd6983c01
- id: planner
  resource: ../../tools/ci_selection.py
  revision: 6ec69f4ac926c887003e5509bb393289f0bc9e71
  working_tree: untracked
  sha256: 581a5a991fd41b6ca76849f31cc2b4e05564134b6792117a0f9d7df2799664ec
- id: ci
  resource: ../../.github/workflows/ci.yml
  revision: 6ec69f4ac926c887003e5509bb393289f0bc9e71
  working_tree: modified
  sha256: 0ffc5516e77e03feb8c4684f8e66700ac020f9f99b4c0c8271bbdcb8d8433335
- id: release
  resource: ../../.github/workflows/release.yml
  revision: 6ec69f4ac926c887003e5509bb393289f0bc9e71
  working_tree: modified
  sha256: 35efb0eb0e4f31293bdd38edf9c9163795ba8d1e4d9f7635426bf047a194700e
---

# 選択と集約の契約

差分が不明な場合は全検証を選ぶ。
文書変更でも変更判定と品質・workflow検査、集約を起動する。
集約は必要なjobのsuccessを要求し、failure・cancelled・skipped・結果欠落を拒否する。
条件付きjobの意図したskipだけを許容する。[^planner][^ci]

# 公開と検査の運用

PRの古い検証とpreviewだけをキャンセルする。
previewは配布入力の変更時と手動実行時に選択し、マージ後のタグ予約と公開は独立した書込jobで実行する。
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
