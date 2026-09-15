# withの例外抑制とfinallyの解析コスト

対象HEAD: `5e631ef`。2026-09-15、macOS arm64、CPython 3.14.7、Lean 4.32.2。製品コードは変更していない。

## 結果

| Issue | 発見 | 証拠 |
| --- | --- | --- |
| [#556](https://github.com/tokyogas-tech/hoimin/issues/556) | withの例外抑制後に型注釈のimport由来を誤認する | Lean生成5入力、debug/release公開plan、CPython、公開run |
| [#557](https://github.com/tokyogas-tech/hoimin/issues/557) | 入れ子finallyを記録と転送で指数的に再走査する | 実装の呼出し関係、release計測、Leanコストモデル |

既存open Issueと関連closed Issueを確認して起票した。#545〜#549は修正済みであり、今回はwithの例外から正常継続への変換と、finally固有の二重走査を対象とする。独立した機能追加のIssueは起票していない。

## 契約と最小反例

契約は「型注釈の置換で使うimportは、その位置へ到達する全経路で同じ由来である」。`docs/development.md:492` 付近の全到達可能exitの合流規則に対応する。[モデル化前の対応表](correspondence.md)で公開入力・観測とモデルの境界を定めた。

`visit_with`は本体のnormal fallthroughを維持するが、本体の例外をcontext managerが抑制してwith後へ進む経路を合流しない。暗黙例外の追跡もfinallyがある場合に限定される。

```python
from contextlib import suppress
Sequence = set
with suppress(KeyError):
    hazard()
    from typing import Sequence
def record(value: Sequence[int]):
    pass
observed = record.__annotations__['value']
```

最小の危険なモデル反例は `[mayRaise, importTyping]`。初期custom → callの例外終了custom → suppressによる正常継続customとなる。callが成功するとimportでtypingになる。後続へ入る状態は `[typing, custom]` なので、typing変異の候補は0件が期待値。実際のplanは `Sequence[int] → list[int]` を1件生成した。classificationは **confirmed bug**。

## 実装との照合

[corpus.jsonl](corpus.jsonl)のソース、候補数、CPythonのtyping判定はLeanから生成した。手編集していない。各fixtureは一時プロジェクトで実行し、hazardに成功／KeyErrorの2種類を外から与えた。

- debug: 4 match / 1 mismatch。[全観測](replay-debug.json)
- release: 4 match / 1 mismatch。[全観測と性能測定](replay-release.json)
- 各binaryのCPython観測10件は全てモデルと一致。不一致はcall-before-importの候補数。
- import-before-call、通常importは1候補を保持。明示raise、custom-onlyは0候補で一致。
- releaseの公開runでは、元ソースがset[int]を保持するテストのbaseline成功後、不適切な候補がkilled=1、score=1.0、complete=true、exit=0に入った。今回の実行workspace cleanupはclean。

5fixtureは `strict`。照合対象は候補数と注釈のtyping判定であり、保存した全candidate descriptorに対する独立のLean期待値は作っていない。全イベント列と性能コストモデルは `model-only`。新規の `internal-fixture` はない。

## Leanの証明・探索・感度検査

[WithModel.lean](WithModel.lean)は意味とカーネル検査可能な証明、[WithMain.lean](WithMain.lean)は有限探索・感度検査・corpus生成の専用入口である。正式ライブラリのimportには追加していない。

証明した性質:

1. 任意長のイベント列におけるモデルruntimeの結果は、normalまたはraisedの到達状態に含まれる。
2. 抑制後の候補を許可するなら、モデルruntimeの観測はtyping由来となる。
3. 初期custom状態でcallから始まる任意の後続列では、抑制後の候補を拒否する。
4. 抑制しない変換は状態を保持し、抑制を2回適用しても1回と同じ状態となる。
5. 二重走査コストモデルのleaf訪問数は全自然数で `2^n`。1回転送モデルでは常に1。

イベントはimport成功・成功またはKeyErrorになるcall・明示KeyErrorの3種。typing/customの2値とnormal/raisedを扱う。import失敗、動的hook、例外型の細分、return/break/continue、async、無限実行は除外した。対称性による削減はない。

| 最大深さ | 全トレース | イベント出現数合計 | 壊したモデルとの差 | 危険な許可 |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 0 | 0 | 0 |
| 1 | 4 | 3 | 0 | 0 |
| 2 | 13 | 21 | 2 | 1 |
| 3 | 40 | 102 | 10 | 4 |
| 4 | 121 | 426 | 36 | 11 |

最短優先・安定順序で探索し、最初の危険な許可は深さ2の上記反例となった。保守的な空normalの拒否に由来する候補欠落も「差」に含まれる。モデルの全差を製品の不具合数とは扱わない。イベント出現数はユニーク状態数やRust内部の実行回数ではない。

感度検査は、抑制された入口の欠落、全候補拒否、抑制しない場合まで抑制する変換、二重走査の再導入を固定witnessで検出した。境界・優先順位のfamilyに対応する。原子性・transaction、永続IDの重複は対象のないfamilyとして除外した。抑制変換の冪等性はモデル定理として確認している。

## 性能

`apply_finally`は合流した入口でfinalbodyを走査し、その後`route_finally_entry`でも走査する。内側のapply_finallyは`record_annotations=false`でも最初の走査を省略しない。normal exitだけの入力でも入れ子ごとに二重走査となる。

CPythonでcompileできる人工fixtureを、深さ10〜20まで1段ずつ、各3回release planに渡した。全てexit=0、候補1件だった。

| 深さ | bytes | 中央値（秒） |
| ---: | ---: | ---: |
| 16 | 1921 | 0.0809 |
| 17 | 2140 | 0.1549 |
| 18 | 2371 | 0.2971 |
| 19 | 2614 | 0.5900 |
| 20 | 2869 | 1.1510 |

Rust訪問カウンタとの対応は未確認のため、コスト定理は `model-only` とする。時間は公開CLIでの独立した観測であり、正確な訪問回数や通常プロジェクトの平均速度を証明しない。修正では実際の操作数ゲートを追加することを#557に提案した。

## 検証コマンドと資源制限

repository rootから実行する。出力先には新規パスを指定する。

```sh
cargo build --offline -p hoimin-cli
cargo build --offline --release -p hoimin-cli
python3 docs/audits/2026-09-15-with-finally/verify_lean.py --output /tmp/with-audit-proof-recheck
python3 docs/audits/2026-09-15-with-finally/replay.py --binary target/debug/hoimin --output /tmp/with-audit-debug.json
python3 docs/audits/2026-09-15-with-finally/replay.py --binary target/release/hoimin --output /tmp/with-audit-release.json --bench --full-run
cargo test --offline -p hoimin-cli --test lean_implicit_finally_oracle --test lean_binding_flow_oracle --test lean_nested_try_flow_oracle
```

関連Rustテストは4+3+2=9件成功した。今回の新規mismatchはそれらの対象外だった。`replay.py`はmismatchを記録する監査用adapterであり、mismatchでも終了コード0となる。infrastructure errorは2となる。正式CIゲートへの昇格は修正時に行う。

最終Lean検証はモデル3254ms / peak 679888KiB、探索・感度・鮮度520ms / peak 680096KiB。[検証ログ](verification/search.log)と[モデル統計](verification/model.json)、[探索統計](verification/search.json)を保存した。各深さの探索計測はミリ秒分解能で0ms、RSSはコマンド全体のpeakである。各Lean処理は直列、20秒・2GiB・50ms監視、1スレッド、既定heartbeat上限を保持した。大きい探索や無制限の証明探索は行っていない。

最初はsandbox内のps禁止で監視がexit=126となった。これは `infrastructure-error` とし、監視付きコマンドのsandbox外実行で解消した。証明作成中の型エラーと探索ログの予約語使用も修正し、上記の最終検証は全てexit=0。意味的mismatchとは数えていない。

## 残る判断と限界

#556では抑制可否不明のcontext managerをどう保守的に合流するか、async・複数with item・各exitの回帰範囲を決める必要がある。#557では記録と転送の分離または結果の再利用を検討する。Leanはモデルを証明したもので、Rust実装全体を証明したものではない。

全workspace試験、全Leanライブラリ、Windows/Linux資源バックエンド、任意のPython構文は今回の対象外。新しいモデル・再現コード・記録は未コミットであり、この報告と[不具合Issue本文](issue-with.md)、[性能Issue本文](issue-finally-performance.md)から参照できる。
