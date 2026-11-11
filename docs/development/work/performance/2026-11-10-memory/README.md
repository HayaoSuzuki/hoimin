# hoiminのメモリ使用量の測定

2026年11月10日、前回と同じreleaseバイナリで12ケースを各3回測定した。今回の入力範囲では、worker 1のhoimin本体の観測最大RSSは約11〜13 MiBだった。workspaceの32 MiBのファイル内容を、そのままRAMにコピーして保持するような増加は観測しなかった。一方、候補数を増やすと本体の使用量が増え、worker数を増やすと主に子プロセス分の使用量が増えた。

今回の結果から、メモリ削減を直ちに優先する根拠は見えていない。大量候補や実際のテストスイートで問題が出る場合には、解析・候補保持と、Pythonの子プロセスを分けて調べる。小規模な合成入力とリポジトリ内の実コード2ファイルだけでは、実プロジェクト全般の必要量を確定できない。

## 通常バイナリのRSS

リビジョンは `c88d581ce17ff52344a0a3399090eeaea3602f3f`、バイナリのSHA-256は `de6452194b7e13029b32a02e901de20c407deeee298a21532f0b9d87c53aab0d`。macOS 15.7.7 arm64、Rust 1.99.1、Python 3.12.6で測定した。製品ソースへの計測追加は行っていない。

macOSのlibproc APIを使い、hoimin本体と子孫プロセスのRSSを約2 ms間隔で取得した。表は、各runのサンプル最大値を求め、その3回の中央値を取った値。各プロセスの取得時刻にはずれがあり、短いピークも見落としうる。また、RSS合計では共有ページを重複して数える場合がある。したがって、表の値は同時使用物理メモリの厳密な最大値ではない。

| verifyの条件 | hoimin本体のRSS MiB | 子プロセスを含むRSS合計 MiB |
| --- | ---: | ---: |
| 追加データなし、worker 1、64件 | 11.00 | 20.59 |
| 32 MiB・1ファイル、worker 1、16件 | 10.67 | 20.22 |
| 8 MiB・512ファイル、worker 1、16件 | 13.41 | 22.44 |
| 8 MiB・128ファイル、worker 1、16件 | 12.56 | 22.22 |
| 同じ入力、worker 2 | 11.72 | 30.95 |
| 同じ入力、worker 4 | 12.52 | 51.70 |
| 同じ入力、worker 8 | 14.41 | 92.11 |

最初の3行は空のPythonコマンドを実行した。worker数の比較では、各子プロセスを30 ms待機させ、複数workerが同時に動く区間を観測した。1/2/4/8個の子プロセスをそれぞれ捕捉できた。追加データ量は選択対象の小さなPythonソースを除いた値で、workerごとのコピーはディスク上に作る。ディスクのページキャッシュの使用量は、このRSS測定で数えていない。

子プロセスの検出感度を確認するため、Pythonで64 MiBのbytearrayを確保する対照も測定した。本体の観測最大RSSは約10.11 MiBのままで、子プロセス込みの合計は約19.53 MiBから約83.50 MiBに増えた。実際のpytestなどが多くのメモリを使う場合、hoimin本体より子プロセス側の影響が大きくなりうる。

候補数の比較では、同じ加算文を繰り返したソースで全候補を `plan` 出力した。以下はサンプリング値ではなく、`wait4` から得たOS報告の最大RSSの3回の中央値。`plan` はテストプロセスを実行しない。

| 候補数 | ソースサイズ | 最大RSS MiB |
| --- | ---: | ---: |
| 1,000 | 10,000 bytes | 8.91 |
| 4,000 | 40,000 bytes | 13.03 |
| 16,000 | 160,000 bytes | 31.14 |

この比較ではソースサイズ、ASTのノード数、候補数、出力サイズが同時に増えるため、増分をすべて候補保持の費用とは断定しない。前回の[実コード測定](../2026-11-10-baseline/README.md)では、1,422候補で10.89 MiB、1,142候補で9.95 MiBだった。

`wait4` の最大RSSは、子プロセスのピークを反映する場合がある。今回も64 MiB対照で約73.78 MiBを報告したため、verifyの「hoimin本体」の値にはこれを使わず、PIDごとのRSS観測を使った。子プロセス込みのRSS合計とも区別して保存した。

## 既存テストによるヒープ計測

既存の5つのintegration testバイナリをreleaseで実行し、10テストが成功した。これらは指定した区間のRust側要求割り当て量を数えるテストであり、プロセス全体のRSSではない。

| 対象 | 今回の観測または通過した条件 |
| --- | --- |
| workspaceのpreflight・worker作成・未変更reset・変更後reset | ファイル内容1/8/32 MiBのいずれでも、区間内の追加ピークは順に83,154 / 65,950 / 131,640 / 131,640 bytes |
| preflightの総バイト数・worker数 | テスト内の入力範囲では追加ピーク87,017 bytesを維持。エントリ数を増やすと最大98,569 bytesへ増加 |
| worker作成後の保持ヒープ | 4 MiBのworkspaceに4workerを作る既存fixtureで、追加保持量2 MiB以下の条件を通過 |
| planのソースハッシュ工程 | 1/16/64 MiBのソースで追加ピーク約1.7〜1.8 KiB。解析を含むplan全体の値ではない |
| UnicodeソースのCLI discovery | 約0.5/1/2 MiBのソースで追加ピーク約3/6/12 MiB。解析側には入力に応じた増加がある |

workspaceのstream処理と、ソース解析の必要量には違いがある。今回のworkspaceテストは、コピーやresetでファイル全体を一括バッファに入れる必要がないことの証拠になる。一方、Unicodeの位置索引、AST、候補や出力のメモリが入力に比例して増えることまで排除するものではない。

[Rust Performance Book](https://nnethercote.github.io/perf-book/profiling.html)では、割り当て箇所を調べるためにDHATなどを挙げている。今回の測定はRSSと既存のヒープ計測テストまでで、割り当てのコールスタックは取得していない。大きな実入力で削減が必要になった場合に、解析・位置索引・候補保持をプロファイルする。

## 再実行と保存データ

各ケース1回のウォームアップを捨て、その後3巡、固定シードでケース順を入れ替えて測った。各CLI実行の `TMPDIR` を分離し、入力作成は時間の外で行った。メモリ測定中にビルド・別のテストは実行していない。各runで正常完了と候補数を検証した。監視には30秒のタイムアウトを設け、今回の小さな入力以外に一般的なRSS上限を保証してはいない。

```sh
cargo test --release --locked --offline -p hoimin-cli \
  --test workspace_stream_heap --test workspace_preflight_heap \
  --test workspace_heap --test plan_source_heap --test source_index_heap \
  -- --nocapture --test-threads=1
# cargo testのfeature統合によるバイナリを使わないよう、通常版を再ビルドする。
cargo build --release --locked --offline -p hoimin-cli
python3 docs/development/work/performance/2026-11-10-memory/measure.py \
  /tmp/hoimin-memory-new
```

未作成の出力先を指定する。macOSのlibproc APIを使用するため、他OSではそのまま実行できない。

- [summary.json](summary.json)と[summary.csv](summary.csv): ケース別の中央値、範囲、捕捉した子プロセス数。
- [measurements.json](measurements.json): 36回の観測と各RSSサンプル。
- [raw.tar.gz](raw.tar.gz): 全stdout/stderrと保存plan。ウォームアップも含む。
- [heap-tests.log](heap-tests.log): 10テストの実行結果とヒープ計測値。
- [measure.py](measure.py): 測定スクリプト。
- [context.json](context.json): リビジョン、SHA-256、条件、採用しなかった事前試行。
