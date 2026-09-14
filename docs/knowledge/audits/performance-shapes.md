---
type: Playbook
title: 入力形状別の性能検証
description: 決定的な回帰ゲートとrelease計測、未統合の依存、証拠の読み方を区別する。
status: draft
catalog_revision: 165a2d284a1af92eb02ffd214ba8c0070c2f3808
sources:
- id: integration
  resource: ../../performance/2026-09-14-integration-check.md
  working_tree: untracked
  sha256: e11e40961e7b00a5e7781f87b0bec36123798830da6d9ebc3e2731a64cd62fce
- id: guide
  resource: ../../performance/README.md
  working_tree: modified
  revision: da6b9cf5fd6eff71438c503b0b99a15571b5e696
  sha256: 94482fc2358513b5723d839d68345996629a68808931f319e5adef5aecf0ffaf
- id: registry
  resource: ../../performance/shapes.json
  working_tree: modified
  revision: da6b9cf5fd6eff71438c503b0b99a15571b5e696
  sha256: 6a2a9681cd6df920d65e5f08f16967f0691947e6b9643e42500ddc87cb3ce899
- id: design
  resource: ../../superpowers/specs/2026-09-14-issue-491-performance-shapes-design.md
  working_tree: modified
  revision: da6b9cf5fd6eff71438c503b0b99a15571b5e696
  sha256: 89fb01871fdbcd7cfdb3a4f1fca9c3d30b2251e9656c7fe4a90c93a141eac1bf
- id: review
  resource: ../../superpowers/reports/2026-09-14-issue-491-performance-shapes-review.md
  working_tree: modified
  revision: da6b9cf5fd6eff71438c503b0b99a15571b5e696
  sha256: 903abf4a3d7010cd791a463fb8d1393583440e1150464ec4ec07bc28e85ad1f9
---

# 入力を増やす軸と測定指標

性能検証の台帳は、対象発見、fingerprint、ソース配置、AST、解析状態、verify、出力、workspaceの8次元を11形状へ対応付ける。同じ形状をN/2N/4Nで作るが、入力構造の説明と修正後の期待値は別欄である。[^registry][^design]

# 通常ゲートとrelease計測

通常ゲートはactiveなRustテストを実行し、0件一致やignoredだけの成功を認めない。pendingな依存テストは成功数へ含めない。release計測は新規の出力先へbinary digest、入力、stdout/stderr、時間、RSSと比較結果を保存する。時間には監視の固定費が含まれ、未観測のRSSはnullである。retained heap、allocator peak、sampled RSSは別の指標として読む。[^guide][^registry]

# 観測と残る作業

今回のmacOS実行では11形状の198回比較と最終top1追試18回が意味検証を通過した。既存Leanのbuild・感度・freshnessとRust adapterも成功したが、新しいコストモデルを追加したわけではない。全8次元の漸近的回帰がCIで阻止されること、全体定数メモリ、別OSでの性能は結論しない。[^review]

依存修正を取り込むときは、テスト名、実際の計数点、モデルの仮定を照合し、ゲートを実行してからpendingをactiveへ変更する。入力上限、測定指標、CLI出力契約、監視方式を変更した場合も台帳と本ページを再確認する。[^guide]

[^guide]: [README.md](../../performance/README.md)。
[^registry]: [shapes.json](../../performance/shapes.json)。
[^design]: [2026-09-14-issue-491-performance-shapes-design.md](../../superpowers/specs/2026-09-14-issue-491-performance-shapes-design.md)。
[^review]: [2026-09-14-issue-491-performance-shapes-review.md](../../superpowers/reports/2026-09-14-issue-491-performance-shapes-review.md)。

# 個別修正の統合確認

全10件のRust差分をローカルで組み合わせ、追加テスト46件の欠落がないことと、全workspace試験1,814件の成功を確認した。これは初回PR公開時の統合確認であり、この結果だけではpendingをactiveに変更しなかった。順次マージ時には依存実装を取り込み、21ゲートすべての実行成功によりactiveへ昇格した。[^review][^integration]

[^integration]: [2026-09-14-integration-check.md](../../performance/2026-09-14-integration-check.md)。
