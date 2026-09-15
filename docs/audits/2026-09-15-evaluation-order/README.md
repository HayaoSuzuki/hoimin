# 追加監査: 評価順序と未選択methodの確保

対象は `5e631efc46a6e8c0b9fcf2d7a74536e57f79a369`、macOS arm64、CPython 3.14.7、Lean 4.32.2。既報と重複しない2件を起票した。実装は変更していない。

| Issue | 確認した結果 |
| --- | --- |
| [#560: 評価順序によるbuiltinの誤認](https://github.com/tokyogas-tech/hoimin/issues/560) | 多重代入、連鎖代入、RHSのwalrus、starred引数で、先に再代入された名前を組込みと誤認する。sourceとdestination両方に影響する。 |
| [#561: 未選択methodのreplacement確保](https://github.com/tokyogas-tech/hoimin/issues/561) | append変異を選択していなくても、親callの文字列を複製して破棄する。深さ32で大きな確保要求の累積量は97,040,138bytes、対照は1,000,394bytes。 |

## 評価順序の契約と実装対応

代入文はRHSの評価後にtargetを左から順に格納する。[Python公式仕様](https://docs.python.org/3/reference/simple_stmts.html#assignment-statements)。また、starred引数は、ソース上で先行するkeyword引数より先に処理される。[呼出しの公式仕様](https://docs.python.org/3/reference/expressions.html#calls)。現在の名前解決はソースoffsetを時点として使うため、この順序を表せない。

[事前対応表](correspondence.md)に前提と照合範囲を記した。[OrderModel.lean](OrderModel.lean)は置換元・置換先が組込みかをBoolで持ち、bindSource、bindDestination、lookupの3イベントを扱う。初期状態は両方trueであり、束縛でfalseになる。組込みへ戻す操作は含めない。

モデル内では、どちらかを再代入した後は任意長の後続列で候補を拒否すること、lookupはその時点の状態を参照すること、lookup後の代入は先行lookupを失わせないことを証明した。Rust全体の名前解決の形式証明ではない。

[OrderMain.lean](OrderMain.lean)が生成した[7入力](corpus.jsonl)を公開planで照合した。期待値は候補のoriginal/replacement組であり、spanが元ソースに対応することも検査した。ID・順位・descriptor全体の正しさは証明していない。

| 入力 | 期待候補数 | debug / releaseの候補数 |
| --- | ---: | ---: |
| unpack_target | 0 | 1 / 1 |
| chain_target | 0 | 1 / 1 |
| rhs_walrus | 0 | 1 / 1 |
| destination_target | 0 | 1 / 1 |
| keyword_star | 0 | 1 / 1 |
| rhs_before_store | 1 | 1 / 1 |
| normal_call | 1 | 1 / 1 |

debug/releaseともstrict照合は2 match / 5 mismatch / infrastructure-error 0。全元ソースと全候補はCPythonで正常に実行でき、観測値の違いを記録した。unpack_targetでは、元ソースの `['custom']` が変異後に `[True]` となる。公開release runもbaseline成功、killed=1、score=1.0、complete=true、exit=0だった。証拠とバイナリSHA-256は[debug結果](replay-debug.json)・[release結果](replay-release.json)、最小再現と原因箇所は[Issue本文](issue-order.md)に保存した。

## Leanの探索範囲と検出感度

モデルと探索プログラムを分離し、単一プロセス・単一スレッド・既定heartbeat上限で実行した。各プロセスの制限は20秒、2GiB、監視間隔50ms。定理にsorry、admit、native_decideは使っていない。

3イベントを固定順序で深さ0から4まで列挙し、対称性削減は行わなかった。表のtracesは空列を含む各深さ以下の列数、transitionsは列ごとの長さの合計である。mismatchesは全書込みを文末へ遅延する壊したモデルとの不一致であり、Rust照合件数ではない。

| 深さ | traces | transitions | mismatches |
| ---: | ---: | ---: | ---: |
| 0 | 1 | 0 | 0 |
| 1 | 4 | 3 | 0 |
| 2 | 13 | 21 | 2 |
| 3 | 40 | 102 | 14 |
| 4 | 121 | 426 | 64 |

最小反例は `bindSource → lookup`。source・destinationの反映遅延を検出し、逆順の `lookup → bindSource` を保持する正例で事前の一律拒否も検出した。順序と境界の検査であり、並行処理の原子性や永続IDは対象外。全列探索と任意長の定理はmodel-only、7つのPython fixtureだけがstrictである。

モデル検証は3,004ms / peak RSS 672,528KiB、探索・コーパス照合は532ms / 686,128KiBで、両方exit=0。各深さの探索本体はミリ秒計測で0msだった。[model計測](verification/model.json)、[search計測](verification/search.json)、[探索ログ](verification/search.log)を保存した。初回は生成器の予約語使用でコンパイルに失敗し、修正後に再検証した。これは反例件数に含めていない。

## 未選択methodの確保

[alloc_probe.rs](alloc_probe.rs)はHEADのdebugライブラリの公開 `analyzer::discover_targets` を呼び、500,000bytes以上の成功したalloc/alloc_zeroed/realloc要求を数える。fixture作成・runtime構築は区間外。約500KBの文字列をappendで囲み、最深部の加算だけを選択した。同じ長さのignoreを対照とし、8条件すべてで加算→減算の候補1件を確認した。

深さ1・8・16・32でappend側の累積要求量は約4・25・49・97MB、対照は約1MBだった。2回の実行で回数と累積量が一致した。[初回ログ](allocations.jsonl)、[再実行結果](allocations-debug.json)、[測定スクリプト](measure_allocations.py)、[詳細表と原因](issue-method-allocation.md)を保存した。

この値は累積確保要求量であり、同時保持量やpeak RSSではない。debugの時間差は小さく、速度改善率は主張しない。release rlibに直接linkするprobeはlinkエラーとなり、releaseの確保量は未確認。性能はLeanとの対応を設けず、独立した実測として扱う。#461で修正したlist/tuple helper以外に残る処理である。

## 再実行

HEADのdebug/releaseバイナリとdebug rlib、Lean、Python 3.14を用意し、過去の結果を保存するため新しい出力先を指定する。Leanの資源監視には子プロセスのRSSを読む権限が必要となる。

```sh
python3 docs/audits/2026-09-15-evaluation-order/verify_lean.py --output /tmp/order-proof-recheck
python3 docs/audits/2026-09-15-evaluation-order/replay.py --binary target/release/hoimin --output /tmp/order-release.json --full-run
python3 docs/audits/2026-09-15-evaluation-order/replay.py --binary target/debug/hoimin --output /tmp/order-debug.json
python3 docs/audits/2026-09-15-evaluation-order/measure_allocations.py --output /tmp/method-allocations.json
cargo test --offline -p hoimin-cli --test comprehension_named_bindings --test type_parameter_bindings --test lean_annotation_scope_oracle
```

関連する既存テスト27件（15+8+4）は成功した。replayは不一致をJSONへ記録するが、不一致だけでは失敗終了しないため、そのままCIゲートには使わない。Unicode識別子のNFKCとanalyzer timeoutも補助確認したが、新規Issueに相当する不具合は確認していない。例外による代入途中の停止、augmented assignment、任意のscope横断・動的hookの対応は未検証である。
