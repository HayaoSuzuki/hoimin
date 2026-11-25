# 終了・削除許可・所有権への検証拡張

起点は `9c31d188`。予算・レポート・完了台帳に続き、実際の停止・削除許可・OS 所有権を検証する。既存の有限上限だけを増やす案と比較し、異なる本番メソッドへ接続する3領域を採る。software-design-philosophy の情報隠蔽と深いモジュールの観点で、実行 runner と ITF reader は既存のものを使う。

- DiskLifecycle：公開 apply/snapshot/may_finish/safety_succeeded。dispatch 上限1の有限 workload、process/output/monitor/report の成功・失敗、2ルートの cleanup要求・結果、終了後の不変性。両ルート・executionのみ・ルートなしの3設定を全探索する。診断文と secondary の順序は有限モデルに入れず、PBT では失敗の固定と重複排除、拒否時 snapshot 全体不変も確認する。
- ExecutionFinalization：CLI の実メソッドへ内部 fixture で接続。未 join の計測は後の成功で復旧しないこと、拒否された削除試行を再試行しないこと、inner/outer drain の先行失敗、cleanup結果と計測 bytes の報告、空き容量 query の一度だけの観測と0の保持を対象にする。OS 上の削除処理そのものの証明ではない。
- RunOwnership：2 actor / 2 run ID の acquire/release/close。実際の非ブロッキング OS file lock、独立した open の衝突、run間独立、drop後の再取得へ履歴を再生する。PBT は4 actor / 3 run IDへ広げる。SQLite transaction、ホスト停止、任意の別プロセスの crash recovery の証明とは区別する。

Quint の指定シナリオは入力のみを固定し、ランダム step と共通の action から期待値を生成する。各モデルの負の対照と隔離した Rust mutation の双方を使う。1コマンド20秒・RSS2048 MiB、TLC1worker・heap1GiB、重い検証は順次実行する。モデルが上限で未完了なら成功とせず、領域や設定を分割する。

新たな本番公開 API は作らない。公開 disk adapter、CLI内部 finalization adapter、実OS ownership adapter の証拠を分ける。AAAと1関数1履歴、rstest、長い履歴のPBT、diskのfuzzを追加する。設計・計画・実装・テストをそれぞれ5回レビューし、同じ作業ブランチへコミットする。

探索上限の判断：dispatch 2回ではTLCの全状態処理後も外側20秒制限に達したため成功とせず、有限モデルは1回へ縮小する。複数 dispatch は公開APIの長い履歴PBTで扱う。
