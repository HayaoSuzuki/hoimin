# 実装計画

1. 3設定の有限モデルと、最終レポート確認前の session finish を許す負例を作成する。
2. 公開 API 用 driver と ITF adapter、literal cases、PBT を作成する。
3. 同じ driver を使う libFuzzer target と seed、CI 登録を追加する。
4. 実装変異で停止保護・終了順序・予約照合の検出能力を確認する。
5. 正負のモデル、鮮度、通常/全features、全体試験、lint、文書を確認して作業ブランチへコミットする。

## 計画レビュー5回

- P1: private ledger ではなく公開 transition と出力 payload を観測対象にした。
- P2: 正常な停止完了、disk reason の保護、出力失敗を固定入力スケジュールに含める。
- P3: 1関数1履歴、AAA、rstest の独立ケース。反復と分岐は driver に閉じ込める。
- P4: 有限モデルの停止回数制限を製品の ID 上限として扱わず、生成試験で長い履歴を実行する。
- P5: 20秒/RSS2048MiB の既存 guard と fuzz campaign の deadline を維持する。コンパイル失敗を変異検出に数えない。

software-design-philosophy: 8/8、10/10（検証モジュールの責務分割）。失敗項目なし。実行基盤とモデルのレジストリを再利用する。
