# Issue #746・#747 の設計と検証記録

対象は `6ec69f4ac926c887003e5509bb393289f0bc9e71` を起点とする作業ブランチ `fix/issues-746-747`。
2026-10-10に提示した設計はユーザー承認済み。

## 設計

CIをworkflow全体のpath filterで停止せず、常に変更判定と集約チェックを起動する。
通常のMarkdown文書だけの変更ではRust・Lean・wheelのジョブを省略する。
実装、formal、配布入力を分類し、未知のpathと差分取得失敗では全検証を選ぶ。
監査の実行用コード、schema、fixture、vendor、lockfile、workflowを文書に含めない。
変更判定の出力を使って、必要ジョブがsuccessであることを集約チェックで確認する。
意図した省略だけを許容し、判定自体の失敗、必要ジョブのfailure・cancelled・skippedを拒否する。

PRの検証とrelease previewはworkflow名・PR番号単位のconcurrencyで古いrunをキャンセルする。
その他のイベントはrun単位で分離し、マージ後releaseの既存グループとPyPI公開を維持する。
previewは配布入力の変更時と手動実行時にビルドする。
公開イベントでPRのコードを権限付きで実行しない。

全workflowにactionlint、ShellCheck、zizmorを適用する軽量ジョブを追加する。
検査ツールはPython配布パッケージをdependency groupで固定し、uv.lockとRenovateで管理する。
秘密情報・書込権限・SARIF投稿を必要としないoffline検査とする。
例外が必要ならrule・対象箇所・理由を記録する。

## 設計セルフレビュー

1. 分類の網羅性をIssueと照合。`docs/audits/`の非Markdown、schema、vendor、設定ファイルを安全側へ分類する。
2. キャンセルの識別子をイベント別に点検。別PR、main、merge_group、公開イベントを分離する。
3. 集約の失敗条件を点検。判定成功と選択された全ジョブのsuccessを要求し、上流skipを成功にしない。
4. 権限とcheckoutを点検。previewの判定はread権限で行い、公開時のmerge commitとタグ予約を保持する。
5. rulesetを実測と照合。現リポジトリには必須チェックがなく、Pulumiは旧組織を対象とする。移管済み旧組織はユーザー指示により対象外とし、現移管先の保護設定の状態と集約チェック名を記録する。

## 実装計画

- [x] `tests/test_ci_selection.py`に分類表、実Git差分（追加・削除・rename）、差分失敗、集約の異常系を先に追加して失敗を確認する。
- [x] `tools/ci_selection.py`に分類、イベント別差分取得、GitHub出力、集約CLIを実装する。Gitの差分はNUL区切り・rename検出無効で旧pathも検証する。
- [x] `ci.yml`に変更判定と集約を追加する。qualityのRustステップと各重いジョブを選択し、nightly shuffle・fuzzの制限を保持する。
- [x] `release.yml`にread権限の判定を追加し、配布変更のpreviewだけをprepareへ進める。公開のcheckout・タグ予約・公開条件を保持する。
- [x] workflow検査用dependency group、実行CLI、CIジョブ、隔離fixtureと負例検査を追加し、既存全workflowの検出を修正する。
- [x] 既存workflow契約テストを新しい選択・concurrency・checkoutの契約へ更新し、Python全テスト、lint、型検査、ツール実行を確認する。
- [x] 開発ガイド、release手順、現移管先の必須チェック運用、OKF概念を更新する。実測と未検証事項を区別する。
- [x] 実装・テストの各5回レビュー、ディスク確認とcargo cleanを実施し、文書を含めて同一ブランチへコミットする。

## 実装計画セルフレビュー

1. 要件の対応先を点検。分類、キャンセル、preview、集約、ruleset、測定と3検査ツールの担当ファイルを確定した。
2. 実行順を点検。回帰テストの失敗を確認してから本体、workflow接続、既存契約の更新、全体検証へ進める。
3. 入出力を点検。classifierのboolean出力とJSONのjob選択を同じ判定から生成し、集約は固定job集合と照合する。
4. 境界ケースを点検。削除・rename・空差分・未知path・Git失敗・必要jobのskip・fork PRをテスト対象に追加した。
5. 証拠の限界を点検。ローカルの結果をhosted CI時間やruleset適用と同一視しない。変更前のrun IDと取得日を保存し、導入後の測定方法を記載する。

## 導入前の観測

2026-10-10にGitHub APIで取得した。
PR run `38023706140`（CI）は04:20:15–04:31:49 UTC、694秒。
Qualityは84秒、Rustは605秒、Lean auditは167秒、Fuzzは595秒。
対応するrelease preview `38023706209` は04:20:15–04:30:39 UTC、624秒。
これはSBOM実装変更の1例であり、文書変更の代表値や平均ではない。
CIは10実行job・2803job秒・queue合計28秒、previewは5実行job・1387job秒・queue合計19秒。
API取得結果を`docs/performance/ci/`へ保存した。
導入後の同種変更のhosted測定はローカルテストとは別に取得する。

