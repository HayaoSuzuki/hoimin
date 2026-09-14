# 入力形状別の性能検証

[shapes.json](shapes.json) は、8次元・11形状の入力、N/2N/4Nのサイズ、測定対象、実行可能な回帰テストを記録する台帳である。`growth_model` は入力の構造、`expected_after_fix` は依存Issueが統合された後の期待値を示す。実測の記録は、それぞれに明記したrevisionの観測として読む。

| 次元 | release入力 | 決定的な通常ゲートとの対応 |
| --- | --- | --- |
| 対象発見 | `--file case.py` を固定し無関係なsubtreeのファイル数を増やす | #453の走査数、#474のselector照会、#475の行範囲統合をactive登録 |
| fingerprint | 小さなconfig1ファイルに重複globをN件指定 | #457のwalk回数をactive登録 |
| ソース配置 | 同じbytes・候補を長い1行と多数行へ配置 | #470の列照会をactive登録。既存token範囲照会をactive登録 |
| AST | 未選択collectionの深さを増やし、選択した加算を内部に置く | #461のreplacement生成数をactive登録。既存prefix/overflowをactive登録 |
| 解析状態 | import幅×annotation数、再代入×呼出数。両方とも候補0 | #479/#482の操作数をactive登録 |
| verify | 同じファイルの候補数とtop1/topN | #456の前処理回数をactive登録 |
| 出力 | mutant record数 | 既存report/progressのallocator peakをactive登録。#463のwrite数をactive登録 |
| workspace | 1 workerへコピーするファイル数とbytes | 既存の作成後retained heapをactive登録。preflight peak/RSSとは区別 |

現在のブランチは21個のactiveゲートを実行する。これで全入力形状の性能回帰を網羅するわけではない。追加の修正が未導入の場合はpendingとして登録し、成功数へ含めない。修正を取り込んだ後に実際のテスト名と計数箇所を照合し、`args` を指定して実行してからactiveへ変更する。単にラベルを変えて検証済みにしない。

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

台帳の `lean` から既存のモデル・証明・生成corpus・Rust adapterをたどれる。生成器のbuild、`--sensitivity`、`--check` をresource guard付きで順番に実行した後、`lean-target-reads` gateで実装の対象読取り数を照合する。caseはLeanで定義し、JSONLを手編集しない。既存の0/1/上限/overflowケースはprefixと順序を検証し、CLI全体の定数メモリやnative stackを保証しない。新しいbinding索引やwrite回数のモデル対応は、各観測点と表現上限を確認して追加する作業として残る。

## 個別修正を組み合わせた確認

[2026-09-14の統合確認](2026-09-14-integration-check.md) に、10件のPRのRust差分を組み合わせたrevision、競合解消、全workspace試験の結果を記録した。依存修正を取り込んだ統合時には21ゲートを実行してactiveへ昇格した。mainへのマージは各PRのCI成功後に順次行う。
