# 監査資料

監査時の報告、再現コード、Leanモデル、生成ケース、実行ログを保存する。2026-09-14の4組は `f11013542ccd735ab9741b5079c0b39a517df256` を対象とした記録である。そこで見つかった #545〜#549 は2026-09-15に修正をマージした。各資料の失敗件数は修正前の観測を示す。

## 監査の一覧

| 資料 | 対象と結果 | 現在の検証先 |
| --- | --- | --- |
| [2026年7月 Rust監査](2026-07-rust-codebase/README.md) | 初期のコード監査と対応Issue | [当時の対応一覧](2026-07-rust-codebase/issues.md) |
| [統合後のLean監査・性能調査](2026-09-14-post-integration/README.md) | 暗黙例外、入れ子ループ、import状態コピー。#545〜#547を発見 | [暗黙例外](../knowledge/audits/implicit-finally.md)、[性能検証](../knowledge/audits/performance-shapes.md) |
| [型注釈の具体型名](2026-09-14-additional/README.md) | module再代入と型パラメータ。#548を発見 | [型注釈の参照先](../knowledge/design/annotation-builtins.md) |
| [多次元スライス](2026-09-14-slice-tuples/README.md) | tuple内のSliceをリスト式と誤認。#549を発見 | [修正・検証報告](../superpowers/reports/2026-09-15-issue-549-slice-tuple.md) |
| [plan／verify／progress追加調査](2026-09-14-followup/README.md) | 既存Rust・Lean検証。独立した新規不具合は未確認 | [追加調査の範囲](2026-09-14-followup/README.md#対応関係と限界) |

[監査結果と正式な回帰検証の対応](../knowledge/audits/analysis-2026-09.md)に、修正PR、テスト、モデルの参照先をまとめた。マージ前の全5件の検証は[統合検証報告](../superpowers/reports/2026-09-15-issues-545-549-integration.md)を参照する。

## 再実行と保存済みの結果

各ディレクトリのJSON・JSONL・ログは監査当時の証拠である。`corpus.jsonl`は各監査のLeanモデルが生成した入力であり、現在の正式コーパスとは別に保存する。履歴を確認するときは報告の対象コミット、binary SHA-256、実行環境を合わせて読む。

現在の回帰検証には `crates/` のテストと `formal/HoiminOracle/` のモデル・コーパスを使う。監査用の `replay.py` は比較結果をJSONに保存するが、不一致件数をCIの失敗終了へ変換しない。正式なCIゲートとしてそのまま追加しない。

監査を再実行する場合は新しい出力先を指定し、結果の対象コミットとバイナリを記録する。スライス監査の `run_repro.py` は保存済みの `run.json` を上書きするため、歴史的な観測を維持するには一時checkoutで実行する。Lean検証には各README記載の資源制限を用いる。
