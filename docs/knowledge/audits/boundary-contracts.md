---
type: Audit
title: 境界をまたぐ契約 fixture と実行証拠
description: 六つの境界の前提、実行モード、Lean生成入力と未実行条件を結ぶ。
status: draft
sources:
- id: issue-490-design
  resource: ../../superpowers/specs/2026-09-14-issue-490-boundary-contracts-design.md
  revision: 7d5bdc5a643add4ba55e8541fa59a069d0ff5c5e
  working_tree: untracked
  sha256: 8e90be8bf11f38328ea1fd92fd82af2b8b4972d73437a88e94c547a336807bad
- id: issue-490-review
  resource: ../../superpowers/reports/2026-09-14-issue-490-boundary-contracts-review.md
  revision: c7ce87f8d4c584c2d8df2dcb9d65348c3dd3a706
  working_tree: modified
  sha256: 03cc7e076df1207ade765d1a2ee0160e1efeb3c45678e41d56ef5f5b4067a344
- id: issue-490-evidence
  resource: ../../superpowers/reports/2026-09-14-issue-490-boundary-contracts-verification.json
  revision: 7d5bdc5a643add4ba55e8541fa59a069d0ff5c5e
  working_tree: untracked
  sha256: 409da3fb56c967d77dc7ceaa37ddc68743a0f5a03d5c21d47ddd1fd40074e21a
- id: issue-490-registry
  resource: ../../../tests/fixtures/boundary-contracts.json
  revision: 7d5bdc5a643add4ba55e8541fa59a069d0ff5c5e
  working_tree: untracked
  sha256: 3178ba2f0dff85e56c6ec9b40d9ec68dbef014ee00d620fc939d02ac8460f298
- id: issue-490-runner
  resource: ../../../tools/boundary_contracts.py
  revision: 7d5bdc5a643add4ba55e8541fa59a069d0ff5c5e
  working_tree: untracked
  sha256: 44c57d037d64e80ebf5b22ee9f253f18661ad03c0528bfe836a0c2166e2e9bd6
---

# 境界をまたぐ契約 fixture と実行証拠

この registry は対象選択、backend、SQLite/resume、reader/progress、出力先、
plan/verify の六つの境界を、前提・公開観測・証拠レベル・実行モードで結ぶ。
設計の worksheet が各条件を定義する。[^issue-490-design]

strict モードは登録された exact test を実行する。テストの存在やゼロ件成功を
一致とは扱わず、match、mismatch、infrastructure-error、unexecuted を区別する。
report モードは失敗と未実行行も保存する。[^issue-490-registry][^issue-490-runner]

今回の macOS 実行では strict 10 件が一致した。既存 Lean 入力 282 件を JSON v2、
JSON v3、JSONL v3 で実 CLI に適用した 846 観測と、実 SQLite に対する既存 session
corpus 20 件を含む。モデルの期待値を production helper から生成していない。
これは有限入力の対応検査であり、任意の実行列や OS の原子性の証明ではない。[^issue-490-evidence]

準備段階のエラー優先順位は観測したが、manifest/fingerprint/copy のキャンセル応答や
verify 全体の期限は検証していない。Linux hard 制御と Windows PID/fault も未実行行に
理由を残す。Lean cache の初回失敗と再構築後の成功を分けて記録した。[^issue-490-review]

# 関連

- [progress 入力契約](progress-input.md)
- [Lean 証拠の区別](lean-evidence.md)
- [session とレポート](../design/session-report.md)
- [カタログ入口](../index.md)

[^issue-490-design]: [原文](../../superpowers/specs/2026-09-14-issue-490-boundary-contracts-design.md)。参照版と SHA-256 は frontmatter に記録。
[^issue-490-review]: [原文](../../superpowers/reports/2026-09-14-issue-490-boundary-contracts-review.md)。参照版と SHA-256 は frontmatter に記録。
[^issue-490-evidence]: [原文](../../superpowers/reports/2026-09-14-issue-490-boundary-contracts-verification.json)。参照版と SHA-256 は frontmatter に記録。
[^issue-490-registry]: [原文](../../../tests/fixtures/boundary-contracts.json)。参照版と SHA-256 は frontmatter に記録。
[^issue-490-runner]: [原文](../../../tools/boundary_contracts.py)。参照版と SHA-256 は frontmatter に記録。