`HayaoSuzuki/hoimin`のruleset `24639058` はdeletionとnon_fast_forwardのみ。
mainのbranch protection APIは404「Branch not protected」を返した。
`infra/github/Pulumi.yaml`は`tokyogas-tech/hoimin`を対象に9必須チェックとmerge queueを宣言している。
本作業で外部rulesetを自動変更しない。

作業開始時の空き容量は約190 GB。
初回の`cargo clean`は3819ファイル、631.4 MiBを削除した。


## 実装セルフレビュー

1. 分類とGit入力を点検した。schema・実行用監査コード・fixture・未知pathは全検証、renameは旧pathも含める。READMEがwheelの入力であるため、文書扱いを配布扱いへ修正した。
2. workflowの依存グラフと集約を点検した。変更判定、固定全jobの存在、boolean出力を検査し、必要jobのskip・cancelledを拒否する。previewはreserveの意図したskipで止まらない条件にした。
3. 権限と公開を点検した。prepareをread権限に分け、writeはマージ済みcommitのreserveとpublishだけに限定した。既存タグ予約とバージョンの再利用を保持した。
4. 全lintの検出を点検した。actionlintのWindows pipe停止を再現し、ShellCheckを別プロセスで全OS共通に実行する。ユーザー指示に従いShellCheckの一括除外とzizmorの重大度足切りを撤去し、informationalのjob名欠落も修正した。SBOMのPATHは使用step内だけで設定し、回避可能だった3箇所のgithub-env除外を撤去した。
5. 起動環境と最終差分を点検した。独立レビューがambient Python 3.12の構文非互換を指摘した。ユーザー方針に合わせ、判定・集約を含めsetup-python 3.14を明示する契約へ変更した。Renovateの個別ruleを汎用ruleの後へ移し、上書きを防いだ。

## テストセルフレビュー

1. 新規分類・集約テストのredを確認してから実装した。正常なdocs-onlyと必要jobのfailure・cancelled・skipped・欠落を別々に検査する。
2. 実Gitで追加・削除・rename・分岐後のmerge baseとイベント別差分を検査した。CLIのmalformed JSON、環境変数とcgroupの信頼条件も検査した。
3. 実actionlint・ShellCheck・zizmorで式不正、未定義参照、危険な入力、過大権限のfixtureを拒否することを確認した。新規`.yaml`と4KiB超scriptの負例も検査した。
4. 既存公開契約と全Pythonテストを点検した。Windowsのcp932による既存pyproject読取り失敗をUTF-8指定で修正した。厳格lint対応のjob名追加を既存契約へ反映し、公開条件・Action pinの負例は保持した。
5. 最終品質ゲート、Python 3.14設定、変異検証、文書の出典と未検証事項を照合した。独立レビューでWindowsの既定pwshにbash変数を渡す不整合が見つかり、該当stepへbashを明示し回帰テストを追加した。実行結果を下記へ記録し、hosted実測や公開をローカル成功から推定しない。

## 例外と証拠の限界

全workflowの厳格lintで検出は0件、局所除外は3箇所。
rule、箇所、理由と再現コマンドは`docs/ci.md`に記載した。
RuffのsubprocessとCLI出力に関する局所除外も、shellを使わないargv起動とCLI契約の理由を記載する。
Rustのproduction sourceは変更していない。CLIは変異検証のため現ソースからビルドした。
hosted Runnerでのafter測定、再pushキャンセル、merge queue実行は未確認。
旧組織に対するインフラ適用と本番公開は実施しない。


## 最終ローカル検証

- `uv run --frozen --no-sync pytest -q`: 847 passed、20 skipped、231.27秒。skipはOS・任意依存・外部環境の条件付き検証であり、成功へ読み替えない。
- 厳格lintの追加負例は別途実行し、informationalとSC2154の拒否を確認した。
- `ruff format --check .`、`ruff check --no-fix .`、`ty check`、全workflowのactionlint・ShellCheck・zizmorが成功した。
- `cargo build --locked -p hoimin-cli`: 現ソースのWindows CLIビルド成功（1分28秒）。Rust全テストの再実行ではない。
- 変異planは`tools/ci_selection.py`から243候補を発見。`categories_for`の`==`→`!=`（候補ID `m1_6008be571cb0e3308c4b9ae7769ef903da07cd63e76d0a504c0c85e1b5126adf`）を選び、fresh baseline成功、killed 1・survived 0・exit 0を確認した。全候補の網羅的なmutation scoreを示す結果ではない。
- 変異検証は1並列、workspace 8GiB上限、空き10GiB予約。peak owned 38,480,736 bytes、最小空き187,659,636,736 bytes。hoimin管理領域と外部temp rootの削除を確認した。
- 最終`cargo clean`は2582ファイル・2.4GiBを削除。直後のCドライブ空きは189,734,293,504 bytes（約176.7GiB）。初回・中間・ビルド後にcleanと容量確認を行った。

変更後の測定と公開の実機確認は未実施として残し、実装のローカル検証結果と区別する。

OKFは33ページの構造を検査し、更新した出典のhash・脚注・リンクと入口索引の到達を確認した。
