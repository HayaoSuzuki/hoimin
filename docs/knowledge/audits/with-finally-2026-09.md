---
type: Audit
title: withの例外抑制とfinallyの解析コスト
description: 5e631efで確認した抑制後のimport誤認とfinally二重走査、Lean証明と公開CLIの対応範囲。
status: draft
catalog_revision: 5e631ef
sources:
  - id: issue-557-design
    resource: ../../superpowers/specs/2026-09-25-issue-557-finally-traversal.md
    revision: f071781e20bc87c13b537ecd620e5ed06e9cd1f6
    working_tree: clean
    sha256: a2b984813077de46a2efac8105cc689f42641d23c87c24fb4f26c269672f8f6d
  - id: issue-557-tests
    resource: ../../../crates/hoimin-cli/src/analyzer/rust/finally_transfer_tests.rs
    revision: f071781e20bc87c13b537ecd620e5ed06e9cd1f6
    working_tree: clean
    sha256: 88e229c3d09e03c30001f2f71090540d9f18eb4cb49f56dd7cda0279f285301e
  - id: issue-557-review
    resource: ../../superpowers/reviews/2026-09-25-issue-557.md
    revision: f071781e20bc87c13b537ecd620e5ed06e9cd1f6
    working_tree: clean
    sha256: 4ad3ea7059e45d4a56d1ba819c0d9266e0d6de5ef46356bc3319be4c562ef96c
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

# Issue #557の修正（2026-09-25）

注釈を記録しない転送処理では、`apply_finally`の注釈記録用走査を省略する。終了経路ごとの転送と、finallyによるreturn・break・continue・raiseの上書きは従来の処理を使う。複数の入口を合流した記録結果を転送に流用する変更や、キャッシュの追加は行っていない。[^issue-557-design]

回帰テストは実際の文・注釈訪問回数を数え、module/class/functionの入れ子finallyを検査する。記録を無効にした単一経路では最深部の注釈への訪問が1回になる。旧走査を復元するテスト用切替で回数制限の感度を確認し、候補の内容と順序、記録されたimport状態、明示・暗黙の終了経路を比較する。入力形状別の性能ゲートにも登録した。[^issue-557-tests][^issue-557-review]

深さ16〜20のrelease計測と候補比較の結果は作業記録に示す。訪問回数の上限はこのfixtureに対する検査であり、任意の複数終了経路を持つプログラムの計算量保証ではない。過去のLean定理は二重走査モデルについての証明として保持し、今回のRust計測や時間測定と区別する。[^issue-557-review]

[^issue-557-design]: [修正設計](../../superpowers/specs/2026-09-25-issue-557-finally-traversal.md)。
[^issue-557-tests]: [回帰テスト](../../../crates/hoimin-cli/src/analyzer/rust/finally_transfer_tests.rs)。
[^issue-557-review]: [レビューと検証記録](../../superpowers/reviews/2026-09-25-issue-557.md)。
