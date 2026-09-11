---
type: Audit
title: progress入力の集計値検証と後続修正
description: 境界監査後の修正、368観測の範囲、既存の比較用テストの前提変更を記録する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
audit_revision: 623dd808612dbc34775e16814845eec0bc52dff9
sources:
- id: report
  resource: ../../superpowers/reports/2026-09-11-progress-input-lean.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: evidence
  resource: ../../superpowers/reports/2026-09-11-progress-input-verification.json
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: input
  resource: ../../../crates/hoimin-cli/src/progress/input.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: adapter
  resource: ../../../crates/hoimin-cli/tests/lean_progress_input_oracle.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
---

# 境界監査後のsummary検証修正

progressは、変異試験の結果を読み込んで進捗を比較する処理である。元報告はbase `623dd808612dbc34775e16814845eec0bc52dff9` で、集計結果（summary）の完了状態・件数・終了コードが矛盾していても、読取り処理が比較へ渡す問題 #483 を扱う。[^report]

後続修正は、このカタログが参照するHEAD `a7daea0b557cd435c1e55b540392fbdd116348e1` に含まれる。元報告の「integration待ち」は当時の状態を指すため、現在も未統合とは解釈しない。[^report]

# 完了状態・件数・終了コードの整合性

修正後は、両スキーマの読取り処理で件数を検査した後、`validate_summary_coherence` を呼ぶ。完了を表す `complete` がtrueなら、結果を確定できない候補の件数 `inconclusive` が0であり、終了コードも集計に基づく方針と一致する必要がある。[^input]

未完了の場合は、入力にない実行状態フラグについて、既存の終了コード規則と矛盾しない組合せが存在するかを確認する。フラグが書かれていないことからfalseとは推定せず、矛盾するsummaryを修復する処理も加えていない。これらの呼出しと判定は、カタログ作成時に実装を読んで確認した。[^input]

# 元報告に記録された368観測

| 観測 | 元報告の結果 |
| --- | --- |
| 有限の入力表 | 184行 × JSONスキーマ2/3 = 公開CLIを使う368件のstrict観測 |
| 修正前 | 368中264観測が不一致。同一不具合が複数の入力で現れた件数 |
| 修正後 | 全368観測が一致。不正入力は終了コード2、対象パス付きエラー、progress JSON出力なし |
| 意図的に壊した判定の検出 | completeの無条件受理、inconclusive検査欠落、終了コードの優先順位違反の3種類を検出 |

元報告の実行環境はmacOS arm64である。比較の記録と、実行時間・メモリを監視する資源guardの結果はJSONに保存されている。今回はこれらの資料を参照し、試験を再実行していない。[^report][^evidence][^adapter]

# 既存の比較用テストデータの分類変更

従来のProgressDecisionテストの6ケースは、結果を確定できない候補を含むのに比較可能とするレポートを、JSON入力から作っていた。この入力は新しい読取り契約に反するため、6件を型付きAPIで直接構成する `internal-fixture` に分類し直した。[^report]

公開CLI経由の17件のstrictケースと、モデル内だけで確認する1件のmodel-onlyケースは別に維持する。比較規則と期待値は変えず、各ケースの前提を満たせる入口へ試験を移した。[^report]

# 証明の範囲と実行資源

4つの一般定理が示すのはモデル内の性質である。有限の入力表で試験したため、任意のJSON・入力サイズ・履歴を検証したとはいえない。個々の変異候補の終了理由・出力欄の整合性（#460）や、元コードの試験結果と実行状態フラグとの全関係も範囲外である。[^report]

初回のネイティブ実行形式へのリンク処理は、メモリ監視の768MiB閾値で停止した。後続報告では、承認を得て上限を1GiBとし、20秒の時間制限、250msごとの監視、直列実行を保った条件で、事例生成プログラムの成功を記録している。既存キャッシュを使った増分ビルドの測定であり、キャッシュなしの結果は示していない。この過去の承認は、別作業の資源上限変更の許可には使わない。[^report]

# 再確認条件

summary、終了コードの優先順位、読取り処理の受理条件、元コードの試験結果の分類、スキーマ、比較用テストを変更したら見直す。公開CLIと内部APIのそれぞれで、ケースの前提を満たした入力を構成できているかを照合する。

[^report]: [2026-09-11-progress-input-lean.md](../../superpowers/reports/2026-09-11-progress-input-lean.md)。
[^evidence]: [2026-09-11-progress-input-verification.json](../../superpowers/reports/2026-09-11-progress-input-verification.json)。
[^input]: [input.rs](../../../crates/hoimin-cli/src/progress/input.rs)。
[^adapter]: [lean_progress_input_oracle.rs](../../../crates/hoimin-cli/tests/lean_progress_input_oracle.rs)。
