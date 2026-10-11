# #711 survivor評価の実装と検証

2026-10-11。設計元mainは `d46f13a`、実装ブランチは `feat/issue-711-survivor-review`。
[承認済み設計](../specs/2026-10-11-issue-711-survivor-review-design.md) と
[実装計画](../plans/2026-10-11-issue-711-survivor-review.md) に対応する。

## 実装した範囲

保存planを参照する `review list/show/record` を追加した。
四種類の人の評価と理由・参照文字列を、独立SQLite sidecarへrevision付き履歴として保存する。
project、ID、original・symbolを含む完全descriptorを照合し、source変更時はstale、読取り不能時はsource_unavailableとして現評価から除く。
別checkoutや変更後の候補へ評価を自動移行しない。
recordはexpected revisionを必須とし、更新競合を拒否する。
list/showは存在しないstoreを作らない。

任意の完了v4 JSON reportを検証して実行statusを併記できる。
plan単独はnot_loaded、report内にない候補はnot_in_reportであり、survivorと推測しない。
人の評価は元plan/report、実行status、kill率、ranking、progress判断を変更しない。
外部manifestの取込み、URL取得、学習、候補抑制は追加していない。
配布物に機能が入るため、workspace/Python/両lockの版floorを0.4.0へ揃えた。

## 保存と入力境界

既存rusqlite依存を使い、独立schema v1、BEGIN IMMEDIATE、revision照合、履歴追加を一つのtransactionで処理する。
schema・project・整合性を検査し、既存の不明DBや壊れたDBを初期化しない。
同時初期化は新規ファイルを排他的に作ったprocessだけが行い、他方は完成schemaを待って照合する。
writerの待機は1秒までで、更新失敗はrollbackする。

plan/reportは各64 MiB、storeは16 MiB、履歴は全体1万件までとする。
sourceも64 MiBまでのsnapshotとして検査し、超過は利用不能とする。
SQLite journal等の追加ディスク領域まで16 MiBに収まる保証ではない。
store本体だけでなくjournal/wal/shmの別名が保護対象入力に衝突する場合も拒否する。
reason/evidenceは長さと制御文字を検査し、参照hashも保存・再読取り時に検査する。
text表示はterminal controlをescapeする。
同一ユーザーによる敵対的な同時ファイル差替え、ネットワークファイルシステム、root移動の自動復旧は保証対象外である。

## 形式証明の範囲

Leanで次を証明した。

1. 評価の適用成功にはproject・ID・descriptorの完全一致とfresh sourceが必要。
2. 更新拒否時はrevisionと履歴を含むモデル状態全体が変わらない。
3. 更新成功時は旧履歴がprefixとして残り、revisionが1増える。

モデルではdescriptor全11項目をtoken列に抽象化する。
historyとrevisionの対応は、Rustの実際のhistory末尾から得るrevisionに対して差分検査する。
14種類の同一性差分×3鮮度×4 revision条件の168ケースをLeanから生成し、Rustの純粋処理と比較した。
全identity項目の不一致と古いrevisionについて、guardを省いた判定との差を検出する感度検査も行った。
corpusはLeanが生成し、手書きしていない。

ローカルでは既存Windows Lean 4.34.1を使用し、単一Lean processを30秒・2 GiBの監視下で実行した。
model 1.48秒/約273 MiB、proofs 0.97秒/約265 MiB、cases 1.05秒/約298 MiB、生成・鮮度・感度は各1秒未満で成功した。
最初の不要な `import Lean` は30秒制限を超えたため、純粋モデルに必要な `import Init` に限定した。
CIの正本は既存のLean 4.32.2とPOSIX process-tree guardで、30秒・2 GiB・逐次実行を維持する。
Windowsの確認を、Linux/pinned版での検証成功と置き換えない。

証明はモデルに関するもので、Rust全体、SQLite/OS永続化、hash衝突耐性の形式検証ではない。
実行結果への非干渉はモジュール分離と実装テストで確認した。

## 検証の記録

- 変更前CLI unit: sandboxではACL操作により失敗、同一binaryを制限外で実行して805成功・12 ignored。
- review unit: 16成功。候補誤帰属、source鮮度、report不整合、保存履歴、競合、rollback、容量、journal aliasを含む。
- CLI統合: 2成功。record/list/show、revision競合、stale、保存後の出力失敗と再試行時の非重複を含む。
- 共有reader変更後の既存CLI unit: 819成功・12 ignored。
- progress input/decisionの既存Lean oracle: 4件+5件成功。
- review Lean oracle: 168ケースを比較する1テスト成功。
- JSON schema: 実際のlist/record出力を専用schema v1で検証し成功。
- workflow契約: 全体167件のうち166件成功後、残ったgenerator順序を修正。失敗に関係する2件の再実行が成功し、全対象を確認した。
- release/PyPI契約: 69成功・10 skipped。
- 厳格Clippy: workspace/all-targets/all-featuresで成功。新しいlint除外は追加していない。

