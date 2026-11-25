# 実装計画

1. session 無し/有りの有限 worker モデル、二重集計する負例、逆順完了と停止境界の重点履歴を追加する。
2. 公開前段と実行中プロトコルの driver、ITF adapter、literal cases、PBT を追加する。
3. 同じ driver を使う fuzz target、seed、CI 登録を追加する。
4. 保存・集計・停止時の retirement と順序を壊した実装変異を確認する。
5. 正負のモデルと鮮度、通常/全features、全体試験、lint と文書を確認し、作業ブランチへコミットする。

## 計画レビュー5回

- P1: 実プロセスを起動する試験ではなく、公開状態機械と effect payload の対応を対象とする。
- P2: session 保存待ちでの停止と、終了出力待ちでの停止を固定スケジュールに含める。
- P3: 1関数1ケース、AAA、rstest の個別ケースとし、反復と分岐は driver に置く。
- P4: 成功応答だけを追加すれば終了できる条件を生成履歴の後段で検査し、無条件の liveness を主張しない。
- P5: 既存20秒/RSS2048MiB guard と fuzz campaign deadline を維持し、timeout や compile error を負例検出と扱わない。

software-design-philosophy: 8/8、10/10（追加検証の責務分割）。失敗項目なし。runner、trace reader、CI の基盤を再利用する。

## quint-connect を含めた具体化

共有 Snapshot を quint-connect の State に接続し、通常 live generation、session live generation、API 応答を省く意味的負例、生成 seed の PBT を追加する。一般の Rust テストは固定 ITF を使い、専用 job だけが live Quint を起動する。CLI の終了コード124/125/126を隠さず伝播させる境界試験を追加する。公開 runner を使うため crate は0.1.2へ固定し、更新時に API と backend/guard の再確認を必要とする。

## ディスク容量の運用

ユーザー指定に従い、長いビルド・検証バッチの区切りで空き容量を確認し、Cargo/rustc/fuzz が動作していないことを確認して cargo clean を定期実行する。workspace と独立 fuzz workspace の両方を対象とし、最終検証・コミット後にも整理する。ソース、lockfile、seed、固定 ITF、検証ログは保持する。
