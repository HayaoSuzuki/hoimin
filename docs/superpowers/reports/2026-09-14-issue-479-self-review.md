# Issue 479 セルフレビュー記録

対象は `origin/main` `165a2d284a1af92eb02ffd214ba8c0070c2f3808` から作成した `perf/issue-479-stream-annotations` である。

## OKF

1. 形式: YAML、source脚注、本文リンクをvalidatorで確認する。
2. 出典: 設計原文のSHA-256を実ファイルから計算し、未commit状態を記録した。
3. 限定: 型候補collector、runtime抑止用range、test snapshotを区別し、変更対象だけを記述した。

## 設計

1. lifetime: callbackはannotationとimportsを呼出し中だけ借用し、候補をその場で所有化する。
2. flow: import transferやjoinは変更せず、従来の `record` 時点だけをstream化する。
3. compatibility: correspondence projectionは従来のowned collect modeを利用できる二mode構成にした。

## 実装計画

1. 費用: 未選択 `(0,0)` と選択 `(A,0)` を別々に検査し、省略とstream化の両方を必須にした。
2. 演算子: 七つの `MutationOperator::Type*` を列挙し、selector family名ではなく正規化後のselectionを検査する。
3. 検証: scope、branch、loop、rebindの既存correspondenceと全workspaceを含めた。

## 実装

1. early return: `AstFacts` 構築後の型候補producerだけを省き、annotation内runtime候補抑止を保持した。
2. callback: `AnnotationCollector::visit_each` は既存walkerを使い、`record` でcurrent importsをcloneせず借用する。
3. test mode: callbackなしでは従来どおり `AnnotationSite` を所有し、scope kindとimports snapshotを保持する。

## テスト

1. RED/GREEN: 256 alias・annotationで旧コードの未選択 `(records, clones)=(256,256)` を確認し、修正後は未選択 `(0,0)`、選択 `(256,0)` を確認した。
2. semantics: 選択時に候補上限1まで候補が生成されることを確認した。analyzer Rust testは160件成功・2件ignored、workspace全体と既存correspondenceも成功した。
3. release: 55,799-byte、2,000 alias/annotationのmacOS arm64実測。型未選択は旧版0.55秒・568,360,960-byte RSS、新版0.00秒・8,454,144-byte RSS。型選択は旧版0.74秒・569,966,592-byte RSS、新版0.27秒・16,384,000-byte RSS。exit 4は候補上限到達で、RSSはCLI全体、時間とともに合否閾値ではない。

## PR

1. 受け入れ条件: PR #531の7ファイルを再読し、未選択省略、選択stream、flow snapshot互換、候補prefix、RSSがdiffと本文に対応することを確認した。
2. metadata: `gh pr view` でtitle、base `main`、head、`Closes #479`、検証値を照合した。実装・試験・OKF・設計資料以外の変更はない。
3. checks: 作成直後のQuality checkはpendingだった。ローカルのRED、focused、workspace、clippy、OKF、releaseだけを成功として記載し、未検証platformとRSS範囲を明示した。
