# Issue 461 セルフレビュー記録

対象は `origin/main` の `165a2d284a1af92eb02ffd214ba8c0070c2f3808` から作成した `perf/issue-461-lazy-candidates` である。

## OKF

1. 形式: YAML、source脚注、ローカルリンクをvalidatorで検査する。
2. 出典: 設計原文のSHA-256を実ファイルから計算し、未commitの状態を明記した。
3. 内容: 候補保持上限と解析全体の確保量を区別し、visitorの子探索を維持する限定を記載した。

## 設計

1. 選択境界: AST subtreeを省く案を退け、現在のliteralに対するreplacement helperだけをguardした。
2. 費用境界: 共通関数のoriginal複製と、呼出し側で既に生成される巨大replacementを別々に前倒し判定する。
3. 意味境界: range、checked conversion、行・symbol選択をoriginal複製前へ移し、profileは候補内容を使うため後段に残した。

## 実装計画

1. 網羅性: RED計測、二つの実装点、既存collection回帰、release実測、OKF、PRをtaskへ割り当てた。
2. 実行性: 対象関数と `(64, 65)` から `(0, 1)` への期待値を明記した。
3. 独立性: #470を前提にせずmainから作成し、共有箇所は小さい `make_candidate` に限定した。

## 実装

1. operator: `make_candidate` の最初にoperatorを検査し、未選択候補のoriginal複製を防いだ。
2. literal: list/tupleの文脈・注釈条件と同じ局所関数でoperatorを検査し、helperを呼ばない。
3. traversal: `visit_expr` の子訪問制御には触れず、入れ子内部の選択済み演算子を引き続き発見する。

## テスト

1. RED/GREEN: 64階層・64,000-byte payloadで旧コードの実測 `(replacement builds, original copies) = (64, 65)` を確認し、修正後 `(0, 1)` を確認した。
2. 意味: 同じfixtureで後続の `binary_add_sub` 一件、そのoperatorとoriginal、非truncatedを検査した。別fixtureで選択済みlistがhelperを1回呼び、従来と同じ `[item]` から `(item,)` の候補を生成することも確認した。
3. release: 5,000,000-byte payloadで深さ1/30/100を各3回測定した。旧版中央値は0.01/0.04/0.12秒、新版はいずれも0.01秒だった。初回page-cache等の外乱があるため時間は合否条件にせず、確保位置counterを決定的回帰条件とする。

## PR

1. 受け入れ条件: PR #529のdiffを再読し、未選択helper、original、子探索、候補値、累積確保と保持上限の区別が実装と本文に対応していることを確認した。
2. メタデータ: `gh pr view` でtitle、base `main`、head、`Closes #461`、検証値を確認した。変更ファイルは実装・試験・OKF・設計資料の7件である。
3. checks: 作成直後のQuality checkはpendingだった。ローカルのfmt、clippy、focused、workspace、OKF、release実ログは本文に記載し、未完了のremote checkを成功とは記載していない。
