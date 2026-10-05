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
同じ集合に含まれる module の束縛も既存の Unknown として除外する。
実装レビューで `other = Root; other.__init__ = ...` は import を経由しないと気づき、
この条件を追加した。書き込みのない正例は1候補、書き込みありは0候補というテストが旧処理で失敗した。

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

### 証明と実装の対応

`closed_writes_cover_alias_paths` は、直接書き込みを含み、代入辺について閉じている集合が、
任意長の `WriteReach` 経路を覆うことを帰納法で証明した。
`alias_path_invalidates_provider` は、その経路で到達する import の提供元を `aliasTrusted` が
許可しないことを証明した。燃料不足で閉包条件を満たせない場合も許可しない。
いずれも `sorry`、追加公理、`native_decide` に依存しないカーネル検査の対象である。

Rust は名前の visited 集合と worklist で閉包を計算する。
辺の向き、正規化した import・代入・書き込み、閉包の完全な集合を、Lean の17 fixture と比較した。
期待値は Lean が生成し、adapter は同じ production visitor を呼ぶ。
期待する中間情報を Rust 側で再計算して自分自身と比較する方式ではない。
並びだけを正規化し、重複した観測を比較時に消さない。

公開 planner の125ケースと、内部抽出17ケース、既存 snapshot 170ケースの対応を確認した。
追加した34行以外に旧 corpus の変更はない。ここで確認したのは有限の fixture に対する対応であり、
Rust の全入力や Python 全体の意味を Lean で証明したものではない。
別ファイルの提供元だけでなく、影響する同一 module のクラス束縛も Unknown として除外する修正を加えた。

### 反例と感度

公開回帰テストは旧処理で `Child → Root` が残ることを確認して失敗した。
同じ fixture を独立 Python プロセスで実行すると、元コードは Child、Root への置換後は TypeError となる。
同一 module のクラスの constructor を別名で変更する追加テストも、修正前は1候補、期待0候補で失敗した。
書き込みのない対照例は1候補を残す。

Lean の感度確認は、従来14変種に「代入を無視」「1回だけ伝播」「辺の向きを反転」の3変種を追加して検出した。
別名 fixture は17個、各最大3辺であり、全 Python 構文や全グラフの有限列挙とは主張しない。
既存 snapshot 探索は alphabet 4、深さ0–3を維持し、深さ3で85 trace、228 event を評価した。
別名の重複・循環を含めても探索範囲や資源上限を増やしていない。

さらに Rust の抽出辺を一時的に反転すると `alias-extraction-chain` が、伝播を無視すると
`alias-assignment` の公開 planner 比較が、それぞれ semantic mismatch を検出した。
これらはテスト後に復元した。抽出境界と候補への反映の両方をテストが観測することを確認した。
atomicity は DB 更新のない静的解析には適用せず、上限超過時に部分 index を公開しない既存契約を維持する。
重複辺・循環は idempotency、辺の向き・複数段・再代入は到達関係、実上限は boundary の確認に対応する。

### 実装のセルフレビュー

1. 作用の到達先: import だけを無効にすると同一 module のクラスを取りこぼす。
   RED を追加して、閉包に含まれる module 束縛も除外するよう修正した。
2. 資源と終了: 代入辺と直接書き込み名に各65536件の上限を設け、重複辺は件数を増やさない。
   閉包は直接書き込み名と辺の端点の和集合以内で、同じ名前を繰り返し queue へ入れない。
   provider 無効化の前に上限超過を拒否する。
3. 構文と対応: 単純・注釈付き・連鎖・代入式、private name、逆順と循環を確認した。
   production の辺反転と伝播無効化をテストが検出することも確認した。
   関数・container 経由の参照は対応したとは扱わない。

### テストのセルフレビュー

1. 意味: 元コードも壊れているだけの反例を避け、正常な Child の送出を独立プロセスで確認する。
   未変更・無関係な提供元の正例が残ることをモデル生成の期待値で比較した。
2. 境界: 実際の65536辺で成功、重複追加も成功、65537辺でエラーとなることを公開 planner で確認する。
   直接書き込み名の65536件、重複、65537件も production parser/summary の経路で確認する。
3. 観測: strict と internal-fixture の観測を混同しない。型・必須 field・mode の整合性を検査し、
   corpus の JSON 不整合を意味上の mismatch として扱わない。比較では集合に変換して重複を隠さない。

### 検証コマンドと資源

Lean プロジェクト内で、各コマンドに次の prefix を付けて逐次実行した。
`NAME` は各実行で異なる統計ファイル名に置き換える。

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 \
  --rss-limit-mib 2048 --sample-ms 250 --stats /private/tmp/NAME.json -- COMMAND
```

`COMMAND`:

```sh
lake build +HoiminOracle.ExceptionHierarchyModel:o
lake build +HoiminOracle.ExceptionHierarchyProofs:o
lake build +ExceptionHierarchyAuditMain:o
lake exe generate_exception_hierarchy --output corpus/exception-hierarchy.jsonl
lake exe generate_exception_hierarchy --check corpus/exception-hierarchy.jsonl
lake exe generate_exception_hierarchy --sensitivity
lake env lean -j1 -DElab.async=false HoiminOracle.lean
```

単一ケースの再現は repository root から実行する。

```sh
HOIMIN_ORACLE_CASE=alias-assignment cargo test -p hoimin-cli --test lean_exception_hierarchy_oracle
HOIMIN_ORACLE_CASE=alias-extraction-chain cargo test -p hoimin-cli --lib lean_exception_hierarchy_alias_extraction_correspondence
cargo test -p hoimin-cli --test exception_hierarchy
cargo test -p hoimin-cli --lib hierarchy
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

検証中の修正: resource guard の相対パス誤り、証明の Bool/let 展開エラー、
Lean の3要素 tuple が入れ子 JSON 配列になることによる adapter deserialize エラーがあった。
全体ビルドでは parser を単独で読み込むテストの `crate::cli` 解決失敗があり、parser 関数を
workspace adapter から渡す形に直した。Clippy の補助 module 重複読み込みも、共有 module を1箇所にして解消した。
それぞれ修正して再実行し、証明の前提や期待値を実装に合わせて弱めることはしなかった。
タイムアウトやRSS上限の引き上げは行っていない。

全 workspace テストは終了コード0で成功した。階層の integration test は20件、
公開 oracle は125ケースを確認した。補助 module の共有化後にも、library の階層テスト21件、
公開 oracle、単独 parser 経路の階層テスト14件を再実行して成功した。
fmt と all-features Clippy も成功した。

成功した Lean 実行の最大値は8.694秒、監視対象 process tree のRSSは約1220 MiBだった。
モデル、証明、生成、freshness、sensitivity、集約 import の全検査が制限内で成功した。
resource guard の20秒・2048 MiBと各定理の50000 heartbeats は維持している。

今回の範囲である代入別名の伝播、抽出境界の対応確認、同一 module の書き込み拒否は完了した。
原設計レビューの診断の細分化、属性単位の精度改善、並行 cache の契約、全体のメモリ測定は残作業とする。
関数や container に渡った参照、外部の動的作用を追跡したという保証は追加していない。
