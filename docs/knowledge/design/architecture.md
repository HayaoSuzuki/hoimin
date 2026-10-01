---
type: Decision
title: 隔離コピーと状態遷移の設計
description: 元ソースへの変異適用を避け、状態遷移と入出力をcrateごとに分離する設計を説明する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: workspace-path-policy
  resource: ../../../README.md
  revision: a3b78d913f57ffc89edf753f6ae36bd940873f1e
  working_tree: clean
- id: workspace-path-implementation
  resource: ../../../crates/hoimin-cli/src/workspace/root.rs
  revision: a3b78d913f57ffc89edf753f6ae36bd940873f1e
  working_tree: clean
- id: workspace-path-tests
  resource: ../../../crates/hoimin-cli/tests/workspace_handler.rs
  revision: a3b78d913f57ffc89edf753f6ae36bd940873f1e
  working_tree: clean
- id: workspace-path-e2e
  resource: ../../../crates/hoimin-cli/tests/run_e2e.rs
  revision: a3b78d913f57ffc89edf753f6ae36bd940873f1e
  working_tree: clean
- id: issue-466-revalidation
  resource: ../../superpowers/reports/2026-09-14-issue-466-revalidation.md
  working_tree: untracked
  sha256: 75fd46a6a0e5fac58f369fb837d571388ddb46969b1458139caa7fbde1b3f0d1
- id: issue-466
  resource: ../../superpowers/specs/2026-09-14-issue-466-retired-blank-line-discovery-design.md
  revision: 8b33167a049e3cae0fc05e96ccf2253c660b7023
  working_tree: modified
  sha256: dfa895dc4f91c1b8a912112008488426525d26376d7e09d397a6f65cbae8d8db

- id: issue-477
  resource: ../../superpowers/specs/2026-09-11-issue-477-import-roots-design.md
  working_tree: untracked
  sha256: a30ed5f129c635e75431212d590171ab03e5973640a4d6624775ee43fd81dcdd

- id: issue-465
  resource: ../../superpowers/specs/2026-09-11-issue-465-retired-discovery-design.md
  working_tree: untracked
  sha256: 7efd3035a37d38c95404f12774387450f94dc0be0a69af1da905056df54cedf6

- id: issue-462
  resource: ../../superpowers/specs/2026-09-11-issue-462-retired-reader-design.md
  working_tree: untracked
  sha256: d60d84425e841eeca29a40a71a9b73a18be18a43e0a306b149add1f7c029b2b0
- id: initial
  resource: ../../superpowers/specs/2026-07-18-python-mutation-tool-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: migration
  resource: ../../superpowers/specs/2026-07-19-remove-python-libcst-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: core
  resource: ../../../crates/hoimin-core/Cargo.toml
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: machine
  resource: ../../../crates/hoimin-core/src/machine.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: analyzer
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: issue-484
  resource: ../../superpowers/specs/2026-09-11-issue-484-metrics-destinations-design.md
  working_tree: untracked
  sha256: b146daf296d69374ad01ac86d5a5becd30b721b1983d5a54507cd0f1750ef3e7
---

# 作業コピーへの変異適用

hoiminの初期設計では、一時領域に作ったテスト実行用のコピー（worker）へソースの変異を適用する方式を採用した。元の作業ツリーを書き換えて後で戻す方式では、強制終了によって復元処理が動かず、変異したソースが残るおそれがある。コピーを使う方式は、この復元処理への依存を避けるための判断だった。[^initial]

別案として、実行時に選べる複数の変異を一つのソースへ埋め込む方式も検討した。こちらは例外発生時の呼出し履歴やモジュール実行の意味への影響が課題となり、初版の対象外とした。[^initial]

# 作業コピーのファイル名

Linux/macOSの作業コピーでは、ファイル名とディレクトリ名に `:` を許可する。従来は変異候補と共通のパス検査を使っていたため、`.dockerfiles/appconfig/app:env:conf-sample` のようなテスト対象外のファイルでも、コピー準備中に拒否してbaseline前に停止していた。コミット `a3b78d9` で作業コピー用の検査を分離した。Windowsではドライブ指定や代替データストリームとしての解釈を防ぐため、引き続き `:` を拒否する。空要素、`.`、`..`、絶対パス、バックスラッシュ、NULも作業コピーのパスとして拒否する。[^workspace-path-policy][^workspace-path-implementation]

