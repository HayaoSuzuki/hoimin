# プロセス実行とワークスペース処理の時間内訳

2026年11月10日、[基準測定](../2026-11-10-baseline/README.md)と同じ `c88d581ce17ff52344a0a3399090eeaea3602f3f` を使い、`verify` の工程時間を測定した。小さなworkspaceで空のPythonコマンドを実行すると、1プロセスあたりspawnに約0.8 ms、その後の終了待機に約13.7 ms、1回のresetに約0.3 msかかった。同じ8 MiBでも512ファイルに分けるとresetは約30.5 msになり、ワークスペース処理の割合が増えた。

今回の入力では、調査対象をファイル数で分ける必要がある。小さなworkspaceではPythonプロセスの起動・終了が中心となり、ファイル数が多いworkspaceではreset、コピー、cleanupの費用が増える。製品コードと通常バイナリは変更していない。

## 測定方法

macOS 15.7.7 arm64、Rust 1.99.1、Python 3.12.6で測定した。通常バイナリに加え、`git archive HEAD` から展開した一時コピーに [計測パッチ](instrumentation.patch)を適用し、同じrelease設定でビルドした。パッチは `Instant` の経過時間をメモリに記録し、コマンド終了前にstderrへ一括出力する。各イベントでファイルへ書き込む処理は追加していない。

10ケースについて、通常版・計測版を各1回ウォームアップした後、各7回、計140回測定した。ケース順を固定シードで変え、各ペアのバイナリ順も交互に入れ替えた。両バイナリで既存の `--metrics` を有効にし、同じ保存済みplanを使った。各runの完了、全mutantのsurvived、プロセス数、計測イベント数を検証した。

workerは1、mutant数は16または64。テストコマンドはPythonの `-c pass` と `/usr/bin/true` を用いた。データファイルは選択対象のPythonファイルとは別に配置し、合計バイト数とファイル数を変えた。入力作成・plan生成・ビルドは時間に含めていない。各CLI実行の `TMPDIR` は新しいディレクトリとし、過去のworkspaceのjanitor走査が比較に混ざらないようにした。その削除も測定の外で行った。

総時間は `perf_counter_ns` でプロセス起動から終了待機までを測った。前回の `ps` 監視と `/usr/bin/time` は挟んでいない。ウォームキャッシュで測り、他のアプリケーションからは隔離していない。今回と前回では監視・metrics・TMPDIRの条件が異なるため、総時間の差を改善率として扱わない。

計測版と通常版のペア差の中央値は、ケースによって−7.7〜+14.1 msだった。各ケースのペア差の最小〜最大範囲は0をまたぎ、32 MiBのtrueケースでは−138.1 msの差も観測した。計測による負荷やコード配置の影響を排除した値ではないため、以下は工程の優先順位を調べる観測として扱う。小さな改善の判定には通常版同士の再比較が必要になる。

## プロセスのspawnと終了待機

小さなworkspace、空のPythonコマンド、mutant16件のケースでは、baselineを含めて17プロセスを実行した。以下は各runの合計時間を呼び出し回数で割り、その7回の中央値を取った値であり、全呼び出しをまとめた中央値ではない。

| 計測区間 | 1プロセスあたり ms |
| --- | ---: |
| プロセス要求の処理全体 | 14.671 |
| argv・出力先・resource supervisorの準備 | 0.029 |
| `command.spawn()` | 0.841 |
| 子プロセスの終了待機 | 13.716 |
| 終了後のsupervisor破棄待ち | 0.013 |
| 終了後の出力回収待ち | 0.003 |

処理全体には、表に独立して示していないsupervisorのattach、pipeの設定、終了状態の分類なども含む。区間ごとに独立して中央値を取るため、内訳の和は全体の中央値と一致するとは限らない。

終了待機は、子プロセスの実行やスケジューリングを含む経過時間であり、hoiminがCPUを使っていた時間ではない。stdout/stderrのdrainとspool出力も待機中に並行して進む。表の「終了後の出力回収待ち」は、その並行処理を含む全I/O費用ではなく、子プロセス終了後に残った待機時間を表す。

mutant64件、65プロセスでも、spawnは平均相当で1プロセスあたり0.850 ms、終了待機は13.594 msだった。プロセス数を増やしても、この入力範囲での1回あたりの費用は近い。

対照として、hoiminを通さず同じPythonとtrueを各31回直接起動した。各3回のウォームアップを除外している。

| 直接起動したコマンド | 起動から終了まで ms | Popenが戻るまで ms | その後の待機 ms |
| --- | ---: | ---: | ---: |
| Python 3.12.6 `-c pass` | 14.112 | 0.988 | 13.160 |
| `/usr/bin/true` | 2.944 | 1.019 | 1.963 |

Pythonは実質的なテストを行わなくても、直接起動だけで約14 msを要した。hoimin内で観測した約14.7 msの大半は、このPythonの起動・終了だけの対照値に近い。直接起動とhoimin内ではcwd、環境変数、起動方法が異なるため、差分すべてをhoimin固有のオーバーヘッドとは断定しない。

## ワークスペースの準備とreset

以下はすべてtrueを使うmutant16件のケース。baseline後の復元とmutant後の復元を合わせて17回resetした。データ量は追加ファイルの合計で、選択対象 `case.py` の160 bytesを含まない。時間はmsで、各runの工程合計の中央値を示す。

