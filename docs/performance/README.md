# 入力形状別の性能検証

[shapes.json](shapes.json) は、8次元・29形状の入力、N/2N/4Nのサイズ、測定対象、実行可能な回帰テストを記録する台帳である。`growth_model` は入力の構造、`expected_after_fix` は依存Issueが統合された後の期待値を示す。実測の記録は、それぞれに明記したrevisionの観測として読む。

| 次元 | release入力 | 決定的な通常ゲートとの対応 |
| --- | --- | --- |
| 対象発見 | 非対象・対象ファイル数、file/symbol/line selector宣言数を独立に増やす | #453の走査数、#474のselector照会、#475の行範囲統合をactive登録 |
| fingerprint | ファイル数・総bytes・重複glob数とexact/glob混在 | #457のwalk回数をactive登録 |
| ソース配置 | 同じbytes・候補を長い1行と多数行へ配置しUnicode列も比較 | #470の列照会をactive登録。既存token範囲照会をactive登録 |
| AST | 幅、左深さ、巨大literal、未選択collectionを別々に増やす | #461のreplacement生成数をactive登録。既存prefix/overflowをactive登録 |
| 解析状態 | import幅×annotation数、再代入×呼出数。選択・候補0対照 | #479/#482の操作数をactive登録 |
| verify | 1file・多file、top1/topN、明示した不完全plan | #456の前処理回数をactive登録 |
| 出力 | mutant record数とrecord長を独立に増やす | report/progressのallocator peak、#463のwrite数に加え、1reportの内容500/1,000/2,000件×履歴2/4/8のprogressゲート |
| workspace | file数・bytes・worker数を独立に増やす | 作成後retained heapとpreflight allocator peakを別ゲートで測る |

現在のブランチは26個のactiveゲートを実行する。これで全入力形状の性能回帰を網羅するわけではない。追加の修正が未導入の場合はpendingとして登録し、成功数へ含めない。修正を取り込んだ後に実際のテスト名と計数箇所を照合し、`args` を指定して実行してからactiveへ変更する。単にラベルを変えて検証済みにしない。

## 通常ゲート

```sh
python tools/performance_shapes.py check
python tools/performance_shapes.py gate --output /tmp/hoimin-perf-gates-new
```

指定したRustテストを `--exact` で実行し、0件一致、ignoredのみ、非zero終了を失敗にする。通常CIと手動非Linux CIは同じ小規模ゲートを使う。各cargoコマンドの外部上限は300秒。全体のjob上限は既存CIに従う。全workspaceテストの代わりではない。

## release計測lane

```sh
python tools/performance_shapes.py measure \
  --baseline /absolute/path/to/baseline/hoimin \
  --candidate /absolute/path/to/candidate/hoimin \
  --output /tmp/hoimin-perf-measurements-new
```

`--shape long-line --shape many-lines` で絞り込める。標準は各サイズ3反復で、3〜10回を指定できる。両方の実行ファイルのSHA-256、argv、OS/CPU/Python/Rust、入力bytesとファイル数、候補数、truncation、終了コード、raw時間/RSSと集計を保存する。実行ファイルのdigestだけでソースrevisionは復元できないため、比較するcommitを実行記録にも書く。

stdout/stderrをファイルへ出し、既存resource guardで1実行30秒・プロセスツリー2GiBを上限とする。guardは25ms間隔でRSSをサンプリングする。観測できなかった短命なプロセスはnullとし、0bytesとは扱わない。経過時間はguardの起動監視・終了確認を含む。小さなfixtureでは監視の固定費が支配的になり、今回の標準サイズだけで漸近的な時間/RSSの保証はできない。allocator peak、retained heap、RSSを互換の値として比較しない。

入力projectは実行ごとの一時ディレクトリに作り、完了時に削除する。結果ディレクトリは既存のものを上書きせず、新規パスだけを受け付ける。失敗時も、その実行のstdout/stderrと判定を保存する。出力先の作成自体ができない場合はartifactを保存できない。予備のverify plan生成は計測外で、失敗した場合は測定成功と数えない。

GitHub Actions の `Performance measurements` は手動実行専用。選んだrefと `baseline_ref` をrelease buildし、同じ台帳で計測してartifactを保存する。通常PRの合否に時間/RSSの閾値を加えない。POSIXのプロセス監視を使うため、計測laneはLinux/macOS向けであり、WindowsのRSS実測を済ませたことにはならない。

## Leanの証拠

台帳の `lean` から既存のモデル・証明・生成corpus・Rust adapterをたどれる。生成器のbuild、`--sensitivity`、`--check` をresource guard付きで順番に実行した後、`lean-target-reads` gateで実装の対象読取り数を照合する。caseはLeanで定義し、JSONLを手編集しない。既存の0/1/上限/overflowケースはprefixと順序を検証し、CLI全体の定数メモリやnative stackを保証しない。追加した `lean.cost` は56ケースでbinding索引、annotation callback内の全状態clone、実replacement builderの回数・bytesを照合する。0/1、7/8/9、8/16/32と、実際に全走査・clone・filter前buildを行う壊した経路を検査する。counterの観測には内部seamが必要なためinternal-fixtureとし、詳細は[対応表](../superpowers/reports/2026-09-14-issue-491-cost-correspondence-worksheet.md)に記録する。