許可されたファイルはコピー、workerの復元、原本変更検出の対象となる。テストに不要なら `--exclude '.dockerfiles/**'` で明示的に除外できる。変異候補とfingerprint入力には、従来のOS共通のパス制約を適用する。[^workspace-path-policy][^workspace-path-tests]

2026-10-01にmacOSで、追加した回帰テスト3件の修正前の失敗と修正後の成功を確認した。対象は `colon_paths_are_copied_restored_and_checked_for_original_changes`、`colon_paths_can_be_explicitly_excluded_from_copy_and_integrity_checks`、`baseline_and_mutants_can_read_colon_named_workspace_fixtures` である。コピー・復元・原本変更検出・明示除外と、実際のCLI経由でのbaselineおよび変異テストからのファイル読取りを確認した。LinuxとWindowsでは今回実行していない。パス検査やOS別のファイル操作を変更した場合は、この境界と回帰テストを再確認する。[^workspace-path-tests][^workspace-path-e2e]

# 状態遷移と入出力の分離

制御を担当する `hoimin-core` は、状態と完了通知（event）を受け取り、次の状態と実行要求（effect）を返す。入出力を担当する `hoimin-cli` はその要求を実行し、完了通知をcoreへ返す。この分離によって、状態遷移の判断をファイル操作やプロセス起動から独立して扱う。[^initial]

現在の実装にも状態遷移関数 `transition` があり、coreの通常依存にはTokio・SQLite・OS API用のcrateを置いていない。これはカタログ作成時にコードと依存宣言を読んで確認した結果である。[^core][^machine]

| 構成要素 | 担当する処理 |
| --- | --- |
| core | 状態遷移、実行要求の識別、結果分類、予算と終了判断 |
| target / analyzer | ファイルシステム・Gitからの対象選択、Python構文からの候補生成 |
| workspace / process | コピー、指定バイト範囲の変更、テスト起動、復元、資源制御 |
| report / session | 結果出力、SQLite保存、互換性の判定と再利用 |

この表は初期設計に記された分担を示す。実行順序や例外処理の正しさは、各処理に対応する試験・監査で確認する必要がある。[^initial]

# Rust解析器への移行

2026-07-18の初期設計にはLibCSTヘルパーが登場する。その翌日の移行設計では、解析専用のPythonコードとLibCSTを撤去する方針を定めた。現在の解析器では、Rust内でRuffの構文解析器と抽象構文木（AST）を使う。[^migration][^analyzer]

したがって、初期設計にあるヘルパー起動や版取得は現行手順として使えない。利用者が指定するテストコマンドでPythonを起動することと、hoiminの解析器がPythonに依存することも区別する。[^migration]

# 廃止したRust関数探索

Issue #465 の関数取りこぼしは、Rustの関数を正規表現で列挙してcargo-mutantsの候補を絞るPython製開発ツールの問題だった。コミット `2f27e2a` で探索処理と候補フィルタを含む連携全体が削除されている。今回、その削除が作業対象に含まれること、現行の呼出し元が残っていないこと、開発スキルの契約テスト3件の成功を確認した。既存の機能削除による解消であり、新しいRust構文解析器を実装したものではない。Pythonソースを解析する現行hoiminのRust実装とは対象が異なる。[^issue-465]

Issue #466 の空行数に対する処理時間増加も、同じ開発ツールの正規表現によるRust関数探索で発生していた。履歴上の `_FUNCTION` は改行を含む `\s` で始まり、ファイル全体への `findall` が候補数の検査より先に完了する構造だった。コミット `2f27e2a` はこの探索処理、コマンド入口、補助パッケージを削除している。現行ツリーで削除コミットの包含、対象ファイルと呼出し元の不在、cargo-mutantsを実行しない開発方針を確認した。これは既存の削除による解消であり、正規表現の修正版や新しい性能測定ではない。将来Rust mutation discoveryを再導入する場合は、構文の対応範囲、時間予算の確認点、固定時間閾値に依存しない入力規模試験を改めて設計する。[^issue-466]

# 廃止した開発ツールの不具合

Issue #462 のFIFO読取り停止は、Pythonで実装されていたRust mutation testing用の開発ツールに関するものだった。コミット `2f27e2a` で読み取り関数・探索処理・コマンド入口が削除されており、現在の開発手順もcargo-mutantsを実行しない方針である。今回、削除コミットが作業対象に含まれること、現行ツリーに呼出し元が残っていないこと、現行スキルの契約テスト3件の成功を確認した。これは既存の機能削除による解消であり、現行Rust実装全体のFIFO安全性を示す検証ではない。[^issue-462]

