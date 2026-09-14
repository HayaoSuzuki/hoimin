# Issue #456: 候補検証の前処理共有と検証記録

基準コミット: `165a2d284a1af92eb02ffd214ba8c0070c2f3808`。macOS arm64、Rust 1.98.0、Python 3.14.7で検証した。対象はverifyで候補descriptorを検証する前処理である。候補再発見、baselineとmutant実行の時間はこの計測に含まない。

## 再現と結果

修正前の `requested_validation_preprocesses_each_file_once` は候補2件で `contexts=2, source_bytes=40` となり、期待値1に対して失敗した。修正後は同じテストが成功した。1/2/4ファイル × 各1/2/4候補の検証でも、context構築数はファイル数、走査bytesはファイルbytes合計と一致した。候補ごとの検証とstable ID照合は省略していない。

ファイルごとに借用contextを作り、要求候補の結果を保存してソースを解放する。要求ID順に結果を取り出すことで、同一ファイルの後続エラーが別ファイルの先行エラーを追い越さない。元のsource cacheは削除した。ソースbytesと行索引は最大一ファイル分、参照と検証結果は要求候補数に比例する。これは所有権と実装構造の確認であり、RSSやallocator peakの実測値ではない。

## release計測

コマンド: `cargo test -p hoimin-cli --lib --release benchmark_requested_descriptor_preprocessing --offline -- --ignored --nocapture`。

各ファイル2 MiB、各条件3回の中央値（ms）。旧APIの行はメモリ上のソースに対する繰り返し検証。新経路は実ファイルの読み込み・要求候補のグループ化・検証を含む。比較中に別worktreeのビルドとテストも動作していたため、時間には競合による変動がある。決定的な操作回数テストを回帰ゲートとし、以下の速度比をCIの合否基準にはしない。

| ファイル数 | 各ファイル候補数 | 旧API | 共有経路（read含む） |
| ---: | ---: | ---: | ---: |
| 1 | 1 | 1.820 | 2.965 |
| 1 | 100 | 265.786 | 2.894 |
| 1 | 500 | 1201.602 | 3.859 |
| 2 | 1 | 3.700 | 4.470 |
| 2 | 100 | 490.684 | 5.885 |
| 2 | 500 | 2348.319 | 16.667 |
| 4 | 1 | 7.060 | 11.851 |
| 4 | 100 | 915.955 | 16.403 |
| 4 | 500 | 4503.027 | 68.723 |

## セルフレビュー: OKF

1. 既存の対象選択・plan・verify概念と、その出典の現行plan.rsを照合した。過去のschema/ranking表を今回の事実に書き換えず、新しい前処理の条件だけを追記する方針とした。
2. 出典を設計書・今回の報告・変更済み実装に分けた。変更済みソースのSHA-256を記録し、過去の検証済み表を今回の検査結果に流用しない。
3. 前処理回数、実行時間、ソース保持量の説明を読み直した。RSS未測定と列計算 #470 の範囲を明記し、入力全体の定数メモリ保証と誤読できる表現を除いた。形式・リンク検査は最終更新後に実施する。

## セルフレビュー: 設計

1. 受け入れ条件とソースを対応付け、context共有の対象を読み取り済みの同一ファイルに限定した。
2. パス単位に即時エラーを返す案では診断順が変わると判断し、ID順に検証結果を取り出す構造へ修正した。
3. 公開core API、schema/ranking、再発見とtimeoutを変更しないことを改訂設計と照合した。

## セルフレビュー: 実装計画

1. 受け入れ条件を回帰テスト、descriptor互換性、性能計測、文書とPRへ対応付けた。
2. 自己参照型と全ファイルbytesの長期保持を避ける構造を確認した。計数はcfg(test)の呼び出しローカル変数に限定した。
3. 独立したエラー期待値とlegacy API比較の両方を指定し、単純な実装の写しにしないことを確認した。

## セルフレビュー: 実装

1. 変更前後の検証項目を照合した。hash/span/original/line/column/operator/UTF-8/stable IDの検証と再発見照合を残した。
2. ID順とファイル順が交錯する場合を確認し、結果をキャッシュからremoveして元のID順で返す処理を検査した。未知ID・選択外パスは読み取り前に拒否する。
3. 改訂差分を読み直し、sourceのcloneやcore API変更がないこと、contextがsourceより先にdropされるスコープを確認した。前処理の補助関数を切り出し、実際の経路を直接計測できるようにした。

## セルフレビュー: テスト

1. 修正前の失敗理由が前処理の反復であることを実行結果から確認し、修正後も同じassertionを維持した。
2. ファイル数と候補数を独立に増やす表、未知ID・空集合・非UTF-8・改ざん・エラー順を点検した。legacy API比較に加えて具体的な期待エラーもassertする。
3. Clippyで検出されたusizeからu32への無条件変換を `try_from` に修正した。全体テストの初期失敗はworktree内のPythonパス欠如、Python資源監視テストの失敗はsandboxのps禁止と切り分けた。実装変更でテストを弱めていない。

## 最終検証とPR

| 検査 | 結果 |
| --- | --- |
| `cargo test -p hoimin-cli --lib plan --offline` | 32成功、2 ignored（release benchmarkは別途実行） |
| `cargo test --workspace --offline`（Python環境設定後） | 1,786成功、0失敗、14 ignored |
| `cargo test -p hoimin-core --features contracts --offline` | 301成功、0失敗 |
| `cargo test -p hoimin-cli --features contracts --offline` | 1,470成功、0失敗、14 ignored |
| workspaceとvendorのfmt | 成功 |
| workspace/all-targets/all-featuresとvendorのClippy（`-D warnings`） | 成功 |
| Python unittest discovery | 66成功。psを許可した環境で再実行 |
| `maturin build --release --offline` | wheel作成成功 |
| `HOIMIN_WHEEL=<今回のwheel> python tests/wheel_smoke.py` | 成功。uvキャッシュアクセスを許可して再実行 |
| OKF | PyYAML 6.0.3、16ページのYAML・予約ファイル・出典脚注・ローカルリンク成功 |

workspace試験には `run_e2e` も含む。上記の件数は各コマンドの結果であり、重複を除いた総テスト数ではない。Linux/Windowsの実機、CLI全体のRSS、allocator peakは今回未測定。既存Linux CIの結果はPRで別途確認する。

独立レビュー担当 `inspect_target` が基準コミットとの差分、要求ID順、キャッシュのパス同一性、bytes/contextの解放、テスト範囲を読んだ。重大な問題の指摘はなかった。比較対象の片方だけreadを含むという指摘に対して、上の計測条件と表見出しに明記済みであることを再確認した。

## セルフレビュー: PR

1. Issue #456 の受け入れ条件と差分を照合し、対象外のschema/ranking/再発見変更がないことを確認した。
2. PR本文の検証件数を実ログと照合した。Pythonパスとsandboxによる初回失敗、未実施のネイティブOS/RSS、benchmarkの比較条件を隠さない。
3. コード、テスト、設計・計画・報告、OKFの追跡対象を再点検した。worktreeのローカル `.venv` と生成wheelをcommit対象に含めず、main向けのIssue専用ブランチとして公開する。

