---
type: Audit
title: withの例外抑制とfinallyの解析コスト
description: 5e631efで確認した抑制後のimport誤認とfinally二重走査、Lean証明と公開CLIの対応範囲。
status: draft
catalog_revision: 5e631ef
sources:
  - id: issue-556-design
    resource: ../../superpowers/specs/2026-09-24-issue-556-with-suppression.md
    revision: 282e941e4c1a5a23303d30beca881d7bbfde7763
    working_tree: clean
    sha256: 7ff74c9749274839e3cbe0ceb737544de0dd3b5fb5eef90aae17e3ff461269be
  - id: issue-556-implementation
    resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
    revision: 282e941e4c1a5a23303d30beca881d7bbfde7763
    working_tree: modified
    sha256: 17c79e4a1cb5a22cd1deeec35ede2988288ca9145245c874bee23229e72a6af8
  - id: issue-556-tests
    resource: ../../../crates/hoimin-cli/tests/with_suppression.rs
    revision: 282e941e4c1a5a23303d30beca881d7bbfde7763
    working_tree: untracked
    sha256: 9dcb39290a722054de65284cae8cb5ed342b1b30362424bff274f6411c1baa5a
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

import失敗、動的hook、async、全exit種別、Windows/Linuxは今回の新しいモデル・実行比較の対象外。Rustの訪問カウンタとコスト式は未照合であり、時間計測だけで正確な操作回数を保証しない。監査時点では、製品コードの修正と正式CIへの昇格は未実施だった。[^audit]

# Issue #556修正後の確認（2026-09-24）

with本体の暗黙例外と明示raiseを、抑制後の正常継続へ合流する実装へ変更した。複数itemの開始順序、targetへの部分代入、内側の終了処理の失敗を考慮し、成功したreturn・break・continueを抑制経路へ混ぜない。async withと独自managerにも同じ規則を適用する。[^issue-556-design][^issue-556-implementation]

macOSの公開CLIテストでは、既存Lean corpusの5入力に独自manager・複数item・async withの5入力を加え、候補数とCPython 3.14の20観測を照合する。元の再現を実runするテストは、baseline成功、killed=0、候補実行結果0件を確認する。静的な回帰テストはfinally・handlerと各終了種別も区別する。これはRust実装全体の証明でも、過去のLeanモデルをasyncへ拡張した証明でもない。[^issue-556-tests][^issue-556-design]

#557のfinally走査コストは、この修正では変更・再計測していない。import失敗、任意の動的hook、他OSの実行は引き続き今回の確認範囲外である。[^issue-556-design]

[^issue-556-design]: [Issue 556: import facts after context-manager suppression](../../superpowers/specs/2026-09-24-issue-556-with-suppression.md)。
[^issue-556-implementation]: [修正後の解析器](../../../crates/hoimin-cli/src/analyzer/rust.rs)。
[^issue-556-tests]: [公開CLIとCPythonの回帰テスト](../../../crates/hoimin-cli/tests/with_suppression.rs)。

# 再確認の契機

`visit_with`、`apply_finally`、`route_finally_entry`、暗黙例外の追跡条件を変更するときに、監査の正例・負例・性能fixtureを再実行する。実装を修正したらこの監査の履歴を維持し、Issueと正式テストの対応を追記する。

[^audit]: [監査報告・再現コマンド](../../audits/2026-09-15-with-finally/README.md)。
[^model]: [WithModel.lean](../../audits/2026-09-15-with-finally/WithModel.lean)。
[^implementation]: [解析器](../../../crates/hoimin-cli/src/analyzer/rust.rs)。
