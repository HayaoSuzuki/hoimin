---
okf_version: "0.2"
---

# hoimin 設計・監査カタログ

- [このカタログの範囲と読み方](overview.md) - 参照コミット、証拠レベル、未追跡資料、更新方法。
- [開発での参照・更新手順](../okf-workflow.md) - 作業開始時の確認、概念の作成基準、出典と索引の更新、完了時の検査。

# 設計と契約

- [全体構成](design/architecture.md) - 隔離コピー、状態遷移と入出力の分離、Rust解析器への移行。
- [Python解析と候補](design/analyzer.md) - 構文・名前解決・変更するバイト範囲・入力規模。
- [対象選択・plan・verify](design/selection-plan-verify.md) - ランキング、部分集合、保存形式の版の要確認事項。
- [資源と終了処理](design/runtime-lifecycle.md) - OS別制限、後処理の所有権、レポート書込み。
- [sessionとレポート](design/session-report.md) - DB所有権、移行、読取り時の整合性検証。
- [JSON mutant spool](design/json-report-spool.md) - record単位の書込みと失敗時のack・poison契約。

- [fingerprint入力glob](design/fingerprint-inputs.md) - 複数include globの共有走査と入力順エラー。

# 監査と検証範囲

- [Lean監査の読み方](audits/lean-evidence.md) - モデル証明・有限探索・Rust対応・実機確認の違い。
- [2026-09-11 境界監査](audits/boundary-2026-09.md) - 解析・選択・実行・保存の横断結果と未検証条件。
- [progress入力の後続修正](audits/progress-input.md) - 過去の368観測、単一結果検証を加えた564観測、既存テストデータの前提変更。
- [2026年7月 Rust監査](audits/rust-2026-07.md) - 初期監査の発見分類と実機制約。

# 原文を探す

- [原文索引の入口](references/index.md) - 設計書と監査・報告の一覧。
- [設計書一覧](references/design-documents.md) - 全設計Markdownへの索引。
- [監査・報告一覧](references/audit-documents.md) - 全監査・報告Markdownへの索引。

- [入力形状別の性能検証](audits/performance-shapes.md) - 実行ゲート、release計測、依存PRと証拠の限界。
