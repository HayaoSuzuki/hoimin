---
type: Playbook
title: 入力形状別の性能検証
description: 決定的な回帰ゲートとrelease計測、未統合の依存、証拠の読み方を区別する。
status: draft
catalog_revision: 165a2d284a1af92eb02ffd214ba8c0070c2f3808
sources:
- id: guide
  resource: ../../performance/README.md
  working_tree: untracked
  sha256: f87afed9e510f92dfdab421fa58a98d2203fa0773dd9d2a7594d2f0f919454db
- id: registry
  resource: ../../performance/shapes.json
  working_tree: untracked
  sha256: 42baf57b596ce5435bd2bc5dbd25607214b3fcd17cdc72ea747913dbf9c20da9
- id: design
  resource: ../../superpowers/specs/2026-09-14-issue-491-performance-shapes-design.md
  working_tree: untracked
  sha256: a4f5eff4cb019d8b1b70c78f528c32b05e839c9c1eea879319b8588f4b36b895
- id: review
  resource: ../../superpowers/reports/2026-09-14-issue-491-performance-shapes-review.md
  working_tree: untracked
  sha256: f3b51dcbefefd654871b1dc353804df820588be307e92aa934a737a7f58352dd
---

# 入力を増やす軸と測定指標

性能検証の台帳は、対象発見、fingerprint、ソース配置、AST、解析状態、verify、出力、workspaceの8次元を11形状へ対応付ける。同じ形状をN/2N/4Nで作るが、入力構造の説明と修正後の期待値は別欄である。[^registry][^design]

# 通常ゲートとrelease計測

通常ゲートはactiveなRustテストを実行し、0件一致やignoredだけの成功を認めない。pendingな依存テストは成功数へ含めない。release計測は新規の出力先へbinary digest、入力、stdout/stderr、時間、RSSと比較結果を保存する。時間には監視の固定費が含まれ、未観測のRSSはnullである。retained heap、allocator peak、sampled RSSは別の指標として読む。[^guide][^registry]

# 観測と残る作業

今回のmacOS実行では11形状の198回比較と最終top1追試18回が意味検証を通過した。既存Leanのbuild・感度・freshnessとRust adapterも成功したが、新しいコストモデルを追加したわけではない。全8次元の漸近的回帰がCIで阻止されること、全体定数メモリ、別OSでの性能は結論しない。[^review]

依存PRの統合時は、テスト名、実際の計数点、モデルの仮定を照合し、ゲートを実行してからpendingをactiveへ変更する。入力上限、測定指標、CLI出力契約、監視方式を変更した場合も台帳と本ページを再確認する。[^guide]

[^guide]: [README.md](../../performance/README.md)。
[^registry]: [shapes.json](../../performance/shapes.json)。
[^design]: [2026-09-14-issue-491-performance-shapes-design.md](../../superpowers/specs/2026-09-14-issue-491-performance-shapes-design.md)。
[^review]: [2026-09-14-issue-491-performance-shapes-review.md](../../superpowers/reports/2026-09-14-issue-491-performance-shapes-review.md)。
