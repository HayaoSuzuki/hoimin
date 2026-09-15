---
type: Audit
title: withの例外抑制とfinallyの解析コスト
description: 5e631efで確認した抑制後のimport誤認とfinally二重走査、Lean証明と公開CLIの対応範囲。
status: draft
catalog_revision: 5e631ef
sources:
  - id: audit
    resource: ../../audits/2026-09-15-with-finally/README.md
    working_tree: untracked
  - id: model
    resource: ../../audits/2026-09-15-with-finally/WithModel.lean
    working_tree: untracked
  - id: implementation
    resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
    revision: 5e631ef
---

# 対象と発見

`5e631ef`を対象に、型注釈のimport状態とfinallyの解析を確認した。with本体の途中で例外が起き、context managerが抑制して後続へ進む場合の状態を合流していない。このため、typing由来でない実行経路があるのに `Sequence[int] → list[int]` の候補を生成する。[#556](https://github.com/tokyogas-tech/hoimin/issues/556)に再現と受け入れ条件を記載した。[^audit]

finallyの解析には、記録無効でもfinalbodyの記録用走査を行い、その後転送用に再走査する経路が残る。入れ子深さ16〜20のrelease測定では、1段追加するたびに時間がほぼ倍増した。[#557](https://github.com/tokyogas-tech/hoimin/issues/557)に操作数ゲートと転送の再利用を提案した。[^implementation][^audit]

# 証拠と未確認事項

Leanは、抑制後に候補を許可するなら全モデルruntimeがtyping由来であること、call前のcustom状態を保持すれば任意長の後続で拒否することを証明した。二重走査モデルではleaf訪問数 `2^n` を証明した。有限探索は3イベント、深さ0〜4。[^model]

Lean生成5入力をdebug/releaseの公開planで再生し、それぞれ4 match / 1 mismatchとなった。各binaryのCPython観測10件は全てモデルと一致した。公開runでは不適切な候補がkilled=1、score=1.0に入り、関連Rustテスト9件は成功した。fixture対応はstrict、全トレースとコスト定理はmodel-onlyである。[^audit]

import失敗、動的hook、async、全exit種別、Windows/Linuxは今回の新しいモデル・実行比較の対象外。Rustの訪問カウンタとコスト式は未照合であり、時間計測だけで正確な操作回数を保証しない。製品コードの修正と正式CIへの昇格は未実施。[^audit]

# 再確認の契機

`visit_with`、`apply_finally`、`route_finally_entry`、暗黙例外の追跡条件を変更するときに、監査の正例・負例・性能fixtureを再実行する。実装を修正したらこの監査の履歴を維持し、Issueと正式テストの対応を追記する。

[^audit]: [監査報告・再現コマンド](../../audits/2026-09-15-with-finally/README.md)。
[^model]: [WithModel.lean](../../audits/2026-09-15-with-finally/WithModel.lean)。
[^implementation]: [解析器](../../../crates/hoimin-cli/src/analyzer/rust.rs)。