## 個別修正を組み合わせた確認

[2026-09-14の統合確認](2026-09-14-integration-check.md) に、10件のPRのRust差分を組み合わせたrevision、競合解消、全workspace試験の結果を記録した。依存修正を取り込んだ統合時には21ゲートを実行してactiveへ昇格した。mainへのマージは各PRのCI成功後に順次行う。


## 追加した実行証拠

[今回のrelease計測](2026-09-14-issue-491-expanded-measurement.json)は29形状×3サイズ×3反復×2実行ファイルの522回を記録する。出力document bytes、同一binary内のN/2N/4N比、baselineとの比を保存し、未観測RSSはnullのままとする。既存198回の監査とは別の実行である。詳しい環境・digest・感度検証は[入力軸の報告](../superpowers/reports/2026-09-14-issue-491-input-axis-review.md)に記載した。

preflight・worker作成・resetの内容バッファは固定長で、最大ファイルサイズに比例するヒープを保持しない。manifest entriesなどのメタデータはファイル数・ディレクトリ数に比例する。専用allocatorゲートは単一ファイル1/8/32MiBの各段階を測定し、実際の全内容読取りを対照にする。これはCLI全体の定数メモリやRSSの保証ではない。

resetは同サイズの全バイト比較を維持し、未変更ファイルを再作成しない。同サイズの変更を検出した場合は、同じsnapshotハンドルを巻き戻して復元するため、そのファイルのsnapshot読取り量は最大2倍になる。サイズ不一致ではworker内容を読まず復元する。contractsの事後検証は別途1回比較する。ソースからsnapshotとhashを作る読取り、およびpreflightの最終再検証は各1回のままである。


progressのallocatorゲートは同じ関数名のまま、1reportの内容500/1,000/2,000件と履歴2/4/8を独立に増やす。元の2,000件×16履歴も保持する。実際のreport bytesを記録し、各reportサイズで履歴2に対する追加peakを512KiB以下に制限する。全入力が利用可能で、候補対応数、score、差分、停滞判断が正しいことを同時に検査する。実際に全履歴のparsed reportを保持する対照は同じ上界を超える。この通常ゲートの追加確認を522回のrelease実測に含めない。

## Nested control-flow reanalysis (Issue #546)

The `control-flow-reanalysis-*` exact Rust gates measure a separate input axis:
loop nesting with empty imports, not expression depth or AST width. They count
actual `visit_statement_flow` calls and actual annotation-statement visits at
depths 1, 2, 4, 8, 16, and 20, for both for and while in module and class scope. With the type operator
selected, the bounds are `(d+1)(d+2)/2` total statements and `d+1` annotation
visits (add one total statement for the class definition). The unselected control must perform zero annotation-flow visits, with
zero candidates and no truncation. A second gate re-enables the old replay at
depth 8 and requires it to exceed the same bound without changing candidates.

Only the stable final transfer from the current fixed-point invocation is
reused; no map of cached import snapshots is retained. Annotation recording,
a changed class fallback, and test projections keep their full traversal.
A fallback snapshot is kept only during each active class-loop evaluation,
then discarded; retention depends on fallback size and active nesting depth. These gates
do not establish a quadratic bound for changing import states, class scopes,
or arbitrary finally paths; elapsed time and RSS are not pass/fail metrics.
See the [design](../superpowers/specs/2026-09-15-issue-546-loop-transfer-design.md)
and [review](../superpowers/reports/2026-09-15-issue-546-loop-transfer-review.md).

## 監査入力の回帰ゲート

`audit-promoted-546` は監査原文と同じforループの深さ18／19／20、`audit-promoted-547` はimportとint注釈の数512／1,024／2,048を使う。どちらも型演算子選択・未選択で、候補なしと操作数の上限を検査する。台帳のactiveゲートとして通常CIから実行する。

旧監査の時間・RSS測定は履歴として保持する。今回のゲートは訪問回数・clone回数・コピー要素数を対象とし、時間の閾値を設けない。詳細は[移行検証報告](../superpowers/reports/2026-09-15-audit-verification-promotion.md)を参照する。

## Nested finally reanalysis (Issue #557)

The `nested-finally-reanalysis` exact gate counts actual statement-flow and
AnnAssign visits for nested `try: pass / finally` suites. It covers module,
class and function bodies at depths 1, 2, 4, 8 and 16 through 20. With the type
operator selected, one candidate remains and leaf visits are at most depth+1;
statement visits are at most (depth+1)^2+1, plus one for the class/function
wrapper. Without the type operator, both counters and the candidate count are
zero. The gate detects redundant recording walks in transfer-only finally
analysis without imposing a timing threshold.

Separate tests restore the old walk to demonstrate sensitivity, compare full
candidate descriptors/order, and preserve explicit/implicit exit routing.
These bounds apply to the single-entry fixture, not arbitrary multiple exits
or changing environments. Release observations and review results are in the
[review](../superpowers/reviews/2026-09-25-issue-557.md), with the
[design](../superpowers/specs/2026-09-25-issue-557-finally-traversal.md).
