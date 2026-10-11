# #711 survivorの評価と履歴の保存設計

2026-10-11。対象コードはmainの `d46f13a43667323b8d0374d6c5409942f4c3c822`。
この文書は実装前の設計であり、CLI・保存形式・証明はまだ実装していない。

## 目的と範囲

[Issue #711](https://github.com/HayaoSuzuki/hoimin/issues/711) の目的は、一度調査した候補の有用性と理由を保存し、同じ候補を再調査する負担を減らすことである。
初期版では保存planを参照して、人が付けた評価の記録、履歴、一覧を扱う。
実行結果と人の判断は別の値とし、評価による候補の抑制、ランキング変更、スコア補正、学習は行わない。
外部review manifestの取込み、URLの取得、プラグイン、実行コマンドの追加も対象外とする。

同じstackの [#759](https://github.com/HayaoSuzuki/hoimin/issues/759) は、既存のplan/verifyを外部から組み合わせる段階検証の調査を先に行う。
#711を下段、#759を上段に置くが、#759の機能実装を#711の完成条件には含めない。

## 既存構造と選択

`plan.rs` のplan v5には正規化済み設定、source hash、完全な候補が保存されている。
候補IDの生成にはファイルhash、path、span、operator、replacementが使われ、originalは別途検証する必要がある。
`sequence`、rank、表示位置だけでは候補を同定しない。
既存のverify準備処理は解析プロセスを起動するため、reviewでそのまま呼ばない。
planの構造検査と必要なソース検査だけを共有する読取り処理を分離する。

保存方式には、JSON全体の書換え、JSONLの追記、SQLiteの独立sidecarを比較した。
JSONは履歴と同時更新を安全に扱うための独自ロックと置換処理が必要になる。
JSONLは途中書込みの回復と比較更新を別途設計する必要がある。
既存依存のrusqliteを使うSQLite sidecarを採用し、履歴追加とrevision更新を同一トランザクションにする。
実行session DBとは分離し、review保存を実行結果の更新経路に接続しない。

## CLIと利用例

新規コマンドを次の形とする。storeはすべて明示指定し、自動探索しない。

```sh
hoimin review list plan.json --store reviews.sqlite3 --format json
hoimin review show plan.json --store reviews.sqlite3 --candidate ID --format json
hoimin review record plan.json --store reviews.sqlite3 --candidate ID \
  --assessment actionable --reason '必須フィールド欠落をテストで検出できていない' \
  --expected-revision 0
```

`list` と `show` はtext/JSONを選択できる。既定はtext。
`record` も同じ出力形式を選べ、成功時に保存されたrevisionを返す。
評価は `actionable`、`not_useful`、`suspected_equivalent`、`needs_investigation` の四つで、記録がない候補は `unreviewed` と表示する。
`unreviewed` は保存する評価値ではない。
reasonは必須、`--evidence TEXT` は任意の繰返し指定で、参照文字列をそのまま保存する。
URL、ファイル、コードとして解釈・実行しない。
reasonは空白除去後に非空、UTF-8で4 KiB以下、evidenceは最大8個・各2 KiB以下とする。
改行とtab以外の制御文字を拒否し、text表示時にはterminal escapeを出力しない。

`--expected-revision` は必須で、最初の記録には0、更新にはshowで得たrevisionを指定する。
同じ候補を同時に編集した場合、先にcommitした側だけが成功し、もう一方にはrevision競合を返す。
同じ内容を再登録した場合も、明示した評価行為として新しい履歴を追加する。
履歴の削除、過去イベントの上書き、暗黙の一括評価は提供しない。

保存planには実行結果がないため、planだけの表示ではexecution statusを `not_loaded` とする。
任意の `--report PATH` で既存readerが検証できる完了済みrun reportを付ける場合だけ、実際のstatusを併記する。
初期版の対象は現在版v4のJSON reportとし、JSONL・旧版のreview連携は追加しない。
既存readerから検証済みproject rootとrun IDを取得できるよう、必要最小限の読取り結果を公開する。
入力reportのsummary再計算・整合性検査を省略しない。
別project、候補descriptorの不一致、未完了reportは明示的なエラーとし、reportに存在しない候補は `not_in_report` と表示する。
recordにreportを付けた場合は、その候補がreportに存在することも必要とする。
planだけでの評価登録は許可し、候補がsurviveしたとは推測しない。

## 同定とstale

storeは一つの正規化・canonical化済みproject rootに束縛する。
別checkout、移動したrootへの自動移行はしない。別projectとしてエラーにする。
候補キーはprojectと既存CandidateIdentityを使い、保存時・読取り時にはoriginalを含むCandidateDescriptor全体も照合する。
IDが同じでもdescriptorが異なればidentity conflictであり、評価を適用しない。
sequence、rank、plan内の順序の変更は評価の同定に影響しない。
参照元planのpathと内容hashを履歴に残すが、同じ候補を含む別planへの適用をplan hashだけで拒否しない。

recordでは該当するソースを読み、既存の候補検証でhash・span・originalを確認する。
未知のID、source変更、読取り不能、不正な候補を拒否し、履歴を変えない。
list/showではsource変更を `stale`、読取り不能を `source_unavailable` と表示し、有効な評価として集計しない。
変更後の新しい候補へ旧評価を移さない。
`list --stale` はstore内の旧候補も別枠で列挙し、現在のplan候補の集計と混ぜない。
sourceの鮮度は読取り時点のsnapshotに対する判定であり、その後の編集まで防止する保証ではない。
同一ユーザーによる悪意あるファイル差替えやhashの衝突耐性は、形式証明の対象ではない。

## 保存と障害時の契約

storeはschema v1の独立SQLite DBとし、application IDとuser_versionを検査する。
projectメタデータ、候補descriptor、revision付きの評価イベントを保持する。
イベントは評価、reason、evidence、UTCのUnix millisecond時刻、参照plan、任意の参照reportのpath/hash/run IDを含む。
履歴の順序は時刻ではなく候補ごとのrevisionで定める。
JSON出力も専用schema v1とし、既存plan/reportの版は変更しない。

recordはBEGIN IMMEDIATE内でproject、descriptor、expected revision、容量を確認し、イベントを追加してcommitする。
失敗時は明示的にrollbackし、途中成功を返さない。
SQLiteのロック待ちは1秒までとし、競合はbusyとして再試行可能なエラーを返す。
同じ候補の更新はrevision比較で、異なる候補の更新はDBのトランザクションで直列化する。
list/showは読取りsnapshotを使い、storeがなければ未評価を表示するだけでファイルを作らない。
未知schemaや壊れたDBを自動初期化・上書きしない。

入力はユーザーのローカルplan/reportとローカルstoreに限定する。
plan/reportは各64 MiB、DBは16 MiB、履歴はDB全体で10,000イベントを上限とし、超過時は拒否する。
新規DBにはpage数の上限も設定するが、journal・一時領域まで含むディスク使用量の上限とは主張しない。
既存入力にはサイズを確認してから上限付き読取りを使う。
DBの整合性・既知schemaを検査し、view/triggerなど設計外のschemaを拒否する。
SQLは固定文とbound parameterだけを使い、trusted_schemaを無効化する。
storeがplan/report/sourceそのものを指す場合や、storeの最終要素がsymlinkの場合は拒否する。
共有ネットワークファイルシステムでの同時更新は初期版の保証対象にしない。

## 表示と実行結果の分離

listの集計はoperatorごとの現行候補数、reviewed、unreviewed、評価別件数、stale/source_unavailableを示す。
reviewedはsourceが一致し、最新イベントを正確に適用できた候補だけを数える。
古い履歴を新しい候補のnot_useful件数に含めない。
`suspected_equivalent` は人の疑いであり、等価性の証明を意味しない。
たとえば「仕様が順序を許す並べ替え」はnot_usefulの理由になり得るが、「必須フィールドを落としても通る」はactionableの理由になり得る。
どちらの判断もMutationStatusやkill率を変更しない。

reviewは独立した読取り・保存・表示モジュールからCLIへ接続する。
executor、ranking、score集計、exit policy、resume、progressの判断処理にはreviewを渡さない。
通常のrun/plan/verify/progressはstoreを読まない。
JSON表示ではcandidate、execution、review、applicability、historyを別のフィールドにする。

## 検証と形式証明

テストは、同じIDの偽descriptor、別project、source変更、未知ID、未評価とnot_usefulの違いを優先する。
保存では初回登録・更新・再読取り、revision競合、同時初期化、途中失敗のrollback、壊れたDB、容量超過を小さなfixtureで検証する。
report連携では正常入力、誤帰属、欠落候補、未完了を確認する。
実行結果の同一fixtureに対してreview前後の生のstatus、summary、score、progress判断が同じであることを確かめる。
reviewが解析・テストコマンドを起動しないことも検証する。
operatorの総当たりや巨大OSSでのE2Eは、この保存機能の検証には追加しない。

既存Lean基盤で、候補への適用判定とrevision付き履歴更新の小さな純粋モデルを追加する。
次の三性質を証明対象とする。

1. 適用成功ならproject・descriptor・sourceの鮮度条件をすべて満たす。
2. 更新を拒否した場合、revisionと履歴を含む状態全体が変わらない。
3. 更新成功なら旧履歴がprefixとして残り、revisionが1増える。

Leanから境界ケースを生成し、Rustの純粋判定との対応を差分テストする。
誤ったidentity判定・revision比較を検出できるケースを含める。
証明はモデルの性質であり、Rust実装、SQLite、OSの永続化、hashの暗号学的性質を検証済みと扱わない。
実行結果に影響しないことはモジュール分離と実装テストで確認し、単なる表示projectionの恒等式を保証として増やさない。

## 互換性とstack

既存コマンドの挙動と既存保存形式を維持し、新コマンド・新sidecarだけを追加する。
実装が配布物に入る場合は、ユーザー指定に従って0.4.0とし、その時点のmainの版と整合させる。
この設計文書だけのcommitでは版を変更しない。
作業は `feat/issue-711-survivor-review` と、その上の `research/issue-759-staged-verification` に積む。
設計、実装計画、実装、テストでそれぞれ最低5回のセルフレビューを行い、各段階の実施内容を記録する。
Rust buildを行う際には空き容量を確認し、段階の切れ目でcargo cleanする。

## 設計セルフレビュー（5回）

1. 要求照合: run statusと人の評価を混同する余地を確認した。plan単独のstatusをnot_loadedとし、未評価を保存評価に含めない契約を加えた。
2. 誤帰属: ID単独照合とplan hashへの過度な依存を確認した。project・完全descriptor・source検査と、plan順序変更を許す条件を明記した。
3. 保存障害: JSON追記だけでは更新競合を防げない点を確認した。独立SQLite、必須expected revision、rollback、読取りsnapshotを採用した。
4. 入力と費用: 新たな外部取込みや巨大E2Eへ広がらないか確認した。ローカル入力・容量上限・固定SQL・最小fixtureに限定した。
5. 保証範囲: Leanモデルの証明を実装全体の証明と誤認しないか確認した。三性質とRust差分検査を定め、OS/DBとscore不変性は実装検証に残した。

実装計画、実装、テストのセルフレビューは、それぞれの段階で実施する。設計レビューの回数を他段階へ流用しない。

## 根拠

- [候補identityと検証](../../../crates/hoimin-core/src/candidate.rs)
- [plan生成とverify準備](../../../crates/hoimin-cli/src/plan.rs)
- [report読取り](../../../crates/hoimin-cli/src/progress/input.rs)
- [既存SQLite schema](../../../crates/hoimin-cli/src/session/schema.rs)
- [既存Leanモデル](../../../formal/HoiminOracle)
- [SQLite transaction](https://www.sqlite.org/lang_transaction.html): 明示transactionとwriter競合の契約。
- [SQLite security](https://www.sqlite.org/security.html): schemaの信頼と入力制限の判断材料。
