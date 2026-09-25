---
type: Audit
title: 評価順序と未選択methodの確保の追加監査
description: 5e631efのbuiltin解決における文字位置と評価順序の違い、およびmethod replacementの不要な確保。
status: draft
catalog_revision: 5e631ef
sources:
  - id: issue-561-design
    resource: ../../superpowers/specs/2026-09-25-issue-561-method-replacements.md
    revision: d2b8e83f054c13eacfceac84e37803fbe71512de
    working_tree: clean
    sha256: a7f402b5fbedc5a56144dc5b1ccf8ea92e0f24a725fd5c632680db85c9c1f370
  - id: issue-561-tests
    resource: ../../../crates/hoimin-cli/src/analyzer/rust/method_replacement_tests.rs
    revision: d2b8e83f054c13eacfceac84e37803fbe71512de
    working_tree: clean
    sha256: 2f7432f828036471d1553f7cfee193146dac60d5c64507b2dcbef8550f52da41
  - id: issue-561-compatibility
    resource: ../../../crates/hoimin-cli/tests/method_replacement_selection.rs
    revision: d2b8e83f054c13eacfceac84e37803fbe71512de
    working_tree: clean
    sha256: 967f008657ee014e41e398320c4f4a8eb4859f82747c8bc918f2f4ba92724048
  - id: issue-561-review
    resource: ../../superpowers/reviews/2026-09-25-issue-561.md
    revision: d2b8e83f054c13eacfceac84e37803fbe71512de
    working_tree: clean
    sha256: 53033fb14da337f31360eaaadd4cde5a7478a804047dadc2fb0f7dc3363622e0
  - id: issue-560-review
    resource: ../../superpowers/reports/2026-09-24-issue-560-review.md
    revision: ffb65c051014f3d9601df2deb0bfeb5ff55c7c38
    working_tree: modified
    sha256: a96d2f42bd974491c68dfa0bf9bee040b6c43725ad70ec515536ebc412dbb146
  - id: issue-560-tests
    resource: ../../../crates/hoimin-cli/tests/builtin_evaluation_order.rs
    revision: ffb65c051014f3d9601df2deb0bfeb5ff55c7c38
    working_tree: untracked
    sha256: 9775e388b7c951d7bcacea8a228e24af94d0864e240310f6fa5c377f6bc5b5b1
  - id: report
    resource: ../../audits/2026-09-15-evaluation-order/README.md
    working_tree: untracked
  - id: model
    resource: ../../audits/2026-09-15-evaluation-order/OrderModel.lean
    working_tree: untracked
---

# 2026-09-15に確認した問題

多重代入の後続targetやstarred引数を評価するとき、既に再代入された名前を組込みと誤認する問題を[#560](https://github.com/tokyogas-tech/hoimin/issues/560)に起票した。名前解決がソースoffsetを評価時点として扱うことが原因である。sourceとdestination両方に影響する。[^report]

未選択のappend変異でも呼出し全体を複製する改善を[#561](https://github.com/tokyogas-tech/hoimin/issues/561)に起票した。深さ32では500KB以上の確保要求の累積量が約97MB、同じ長さの対照は約1MBだった。debug計測であり、peak RSSや速度改善率を示す値ではない。[^report]

# 証拠と限界

Leanは束縛とlookupの順序をモデル化し、再代入後の任意長の後続列で候補を拒否することと、先行lookupを保持することを証明した。組込みへ戻す操作はモデルに含まない。壊した遅延書込み規則は深さ2で検出した。[^model][^report]

Lean生成7入力のstrict照合はdebug/releaseとも2 match / 5 mismatch、実行基盤エラー0だった。元ソース・生成候補は全件CPythonで評価し、公開runのkilled計上も確認した。関連テスト27件は成功した。全イベント列と定理はmodel-onlyであり、Rust全体の証明ではない。確保量は独立した実測で、releaseの確保量は未検証である。[^report]

# 再確認の契機

NameResolutionBuilderの束縛記録やresolve_ordered_at、method replacement helperを変更するときに正例・負例と計測を再実行する。性能修正では、未選択の親callの子にある選択済み演算子の探索を維持する。監査時点では未選択methodの確保に対する正式CIの回帰検証は未確認だった。後続修正を以下に記録する。

[^report]: [追加監査と再現手順](../../audits/2026-09-15-evaluation-order/README.md)。
[^model]: [OrderModel.lean](../../audits/2026-09-15-evaluation-order/OrderModel.lean)。

# Issue #560の修正と2026-09-24の検証

名前の出現位置と評価イベント番号を分離し、右辺の評価、targetへの逐次格納、位置引数・starred引数からkeyword引数への評価順序で組込み名を照会する実装へ変更した。先行する右辺の参照と拡張代入のtarget内の参照は保持する。設計・計画・実装・テストをそれぞれ3回自己レビューし、削除対象にも逐次評価が必要であることを追加確認した。[^issue-560-review]

公開plan/runの2テストは修正前の実装で失敗し、修正後に成功した。公開planの入力には、先行targetの格納後に入れ子の展開が失敗する例と、最初の格納前に失敗する例を含む。後者は到達経路を精密に判定せず、候補を保守的に抑制する。これらはRust・CPythonの回帰検証であり、上記のLeanモデルを使った全実装の証明や、#561の確保量の再計測ではない。[^issue-560-tests][^issue-560-review]

[^issue-560-review]: [Issue 560 review log](../../superpowers/reports/2026-09-24-issue-560-review.md)。
[^issue-560-tests]: [公開plan/runの評価順序テスト](../../../crates/hoimin-cli/tests/builtin_evaluation_order.rs)。

# Issue #561の修正（2026-09-25）

呼出し全体の置換文字列を作る前に、対応する演算子が選択されているか確認する。対象はappend/insert、append/extend、get/subscript、sort/reverseの各方向である。appendの2系統は個別に判定し、subscriptの判定はindex/slice候補の収集後に置く。未選択の親呼出しでも子式の探索を続ける。[^issue-561-design]

回帰テストは7か所のhelper入口と、呼出しの複製・mappingの文字列整形箇所を計数する。未選択時のゼロだけでなく、選択時とhelper直接呼出しの計数も検査する。公開解析APIのテストは修正前に保存した候補の全フィールド・ID・順序と比較し、上限0から全候補数を超える値まで打ち切り結果を照合する。[^issue-561-tests][^issue-561-compatibility]

独立したallocator probeによる修正前後の累積確保要求量と、各3回のセルフレビュー・テスト結果は作業記録に示す。カウンタの操作数、allocatorへの累積要求、同時保持量、経過時間は別の指標として扱う。この修正でpeak RSSや一般的な速度改善率は主張しない。[^issue-561-review]

[^issue-561-design]: [修正設計](../../superpowers/specs/2026-09-25-issue-561-method-replacements.md)。
[^issue-561-tests]: [回帰テスト](../../../crates/hoimin-cli/src/analyzer/rust/method_replacement_tests.rs)。
[^issue-561-compatibility]: [公開解析APIの候補互換性テスト](../../../crates/hoimin-cli/tests/method_replacement_selection.rs)。
[^issue-561-review]: [レビューと検証記録](../../superpowers/reviews/2026-09-25-issue-561.md)。
