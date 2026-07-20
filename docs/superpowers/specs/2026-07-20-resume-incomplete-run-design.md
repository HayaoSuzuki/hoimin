# 不完全 run の再開

## 目的

`--max-mutants` に達して不完全終了した session を `--resume` で繰り返し再開できるようにする。
各回が同じ上限で再び不完全終了しても、SQLite session の終了処理は失敗せず、その run は次回の再開候補に残る。

## 現在の不整合

不完全終了では `runs.complete=0` と `runs.finished=1` を記録する。
`LoadSession` は `complete=0` の run を再開候補として選ぶため、上限で終了した run は再開される。
しかし、再開した run が再び不完全終了すると、`SessionHandler::finish(false)` は `finished=0` を条件に更新する。
すでに `finished=1` なので更新件数が 0 になり、`session.finish.state` を返す。

この失敗は、再開候補の選択条件と不完全終了の状態遷移が一致していないために起きる。

## 状態遷移

**完全性**は `complete` だけで表す。

- **不完全 run**：`complete=0`。
  `--resume` の候補である。
  `finish(false)` は何度呼ばれても成功し、`complete=0` と `finished=1` を維持する。
- **完全 run**：`complete=1`。
  `--resume` の候補ではない。
  `finish(true)` は `complete=0` の run を一度だけ完全 run へ遷移させる。
  完全 run に対する `finish(true)` または `finish(false)` は従来どおり状態エラーにする。

`finished` は run が終了処理を通ったことを示す記録として残すが、不完全 run の再開可否や再度の不完全終了の可否には使わない。

## 実装境界

変更は SQLite session handler の `finish` に限定する。
状態機械はすでに不完全な run に `FinishSession { complete: false }` を発行し、`LoadSession` は `complete=0` の最新の互換 run を選んでいるため、変更しない。

`finish(false)` の更新条件から `finished=0` を外し、`complete=0` の run を更新対象にする。
`finish(true)` の更新条件は `complete=0` のままとする。

## テスト

session handler の状態遷移テストで、`finish(false)` を二回実行して成功することを確認する。
続けて `finish(true)` は一回だけ成功し、完全化した run への追加の終了処理は `session.finish.state` を返すことを確認する。

E2E テストで `--max-mutants 8` の session run を実行し、`complete=0` を確認する。
同じ session と設定で `--resume` を実行して再び上限に達しても終了コード 4 で完了し、`session.finish.state` を出さず、run が再度 `--resume` の候補になることを確認する。