# コピー方式で防げる変更の範囲

コピー方式で避けるのは、元ソースへ変異を適用する操作である。任意の出力先や子プロセスによる変更まで防ぐ設計とは読み取れない。[境界監査](../audits/boundary-2026-09.md)では、metrics出力先と元ソースが重なる場合を別の問題として記録している。

Issue484の設計では、選択済みソースや明示fingerprint入力、使用中のsessionとmetrics出力先が衝突する場合をbaseline前に拒否する。renameで置き換えるディレクトリエントリを比較し、別のhardlinkやsymlinkの参照先を同一視しない。衝突を拒否した後の最終出力も保護の対象とする。[^issue-484]

保存先の同一性を確定できない場合は、実行結果を維持してmetricsの保存を見送り、終了時に `metrics.write` で通知する。未作成の親ディレクトリや対応範囲外の別名もこの扱いとし、既知の衝突をbaseline前に拒否する場合と区別する。[^issue-484]

crateの依存、実行要求と完了通知、コピーへの変異適用、解析器の起動方法を変更したら、このページを見直す。終了処理の条件は[状態・終了処理](runtime-lifecycle.md)、検証の根拠は[Leanの証拠範囲](../audits/lean-evidence.md)で確認できる。

# 変異対象とimport rootの分離

Issue 477では、path-only `.pth` に登録した元のsrcディレクトリがworkerより先にimportされる問題を扱う。`--import-root src` はworkerのimport探索先を明示し、`--file`・`--line` の候補範囲を広げない。worker root、明示したimport root、source root、継承PYTHONPATHの順序を保つ。指定ディレクトリがコピーに存在しない場合はbaseline前に拒否する。正規パッケージのpath-only `.pth` を検証対象とし、独自finderや環境変数を無視するPython起動まで保証しない。[^issue-477]

[^initial]: [2026-07-18-python-mutation-tool-design.md](../../superpowers/specs/2026-07-18-python-mutation-tool-design.md)。
[^workspace-path-policy]: [README.md](../../../README.md)。
[^workspace-path-implementation]: [workspace/root.rs](../../../crates/hoimin-cli/src/workspace/root.rs)。
[^workspace-path-tests]: [workspace_handler.rs](../../../crates/hoimin-cli/tests/workspace_handler.rs)。
[^workspace-path-e2e]: [run_e2e.rs](../../../crates/hoimin-cli/tests/run_e2e.rs)。
[^migration]: [2026-07-19-remove-python-libcst-design.md](../../superpowers/specs/2026-07-19-remove-python-libcst-design.md)。
[^core]: [Cargo.toml](../../../crates/hoimin-core/Cargo.toml)。
[^machine]: [machine.rs](../../../crates/hoimin-core/src/machine.rs)。
[^analyzer]: [rust.rs](../../../crates/hoimin-cli/src/analyzer/rust.rs)。

[^issue-484]: [2026-09-11-issue-484-metrics-destinations-design.md](../../superpowers/specs/2026-09-11-issue-484-metrics-destinations-design.md)。

[^issue-477]: [2026-09-11-issue-477-import-roots-design.md](../../superpowers/specs/2026-09-11-issue-477-import-roots-design.md)。

[^issue-465]: [2026-09-11-issue-465-retired-discovery-design.md](../../superpowers/specs/2026-09-11-issue-465-retired-discovery-design.md)。

[^issue-466]: [2026-09-14-issue-466-retired-blank-line-discovery-design.md](../../superpowers/specs/2026-09-14-issue-466-retired-blank-line-discovery-design.md)。

[^issue-462]: [2026-09-11-issue-462-retired-reader-design.md](../../superpowers/specs/2026-09-11-issue-462-retired-reader-design.md)。

# Issue #466 の再確認（2026-09-14）

基準コミット `8b33167` で削除と先行PR #527の包含、現在の呼出し元の不在を再確認した。受け入れ条件4項目は連携の廃止により対象外となる。`tools/performance_shapes.py` は現在存在するため、ディレクトリ全体の不在を判定条件にしない。今回の確認は構文互換性や性能改善の実測を保証しない。[^issue-466-revalidation]

[^issue-466-revalidation]: [Issue 466 retirement revalidation and self-review](../../superpowers/reports/2026-09-14-issue-466-revalidation.md)。
