# 公開停止プロトコルのレビュー記録

基点 `10957809`、作業ブランチ `test/quint-state-machines`。作者のセルフレビューであり、独立した第三者レビューではない。
設計と計画の各5回レビューは [設計](design.md)・[計画](plan.md) に記録した。

## 実装レビュー5回

- I1: 前段を内部 fixture で作ると公開 protocol の検証にならない。RunState::new から ResolveTargets、Preflight、BeginSession の正しい完了通知で各 entry へ到達させた。
- I2: 出力 sequence の初回値は1であり、初稿 driver の0という仮定が誤っていた。production の採番を確認し、既に発行済みの開始出力と次の出力を区別した。
- I3: 「出力 I/O failure 後の出力を全禁止」は cleanup failure の追加 diagnostic も禁止する過剰な仮定だった。最終レポート後の追加出力禁止に限定した。有限モデルの failure diagnostic と対応を確認した。
- I4: PBT は未確認の開始出力から終了した際、最終出力失敗より開始再試行を優先する製品不具合を発見した。3操作の回帰を red にした後、終了判定の優先順だけを修正した。
- I5: driver の停止原因をまとめ、コマンドから通知への変換と適用後の観測を分けた。非公開状態のコピーや試験用の製品 API を増やさず、同じ driver を ITF/PBT/fuzz で共有した。

software-design-philosophy: 最終的な検証モジュール境界は8/8、10/10。失敗項目なし。責務、入口の単純さ、変更の局所化、抽象のコメント、レビュー、知識の所有、境界の説明、設計改善を確認した。モデルや製品全体の正しさの点数ではない。

## テストレビュー5回

- T1: 1テスト関数1履歴、AAA、rstest の独立ケースを確認した。36 ITF ケースと、literal/PBT/decoder/regression の32ケースを追加した。
- T2: 有限モデルの停止2回制限を PBT と fuzz で超える。任意 u64 のディスク観測、retired/duplicate/unknown/wrong-kind、cleanup 予約不一致を扱う。
- T3: 任意の failure を入れ続けた履歴の liveness は主張しない。生成履歴の後に8回の成功 ack を与えたときの終了を別の進行条件として確認する。
- T4: エラー時の clone 保持は driver の replay 方針であり、消費された入力の内部状態の観測ではない。期待エラーの種類を比較し、公開契約以上の atomicity を主張しない。
- T5: 正しいモデルの通過だけでなく、早すぎる session finish の負例と実装変異を確認する。通常/全features、既存全体、資源制限、CI/fuzz/lint と文書の検証結果を下記へ記録する。

## 実行結果

追加68ケースは通常構成・全features構成で成功。全14設定・9負例の検査と全86履歴の鮮度検査が成功した。最後の6重点履歴は既存実行後に追加したため、公開停止の全27履歴を個別に再生成比較した。

| 新規モデル設定 | 到達状態数 | guard 実時間 | peak RSS |
| --- | ---: | ---: | ---: |
| StopEarly | 2146 | 4.466秒 | 約777MiB |
| StopCopy | 2729 | 4.748秒 | 約756MiB |
| StopSession | 3138 | 4.815秒 | 約699MiB |

各20秒・RSS2048MiBの既存制限内。有限全探索は worker 作成前の安全性検査であり、任意の OS interleaving や無限履歴の liveness を意味しない。

隔離コピーで5変異を確認し、すべて assertion の失敗で検出した。compile error は検出に数えていない。重点履歴追加後の68ケースで再実行した。

| 変異 | 失敗ケース数（ITF / literal・PBT） |
| --- | ---: |
| 開始未確認時の最終レポート失敗を終了扱いしない | 6 / 13 |
| 最終レポート中の停止保護を外す | 4 / 2 |
| 最終レポート確認前に FinishSession を発行する | 9 / 13 |
| cleanup 予約集合の照合を外す | 2 / 7 |
| 2回目のディスク停止理由で最初の理由を上書きする | 0 / 2 |

run_stop libFuzzer は pinned nightly、seed1172、15秒指定で16秒・53672入力を実行し、異常なし。入力timeout5秒・RSS1024MiB・max_len4096、最大128生成操作と8成功ack。生成 corpus と seed は分離した。fuzz target の stable Clippy も成功。

Python 全体は2781成功・35スキップ（105.28秒）。workspace 全targets/allfeatures Clippy、fmt、ruff check/format、ty、workflow lint、strict docs build が成功。Rust 全体は `RUST_TEST_THREADS=4 cargo test --locked --workspace --no-fail-fast` により246 suite、18929成功・失敗0・62 ignored。追加68ケースの通常/全features構成、全体試験、モデルと鮮度、実装変異、実 fuzz をすべて確認して完了した。
