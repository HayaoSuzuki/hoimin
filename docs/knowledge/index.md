---
okf_version: "0.2"
---

# hoimin 設計・監査カタログ

- [このカタログの範囲と読み方](overview.md) - 参照コミット、証拠レベル、未追跡資料、更新方法。
- [開発での参照・更新手順](../okf-workflow.md) - 作業開始時の確認、概念の作成基準、出典と索引の更新、完了時の検査。
- [PyPIへのwheel公開](pypi-publishing.md) - 手動公開、Trusted Publishingの登録値、配布物の検証と再実行。

# 設計と契約

- [全体構成](design/architecture.md) - 隔離コピー、状態遷移と入出力の分離、Rust解析器への移行。
- [Python解析と候補](design/analyzer.md) - 構文・名前解決・ユーザ定義例外・変更するバイト範囲・入力規模。
- [対象選択・plan・verify](design/selection-plan-verify.md) - ランキング、部分集合、保存形式の版の要確認事項。
- [資源と終了処理](design/runtime-lifecycle.md) - OS別制限、後処理の所有権、レポート書込み。
- [sessionとレポート](design/session-report.md) - DB所有権、移行、読取り時の整合性検証、実行中の進捗と診断ログ。
- [JSON mutant spool](design/json-report-spool.md) - record単位の書込みと失敗時のack・poison契約。

- [fingerprint入力glob](design/fingerprint-inputs.md) - 複数include globの共有走査と入力順エラー。

# 監査と検証範囲

- [追加監査のLean統合報告](../superpowers/reports/2026-09-15-followup-lean-promotion.md) - 6モデル・33定理・58入力の正式化と、未解決の実装対応。

- [nullable型変異の適用条件](audits/nullable-gates-2026-09.md) - #564/#565の再現、名前解決と型引数の木のLean検証。

- [コピー方針とsession再開](audits/resume-copy-2026-09.md) - #563の旧判定再利用、Leanと公開CLI・SQLiteの照合。

- [値なし注釈と候補精度](audits/declaration-only-2026-09.md) - 値なし注釈の束縛保持、関数ローカル宣言との区別、公開planとCPythonの14入力回帰検証。

- [評価順序と未選択methodの確保](audits/evaluation-order-2026-09.md) - #560の評価順序、#561の未選択method置換の省略と累積確保要求の検証。

- [遅延注釈とcollections.abc.Setの追加監査](audits/annotation-followup-2026-09.md) - #558/#559の再現、キャッシュ保持の証明と型名の照合。

- [withの例外抑制とfinallyの解析コスト](audits/with-finally-2026-09.md) - with抑制の修正と、#557のfinally重複走査削減・訪問回数ゲート。

- [暗黙例外と finally](audits/implicit-finally.md) - typing 由来の合流、Lean 小モデル、公開 plan の対応範囲。

- [Lean監査の読み方](audits/lean-evidence.md) - モデル証明・有限探索・Rust対応・実機確認の違い。
- [2026-09-11 境界監査](audits/boundary-2026-09.md) - 解析・選択・実行・保存の横断結果と未検証条件。
- [progress入力の後続修正](audits/progress-input.md) - 過去の368/564観測、JSONLイベント列の読取りと検証範囲。
- [2026年7月 Rust監査](audits/rust-2026-07.md) - 初期監査の発見分類と実機制約。

# 原文を探す

- [原文索引の入口](references/index.md) - 設計書と監査・報告の一覧。
- [設計書一覧](references/design-documents.md) - 全設計Markdownへの索引。
- [監査・報告一覧](references/audit-documents.md) - 全監査・報告Markdownへの索引。

- [入力形状別の性能検証](audits/performance-shapes.md) - 実行ゲート、release計測、依存PRと証拠の限界。

- [境界をまたぐ契約 fixture](audits/boundary-contracts.md) - strict 実行、全件報告と未検証条件。

- [コレクション型注釈の参照先](design/annotation-builtins.md) - 具体型の両方向、遅延評価とscopeの判定。

- [2026年9月の解析監査と回帰検証への反映](audits/analysis-2026-09.md) - #545〜#549の原証拠、修正PR、正式なテストの対応。