最終Rust workspaceテストは151バイナリ/doc-test集合で2725成功・23 ignored・失敗0だった。
生成CLIリファレンスの一致検査と生成器テスト、cargo fmt、最終Clippy、変更PythonのRuffと全体ty checkも成功した。
Python全体は916成功・29 skipped・Git Bash harnessのtimeout 2件だった。
workflow単独再実行では166成功・timeout 1件となり、全moduleを逐次起動するWindows harnessの60秒期限が不足した。
このテスト期限だけ120秒へ変更し、実CIのLean 30秒・2 GiB制限は維持した。
修正後の対象テストは66.31秒で成功した（旧60秒を超える）。Python全体の再実行は進行中。

## 実装セルフレビュー（5回）

1. 同定: plan header/ID再計算、source record、root内解決、完全descriptor照合を読み直した。rank/sequenceをreview keyへ含めず、同じIDのoriginal変更を拒否することを確認した。
2. 保存: project束縛、初期化所有者、read snapshot、候補ごとのrevision、BEGIN IMMEDIATEからcommit/rollbackまでを読み直した。同時更新の片方だけが保存され、履歴が消えないテストを確認した。
3. 入力: DB schemaとSQL、容量、参照文字列、destination aliasを読み直した。journal別名衝突と不正artifact hashの検査不足を発見し、両方のRED→GREEN回帰テストで修正した。
4. 非干渉: CLI dispatchからreviewの呼出しだけが増え、executor/ranking/summarize/progressにstoreを渡していないことを確認した。実際のreport bytesとprogress結果が評価前後で同じになるテストを確認した。
5. 証明対応: Rustの拒否条件順序、u64上限、descriptor全項目、履歴prefixをLeanモデルと照合した。ローカル4.34.1とCI 4.32.2の違いを記録し、モデル証明をDB/OSの保証に拡大しないことを確認した。

## テストセルフレビュー（5回）

1. 期待値: 未評価≠not_useful、not_loaded≠survived、missing/staleの区別を確認した。候補照合、revision、容量上限には手で確認できる具体値を使い、期待値を対象関数で生成していない。
2. 競合: barrierで2 writerの同時初期化・同candidate更新を起こし、成功1件と保存履歴1件を確認した。別candidateの独立保存も2 connectionで確認した。
3. 障害: commit直前の失敗注入、busy、未知版、破損、容量、stdout失敗後の再試行を確認した。実ディスク故障や電源断を再現した検証とは扱わない。
4. 集計: stale評価の除外、実行statusの別表示、report bytesとprogress comparisonの不変を確認した。既存readerのsummary検証を省略していないことも確認した。
5. 証拠: Lean生成・freshness・sensitivityとRust比較の成功を分けて記録した。sandbox ACL失敗、Windows skip、未確認のLinux/pinned版を成功件数へ混ぜないよう検査した。

設計の5回、計画の5回、実装の5回、テストの5回は別々の記録である。
Rust buildの段階間でcargo cleanを実施し、Task 3終了後には5.7 GiBを解放した。

## 計画からの判断

- 承認済み作業branch/checkoutを継続し、別worktreeは作らなかった。gh stackの構成を維持するためで、並列checkoutによる隔離はない。
- `review::run` は引数を借用する形とした。設定を消費しない処理であり、厳格lintに従った内部呼出し変更である。
- 保存テストを `store_tests.rs` に分けた。transaction fixtureと入力fixtureの責務を分けるためである。
- source snapshotにも64 MiB上限を設けた。入力読取りで無制限のallocationを避けるためで、超過sourceは評価登録できない。
- WSLにLeanがなかったため、既存Windows Leanで局所検証した。pinned版互換性はLinux CIで引き続き確認する。

## 主なコード

- [review module](../../../crates/hoimin-cli/src/review/mod.rs)
- [純粋判定](../../../crates/hoimin-cli/src/review/model.rs)
- [入力検証](../../../crates/hoimin-cli/src/review/input.rs)
- [保存](../../../crates/hoimin-cli/src/review/store.rs)
- [表示](../../../crates/hoimin-cli/src/review/render.rs)
- [Lean証明](../../../formal/HoiminOracle/HoiminOracle/SurvivorReviewProofs.lean)
- [Rust比較](../../../crates/hoimin-cli/tests/lean_survivor_review_oracle.rs)