| 追加データ | preflight | worker作成 | reset合計 17回 | reset 1回あたり | 元ファイル再検証の合計 | cleanup工程 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| なし | 0.851 | 0.326 | 4.161 | 0.245 | 0.432 | 9 |
| 8 MiB、1ファイル | 10.928 | 2.170 | 25.113 | 1.477 | 8.591 | 10 |
| 32 MiB、1ファイル | 38.526 | 7.700 | 85.516 | 5.030 | 32.317 | 12 |
| 8 MiB、128ファイル | 29.686 | 17.051 | 149.455 | 8.791 | 15.912 | 51 |
| 8 MiB、512ファイル | 89.177 | 76.337 | 518.664 | 30.510 | 36.895 | 186 |

preflightは入力のinventory、snapshot作成、validation、元ソースの再読込を含む。8 MiB・1ファイルではsnapshot作成が6.285 ms、preflight内のソース再検証が4.440 msだった。32 MiBではそれぞれ22.000 ms、16.207 msとなった。これらはpreflightの内数であり、表のpreflightに重ねて加算しない。「元ファイル再検証の合計」はpreflight外の `WorkspaceTask::Verify` の時間で、別の計測区間となる。

worker作成は `WorkspaceTask::Create`、resetは `WorkspaceTask::Reset` の実行時間で、blocking taskをキューで待つ時間を含まない。cleanupは既存metricsの終了処理工程の値で、ディレクトリ削除だけを囲った計測ではない。また、この値だけ整数msで記録する。

総量を8 MiBに固定したまま1ファイルから512ファイルに増やすと、resetの合計は約25 msから約519 msへ増えた。worker作成とcleanupでも同じ傾向を観測した。データの総バイト数に加えて、ファイル単位の処理を次のプロファイル対象にする根拠となる。

Pythonコマンドを使った場合も、追加データなしではreset1回あたり0.301 ms、8 MiB・1ファイルでは1.610 ms、32 MiB・1ファイルでは5.188 msだった。小さいworkspaceではプロセス実行に比べてresetが短いが、この関係を大きなworkspaceに一般化できない。

## 通常版と計測版の総時間

各7回の中央値。計測版には記録とstderrへの一括出力の費用も含む。通常版の値は、今回の条件での比較用の基準値となる。

| ケース | 通常版 ms | 計測版 ms |
| --- | ---: | ---: |
| データなし、Python、16件 | 315.60 | 323.30 |
| データなし、Python、64件 | 1,072.55 | 1,093.20 |
| データなし、true、16件 | 110.93 | 115.86 |
| データなし、true、64件 | 286.40 | 285.60 |
| 8 MiB・1ファイル、Python、16件 | 366.34 | 367.11 |
| 32 MiB・1ファイル、Python、16件 | 489.03 | 504.00 |
| 8 MiB・1ファイル、true、16件 | 165.40 | 164.53 |
| 32 MiB・1ファイル、true、16件 | 304.80 | 293.76 |
| 8 MiB・128ファイル、true、16件 | 382.02 | 388.79 |
| 8 MiB・512ファイル、true、16件 | 1,077.93 | 1,081.53 |

総時間には保存planの読込・再解析、CLI起動、metricsの書き出しなども含む。表に挙げたworkspaceとprocessの区間だけで総時間を分解しきっていない。今後は、多数ファイルでのresetに含まれる列挙・open・比較・復元の内訳と、cleanup工程を追加計測する。Pythonの起動費用については実際のテストコマンドでも割合を確かめる。今回はテスト用の生成物、複数worker、冷たいキャッシュ、他OSは測定していない。

## 保存データと再実行

- [context.json](context.json): リビジョン、バイナリ・成果物のSHA-256、条件とコマンド。
- [measurements.json](measurements.json): 通常版・計測版140回と直接起動62回の観測。計測版の各イベントのns値を含む。
- [summary.json](summary.json)と[summary.csv](summary.csv): 工程別の中央値、通常版とのペア差、範囲。
- [raw.tar.gz](raw.tar.gz): 全stdout/stderr、保存plan、既存metrics。ウォームアップも含む。
- [instrumentation.patch](instrumentation.patch): 一時ソースにだけ適用した計測変更。
- [measure.py](measure.py): 入力作成、比較実行、観測検証のスクリプト。

リポジトリルートから、未作成の一時ディレクトリを指定して再実行する。

```sh
mkdir -p /tmp/hoimin-phase-new/source
git archive c88d581ce17ff52344a0a3399090eeaea3602f3f | tar -x -C /tmp/hoimin-phase-new/source
patch -p1 -d /tmp/hoimin-phase-new/source < docs/development/work/performance/2026-11-10-stage-breakdown/instrumentation.patch
cargo build --release --locked --offline -p hoimin-cli \
  --manifest-path /tmp/hoimin-phase-new/source/Cargo.toml \
  --target-dir /tmp/hoimin-phase-new/target
python3 docs/development/work/performance/2026-11-10-stage-breakdown/measure.py \
  --baseline "$PWD/target/release/hoimin" \
  --probe /tmp/hoimin-phase-new/target/release/hoimin \
  --output /tmp/hoimin-phase-new/measurements --repeats 7
```

通常バイナリも同じリビジョンからビルドしたものを指定する。今回のSHA-256は `context.json` に記録した。macOS上で、hoiminのresource supervisorを実行できる環境を使う。計測パッチは製品へ組み込む変更ではないため、全テストスイートは実行せず、releaseビルドと160回のCLI実行（ウォームアップを含む）で計測イベントと完了結果を検証した。
