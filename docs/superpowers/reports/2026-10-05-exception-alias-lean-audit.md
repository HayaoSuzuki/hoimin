# 代入による別名の形式監査

Issue #692、作業ブランチ `investigate/user-defined-exception-mutations`。
開始時点: `1e67a7b`。本書は原設計レビューで残った別名伝播の問題を対象とする。

## 設計する契約

明示 import、名前への単純・連鎖・注釈付き代入、名前への代入式、属性参照の先頭名を解析する。
`target = source` を「target への書き込みが source の参照先にも影響し得る」という辺として保存する。
名前の再代入で以前の辺を消さず、分岐や実行順、同名の別 scope は保守的に合流する。
private name は既存のクラス名による mangling と整合させる。
属性代入・削除、既存の直接 setattr/delattr を起点に、辺を逆向きにたどった全先頭名へ影響を伝える。
その集合と import の由来を突き合わせ、既存の provider 無効化へ渡す。

対象は書き込みを拒否するための may-alias 解析である。代入別名を新たな候補の表記として許可しない。
関数の戻り値、引数を介した別名、container/destructuring 経由、任意の動的作用は対象外とする。
全 scope の名前を合流するため余分に候補を除外し得る。属性単位の精度改善や診断の細分化は別作業。

代入辺は module ごとに最大65536件、辺の端点は最大131072個とする。
既存 import alias と直接書き込みの上限も明示する。閉包は visited 集合と worklist で構築し、
各辺を有限回調べる。循環や逆順の代入でも終わり、途中結果で候補を許可しない。

## 対応表

| 前提・観測 | Lean | 実装と観測 | mode |
| --- | --- | --- | --- |
| Python fixture と候補 | 生成した source、モデルからの許可判定 | 公開 plan の候補の完全な multiset | strict |
| import の先頭名・由来・relative level | fixture ごとの期待 facts | Ruff AST を既存 Escapes visitor で抽出 | internal-fixture |
| 正規化した代入辺と直接書き込み | 期待 facts | 同じ production visitor の保存情報 | internal-fixture |
| 書き込みの閉包 | Lean の有限展開と fixed-point 判定 | production worklist の結果 | internal-fixture |
| 任意長の到達経路 | 帰納的な到達関係と定理 | 実装の全入力に対する証明ではない | model-only |
| parse/実行/資源監視の失敗 | 意味比較をしない | infrastructure-error として停止 | infrastructure-error |

## 証明と反例の方針

最小反例は `import errors as e; other = e; other.Root = object`。
書き込みを直接の import 名にしか関連づけない旧モデルは、Root を信頼できると誤判定する。
公開 planner の RED と、元コードでは Child、置換後は TypeError となる独立した Python の対照を残す。

Lean は有限展開後に、直接書き込みを含み、全代入辺について閉じていることを確認する。
閉じていない計算結果からは候補を許可しない。帰納法により、その条件を満たす集合は任意長の
別名到達経路を覆うと証明する。これにより、その経路にある import の提供元を信頼しないことを示す。
単なる直接書き込みの定理を繰り返すのではなく、伝播と抽出境界を検証対象に加える。

有限の corpus は別名なし、複数段、逆順、循環、重複、再代入、無関係な提供元、private name、
注釈付き代入、連鎖代入、代入式を含める。旧モデルと1回だけの伝播、辺の向きの反転を壊れた変種とする。
任意のモデル入力の証明と、この有限 fixture の Rust 対応を区別して報告する。

## 実装計画

1. モデル・証明・生成 corpus と抽出観測 adapter を追加する。生成後に旧実装の RED を確認する。
2. bounded な代入辺と worklist を実装し、既存の provider 無効化へつなぐ。
3. 正例・反例・実際の65536件境界・独立 Python 対照を検証する。
4. Lean freshness/sensitivity、公開・内部 adapter、既存階層テスト、workspace、fmt/Clippy を確認する。
5. 残る範囲を文書へ反映し、同一ブランチにコミットする。

Lean は既存 resource guard で1コマンドずつ実行する。各コマンド20秒、RSS2048 MiB、
sample 250 ms を維持し、定理には50000 heartbeats を指定する。計算量を増やす前に測定する。

## 設計と計画のセルフレビュー

設計1: `Root` の公開属性と、既存 Child の親は異なるため、別名をクラス定義 ID に変換しない。
書き込みの由来だけを伝播し、現行の provider 単位の拒否へ渡す。

設計2: 実行順に辺を上書きする方式では、deferred function と再代入が取りこぼしの原因になる。
辺を合流して循環を許容し、visited 集合で停止する方式を選ぶ。過剰除外は明示する。

設計3: 閉包計算の燃料が十分という未証明の仮定に依存しない。
Lean の許可判定は fixed point も確認し、証明は実際に確認した条件から導く。

計画1: 候補だけの比較では抽出漏れを局所化できないので、production visitor の facts も比較する。
テスト用の別 parser や production に test-only API は追加しない。

計画2: 旧 corpus の期待値を変更しない。追加ケースだけを生成し、adapter の mode と件数を更新する。
モデルと Rust で異なる前提を比較しない。

計画3: 新しい辺にも実上限の検証が必要。小さいモデル上限だけで実装の資源保証を主張しない。
未対応の関数・container alias と、原設計レビューの他の残作業は完了扱いにしない。

## 結果

実装・テスト後に追記する。
