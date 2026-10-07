# 追加監査: コピー方針とsession再開

`--include` / `--exclude` を変えても旧コピー条件のkilled/survivedを再利用する不具合を確認し、[#563](https://github.com/tokyogas-tech/hoimin/issues/563)に起票した。対象は `5e631efc46a6e8c0b9fcf2d7a74536e57f79a369`、macOS arm64、CPython 3.14.7、Lean 4.32.2。実装は変更していない。

## 契約と最小の証拠

[session再開契約](../../usage.md#sessions-and-resume)は、互換な不完全runを再利用し、不互換な条件の結果を混在させないとする。現在のfingerprintはコピーのinclude/exclude設定を含めないため、テストが読む補助ファイルの有無が変わっても、以前の結果を選んでしまう。分類はconfirmed bugである。

補助ファイル `strict.flag` がある場合だけ `subject.value() == 10` を検査するテストで再現した。subjectには加算3箇所を置き、max-mutants=1で先頭候補だけを実行してSQLiteへ保存する。初回はflagがコピーされ、候補はkilledとなる。次のresumeでflagをexcludeすると、同じ候補を新規実行すればsurvivedになるが、実際は古いkilledを再利用した。scoreも新規実行の0.0に対して1.0となった。

対象ソース・テスト・flagの内容、test argv、import roots、実行件数上限は不変である。flagには両runともfingerprint-fileを付けた。コピー条件が違うだけではroot上のpath/hashが変わらないため、この指定でも防げない。各runのbaseline成功、候補ID一致、残り2候補のnot_run、complete=false、exit=4を確認した。exit=4は意図した件数上限であり、検証基盤の失敗ではない。

[Issue本文](issue.md)に全ソース、コマンド、原因箇所を記した。原因は `FingerprintInput::from_config` の入力不足と、そのfingerprintに基づくsessionの選択である。#350のjobs/max-outputの過剰な無効化や、#472/#516のsession自身のコピー除外とは条件が異なる。

## Leanの前提・証明・有限検査

[モデル化前の対応表](correspondence.md)で、コピー有無、同じ入力、永続化済みの不完全run、再利用、現条件での新規実行を対応付けた。[ResumeModel.lean](ResumeModel.lean)は、コピーされるflagの有無をBool、出力保持上限をNatで表す。今回のfixtureではflagがあればkilled、なければsurvivedとなる。保存済みレコードの判定が保存時のコピー条件に対応することを前提にする。

モデル内では、コピー条件が異なる場合の再利用禁止、resume結果と現条件の新規実行結果の一致、任意の出力上限の違いだけでは互換性を失わないことを証明した。最後の性質により、判定に関係しない設定まで互換性へ追加する過剰な無効化を避ける。これらはRust全体、全コピーmanifestの同値性、hash衝突耐性の証明ではない。

[ResumeMain.lean](ResumeMain.lean)はold/newのコピー有無各2種類と、old/newの出力上限を列挙する。出力上限1種類で測定した後、2種類に増やした。固定順序の有限全列挙で、各条件は保存済みレコードからのresume 1遷移。対称性削減や長いトレース探索は行っていない。

| 出力上限の種類 | 設定数 | ケース・resume遷移数 | コピー条件を落とす規則との不一致 | 出力上限まで比較する規則との不一致 |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 2 | 4 | 2 | 0 |
| 2 | 4 | 16 | 8 | 4 |

固定した最小証拠は `copied=trueでkilledを保存 → copied=falseで再開`。正しい規則では新規実行のsurvived、reused=falseとなる。コピー条件を落とす壊した規則ではkilled、reused=trueとなる。設定据置きの正例では再利用を確認し、一律に再利用を拒否する規則も検出した。互換性の境界を検査しており、並行処理の原子性、DB破損、結果の二重書込みは今回のモデル外である。

モデルと生成器は分離し、sorry/admit/native_decideは使っていない。各Leanプロセスは単一スレッド、既定heartbeat上限、20秒・2GiB・50ms監視で実行した。最終検証はモデル4,160ms / peak RSS 671,728KiB、生成結果の照合635ms / 689,232KiBで両方exit=0。各有限検査の本体はミリ秒精度で0msだった。[model計測](verification/model.json)、[search計測](verification/search.json)、[検査ログ](verification/search.log)、[初回生成ログ](generation/search.log)を保存した。制限を上げた実行や放棄した大規模探索はない。

## 公開CLI・SQLiteとの照合

Lean生成の[7ケース](corpus.jsonl)を[replay.py](replay.py)で実行した。各ケースを一時ディレクトリに隔離し、初回run→同じSQLite DBからresume→現条件でsessionなしの新規run、の3実行を行った。DBを手動で変更していない。

| ケース | 初回 | resumeの実測 | 現条件の新規実行 | 再開契約との照合 |
| --- | --- | --- | --- | --- |
| exclude追加 | killed | killedを再利用 | survived | mismatch |
| exclude削除 | survived | survivedを再利用 | killed | mismatch |
| flagコピーありで据置き | killed | killedを再利用 | killed | match |
| flagコピーなしで据置き | survived | survivedを再利用 | survived | match |
| include追加 | survived | survivedを再利用 | killed | mismatch |
| include削除 | killed | killedを再利用 | survived | mismatch |
| max-outputだけ1024B→2048B | killed | killedを再利用 | killed | match |

debug/release各21 runで、3 match / 4 mismatch / infrastructure-error 0だった。7ケースはstrict、抽象設定の全列挙と任意Natの定理はmodel-onlyである。比較した観測は初回status、resume status、現条件の新規status、再利用有無の4項目。run_id一致と再利用結果のterminationなしが一致し、SQLiteは旧run 1件を保持していた。元ファイルの内容が不変であることもSHA-256で確認した。

[release結果](replay-release.json)と[debug結果](replay-debug.json)には、バイナリSHA-256、コマンド、先頭候補、baseline、集計、コピー設定、fingerprint入力、SQLiteのrun_id/fingerprint、cleanup結果を保存した。保存形式は照合に必要なフィールドの抽出であり、全runの生レポートではない。既存の別実行のjanitor詳細は保存していない。今回のexecution cleanupはclean、deliveryはcleanup_after_deliveryで、残存rootは報告されなかった。

## 再実行と残る判断

```sh
python3 docs/audits/2026-09-15-resume-copy/verify_lean.py --output /tmp/resume-copy-proof
python3 docs/audits/2026-09-15-resume-copy/replay.py --binary target/release/hoimin --output /tmp/resume-copy-release.json
python3 docs/audits/2026-09-15-resume-copy/replay.py --binary target/debug/hoimin --output /tmp/resume-copy-debug.json
cargo test --offline -p hoimin-cli --test session_handler --test lean_session_oracle --test fingerprint_inputs
```

LeanのRSS監視にはpsの権限が必要。生成を更新するときは `verify_lean.py --generate --output <新規ディレクトリ>` を使う。CLI再現はjobs=1、workspace上限8GiB、空き容量10GiB超を維持し、ディスク停止・cleanup異常があれば次のケースへ進まない。不一致はJSONへ記録し、不一致だけでは非ゼロ終了しないため、監査用adapterをそのままCIゲートへ追加しない。

関連する既存テストは28+5+31の64件成功、subprocess fixtureの1件は予定どおりignoredだった。progressの集計・構造検証も読んだが、今回そこへの新規Issueはない。修正時には、互換性へinclude/exclude設定を含めるか、影響するコピー入力を比較するかを決め、fingerprint schemaと旧sessionの扱いを整理する。実装修正・正式CIへのケース移行、Windows/Linuxでの公開runは未実施である。
