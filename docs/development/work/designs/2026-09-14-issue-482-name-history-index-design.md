# Issue 482: 名前束縛履歴索引の設計

## 問題と意味契約

module/classの組込み名解決は、参照ごとに同名の全 `BindingEffect` を挿入順で走査し、offset以下だけをfoldする。束縛と参照がN件ずつある単調な通常入力ではO(N²)となる。

visitorの挿入順は必ずしもsource offset順ではない。同一offsetのBind、MaybeBind、Unknownも非可換な場合があるため、offset sortや末尾eventだけへの置換は行わない。既存の挿入順foldを参照意味とする。

## 採用する索引

各名前のeventを元の挿入位置に対応するsegment-treeの葉へ割り当てる。eventをoffset順にactivateし、同一offset群をすべてactivateした後、rootの挿入順合成変換をそのoffsetの累積状態として保存する。照会は累積状態をoffsetで二分探索する。

この方式は非単調offsetでもeventの適用順を変えない。構築はoffset sort O(N log N)と、eventごとの葉・祖先更新O(N log N)。照会は履歴形状によらずO(log N)、保持する累積状態はunique offset以下である。finalizeはbuilder完了後に一度だけ呼ぶprivate不変条件である。

## 検証

productionと独立にmatchを書いた遅い挿入順foldをtest oracleとし、非単調offset、同一offsetの異なるeffect順、生成履歴の全境界で索引結果を比較する。単調4,096 eventでは前処理の葉・祖先更新数と全照会の比較上限を検査する。交互offset履歴でも全照会の比較上限を検査する。経過時間はrelease比較の証拠に限る。

## 設計セルフレビュー

1. 順序: source sort案は非単調visitor順で意味を変えるため退け、葉と左右合成を挿入順に固定した。
2. 合成: 3つの入力解決状態すべてに対する変換をnodeへ持たせ、MaybeBindの直前状態依存を保持した。
3. 費用: 最初のmin/max pruning案は交互offsetで線形照会を残すため退けた。offline activationの葉・祖先更新とlookup比較を実処理位置で別々に数え、sort費用は比較counterに含まないと明記した。
