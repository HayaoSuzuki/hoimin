# 実行中 worker の形式検証

公開 API の bootstrap 停止検証を、複数 worker の候補適用・開始出力・実行結果・保存・終了出力・reset と停止後の drain へ広げる。RunState::new と公開 transition だけで各 worker に一つずつ候補を割り当て、ApplyMutation 待ちを初期状態とする。

有限モデルは2 worker、2候補、Killed/Survived、停止1回、session の有無を扱う。停止後の未実行候補は NotRun となる。保存待ちで停止した結果は元の結果を保持し、終了出力の再発行で集計を二重加算しない。通常完了順は任意、停止後の drain は候補 sequence 順である。

モデルと実装の対応は pending effect、公開 summary、確認済みの候補開始・終了、終了時の公開 phase、最終レポートの payload で観測する。モデルの補助フラグを private field と同一視しない。PBT/fuzz は1〜4 worker と長い履歴、重複停止、誤った worker/完了種別/保存結果 identity、retired/duplicate completion を扱う。

テスト用 driver が公開前段の構築、通知の組み立て、効果の台帳と公開観測を所有する。既存 runner と ITF reader を使い、shipping API に試験用入口を追加しない。実 OS の subprocess 並行実行、クラッシュ耐性、任意のエラー入力を入れ続ける無限履歴の liveness は別の課題とする。

## 設計レビュー5回

- D1: 内部 WorkerState の fixture ではなく、公開前段で複数 ApplyMutation 待ちへ到達させる。
- D2: 通常の並行完了と停止後の直列 drain を区別し、出力の発行と確認応答を混同しない。
- D3: session の保存待ちと終了出力待ちでは集計済みかどうかが異なるため、独立した集計回数をモデルに持つ。
- D4: 2 worker の有限全探索と、最大4 worker の生成試験を分け、直積の無制限な拡大を避ける。
- D5: 消費された state のエラー後観測を主張しない。拒否入力は clone へ渡す replay 方針を継承する。

software-design-philosophy: 検証モジュール境界は8/8、10/10。失敗項目なし。通知・台帳・モデル対応を driver に集約し、各テストの入口を履歴の指定へ絞る。

## quint-connect の導入

ユーザーから紹介された quint-connect 0.1.2 の公開ソースを調べ、独立した検証用 workspace `formal/quint/connect` へ固定した。候補は全 Rust テストを live generation に置き換える方法と、既存の固定 ITF/PBT/fuzz を維持して live check を追加する方法。通常の cargo test に Node/Quint を要求せず、既存 guard を維持するため後者を選んだ。

Driver と State の対応には既存の worker driver と同じ Snapshot を使う。モデルの操作は typed sum の dispatch として一度だけ記録する。Config.nondet をその値へ向けることで、匿名の合成 action の名前推論に依存しない。State=() は使用せず、各段階で公開観測を比較する。

専用 launcher は固定版 Quint、TypeScript backend、Safety、20秒/RSS2048MiB を強制する。live PBT は seed を生成し、既存 PBT はコマンド列を縮小し、libFuzzer は同じ公開 API driver へ入力を渡す。検証用の Cargo.lock と依存は製品 workspace と分離する。

TLC は有限モデルの到達状態を全探索する。quint-connect/PBT/fuzz は Rust 実装との対応を試験する。この構成だけで Rust atomic のメモリ順序や OS の全スレッド interleaving を証明したとは扱わない。

資料: [公式 crate](https://docs.rs/quint-connect/0.1.2/quint_connect/)、[公式ソース](https://github.com/quint-co/quint-connect)、[Quint MBT](https://github.com/quint-co/quint/blob/v0.33.1/docs/content/docs/model-based-testing.mdx)。
