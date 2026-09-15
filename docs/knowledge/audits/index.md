# 監査と検証範囲

- [Lean監査の証拠をどう読むか](lean-evidence.md) - モデル内証明・有限探索・Rust対応・実機確認の違いを具体例で示す。
- [2026-09-11 境界条件の横断監査](boundary-2026-09.md) - 構文・選択・実行・保存の監査結果と、静的確認・未検証の範囲を残す。
- [progress入力の集計値検証と後続修正](progress-input.md) - 境界監査後の修正、368観測の範囲、既存の比較用テストの前提変更を記録する。
- [2026年7月 Rustコードベース監査](rust-2026-07.md) - 初期Rust監査の発見分類、品質ゲート、実機未確認範囲を保存する。

# 関連

- [カタログの入口](../index.md) - 全体の読み順。

- [入力形状別の性能検証](performance-shapes.md) - 実行ゲート、release計測、依存PRと証拠の限界。

- [境界をまたぐ契約 fixture](boundary-contracts.md) - strict 実行、全件報告と未検証条件。

- [暗黙例外と finally](implicit-finally.md) - typing 由来の合流、Lean 小モデル、公開 plan の対応範囲。

- [2026年9月の解析監査と回帰検証への反映](analysis-2026-09.md) - #545〜#549の原証拠、修正PR、正式なテストの対応。
