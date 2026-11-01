# Issue 479: 型注釈候補を逐次生成する設計

## 問題と契約

型注釈collectorは各siteでその時点の `KnownImports` 全体を複製し、全siteを保持してから候補化する。import aliasがI件、annotationがA件なら保持量はO(I×A)となり、型演算子を選択しない解析にも発生する。

importの再代入、分岐、loop、scopeごとのsnapshot意味、型候補の値・順位・truncationを維持する。注釈範囲はruntime演算子抑止用の `AstFacts` が別に収集するため、型候補collectorだけを省略しても既存の抑止は残る。

## 採用方式

七つの型演算子が一つも選択されていない場合、`type_annotation_candidates` は空prefixを直ちに返す。選択されている場合、`AnnotationCollector` は各annotationとその時点のimportsへの借用をcallbackへ渡し、その場で候補を生成する。imports mapをsiteへcloneせず、collectorが次の文へ進む前に参照を使い切る。

既存のbinding-flowおよびcorrespondence testは所有snapshotを必要とするため、従来の `collect` modeを残す。productionだけが `visit_each` のstreaming modeを使う。

## 検証

実際の `record` とimports snapshot clone位置にtest counterを置く。型未選択はrecord 0・clone 0、型選択はannotation数だけrecordしclone 0を検査する。既存のscope、分岐、loop、再束縛correspondence testを全て通し、候補prefixと公開CLIの回帰も確認する。

releaseでは2,000 alias・2,000 annotationで型未選択と選択を旧版・新版で測り、wall timeと最大RSSを記録する。RSSはCLI全体であり、経過時間とともに合否閾値にはしない。

## 設計セルフレビュー

1. 意味: importsの借用はcallback呼出し中だけ有効であり、collectorの次の状態遷移前に候補を所有化する。各siteのsnapshot時点を変えない。
2. 分離: `AstFacts` のannotation rangeは早期returnの前に既に構築されるため、runtime候補抑止を失わない。
3. 試験互換: test projectionが所有snapshotを比較する用途はstream化せず、productionと同じflow walkerの `record` 分岐だけを追加する。
