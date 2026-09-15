# モデル化前の対応表

対象HEADは5e631ef。契約は「判定に影響するコピー設定が異なる実行の結果を、互換なsession結果として再利用しない」。READMEのSessions and resumeは、互換な不完全runのみを再利用し、不互換な結果を混在させないとする。

| 前提・観測 | Lean表現 | 公開CLI・保存境界 | mode |
| --- | --- | --- | --- |
| 補助ファイルがコピーされるか | copied Bool | --exclude strict.flag、または.ignoreと--include strict.flag | strict |
| 同じソース・test argv・fingerprint入力 | モデルの固定前提 | ファイルを変更せず--fingerprint-file strict.flagを両runに付ける | strict |
| テストの判定 | killed Bool=copied | strict.flagがある場合だけ変異を検出する実テスト | strict |
| 永続化済みの不完全run | Cached | --session、max-mutants=1で3候補のうち1件を保存しexit4 | strict |
| 再開時の結果再利用 | result.reused | --resume後のrun_id一致、先頭候補terminationなし、SQLiteのrun数 | strict |
| 現条件での新規実行 | verdict=copied | sessionなしの同一設定で先頭候補のstatusを比較 | strict |
| max-outputの変更 | outputCap Nat（互換性判定に不使用） | 1024B→2048B、jobsは1を維持 | strict |
| 一般の正しい保存済み値 | killed=copiedの仮定 | 任意のテスト判定を今回のfixtureだけでは網羅しない | model-only |
| 任意NatのoutputCap | 無制限Natの定理 | 実行は2種類の正の上限のみ | model-only |

候補はbinary_add_subの先頭1件、残り2件はnot_run。全runのexit4は同じ実行件数上限による意図した不完全終了として検査する。DBへの手動書換え、並行実行、外部環境変化、暗号学的hash衝突、全コピーmanifestの同値判定は対象外。コピーされる補助ファイルは1つで、他の対象ファイルは一定。

Leanの有限検査はold/newのcopied各2種類と、outputCap各2種類の16条件。1種類目の上限から2種類目へ段階的に増やす。モデルと生成器は分離し、各プロセス20秒・2GiB・50ms監視・単一スレッド・既定heartbeat上限で実行する。
