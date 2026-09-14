# Issue 482 セルフレビュー記録

対象は `origin/main` `165a2d284a1af92eb02ffd214ba8c0070c2f3808` から作成した `perf/issue-482-index-name-history` である。

## OKF

1. 形式: validatorで16ページのYAML、source脚注、local linkが成功した。
2. 出典: 設計原文の実SHA-256と未commit状態を記録した。
3. 限定: 構築時のoffset sort・tree更新と照会時の二分探索を区別し、sort費用をquery counterへ含めない。

## 設計

1. 順序: source offset sortはvisitorの挿入意味を変えるため採用せず、葉を挿入順に保持した。
2. 状態: Bind/MaybeBind/Unknownを単一値へ縮約せず、3入力状態すべてへの変換として合成した。
3. 構築/照会: 最初のmin/max pruning案が交互offsetでO(N)照会を残すと再レビューで判明した。offline activation O(N log N)と任意履歴のO(log N)照会へ修正した。

## 実装計画

1. oracle: production helperを呼ばない独立matchのslow foldを比較対象にした。
2. boundary: 非単調offset、同一offsetのeffect順、生成履歴、全query境界を含めた。
3. 統合: builder終了時に全履歴をfinalizeし、既存record箇所とscope解決分岐は変更しない。

## 実装

1. composition: 葉は元挿入位置に固定し、左nodeの変換後に右nodeを適用する。offset順activationでもfold順は変わらない。
2. snapshot: 同一offsetのeventを全activateした後だけroot状態を保存し、境界の含有規則 `event_offset <= query` を維持する。
3. lifecycle: builder完了時に一度だけfinalizeし、空履歴はbuiltin、照会は累積snapshotの二分探索とする。

## テスト

1. parity: 手書きの非単調・同一offset三順序と、長さ0～127の生成履歴を全境界で独立foldと比較した。
2. cost: 4,096 eventについて実際の葉・祖先更新が `N × (log2 N + 1)` 回であることを確認し、4,096照会の実比較を `queries × (log2 N + 1)` 以下に制限した。交互offsetの8,192照会も同じ対数上限で検査した。最初のmin/max案の厳しい上限がREDとなり、レビューで残る最悪線形を検出して方式を置換した。
3. release: N=8k/16k/32k/64kのmacOS arm64中央値。旧版0.05/0.19/0.74/2.75秒、新版0.02/0.02/0.04/0.08秒（8k新版初回0.24秒は中央値から除外されない3値の中央0.02）。時間は合否閾値ではない。

## PR

1. 受け入れ条件: 挿入順、三effect、scope、cost counter、release scalingをdiffと本文に対応付ける。
2. 証拠: parity 4件、analyzer Rust 159件成功・2件ignored、workspace全体、clippy、OKF、releaseの実ログを確認した。
3. metadata: base main、closing keyword、変更ファイル、remote check、未検証platformを確認する。
