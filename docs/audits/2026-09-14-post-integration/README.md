# 統合後のLean監査・性能調査

> 保存時の位置づけ（2026-09-15）: 以下は修正前の `f110135` に対する監査記録である。#545〜#549は修正済み。現在のテストとの対応は[監査結果と正式な回帰検証](../../knowledge/audits/analysis-2026-09.md)を参照。
対象HEAD: `f11013542ccd735ab9741b5079c0b39a517df256`。実施日: 2026-09-14。環境: macOS arm64、CPython 3.14.7、Lean 4.32.2。未追跡の `.DS_Store`、`.idea/`、`.serena/` は変更していない。productionコードも変更していない。

## 判明した問題

| Issue | 内容 | 証拠 |
| --- | --- | --- |
| [#545](https://github.com/tokyogas-tech/hoimin/issues/545) | 暗黙の例外からfinallyへ入る経路を落とし、型注釈のimport由来を誤認 | Lean最小反例、debug/release CLI、CPythonの束縛評価、実runの不適切なkilled |
| [#546](https://github.com/tokyogas-tech/hoimin/issues/546) | 型注釈解析で入れ子ループの本体を繰り返し走査 | コストモデルの定理、release実測 |
| [#547](https://github.com/tokyogas-tech/hoimin/issues/547) | 文転送に全import状態の一時コピーが残る | ソース上のコピー経路、コストモデルの下界、release実測 |

3件とも再現手順、既存修正との差、受け入れ条件を付けて起票した。#547は完了済み#479の後続であり、snapshot保持による旧来の大量メモリ消費が再発したとは判断していない。

## 対象範囲

2026-09-11監査のHEAD `623dd808` から現在までの変更一覧と、既存open Issueを確認した。重点的に読んだのは、解析器の制御フロー・候補位置・文字コード、planの選択と順位、progressのJSON/JSONL読取り、出力とmetricsの保護、対象発見の索引である。

今回の新しい形式化は型注釈の例外経路と解析コストに限定した。44件の既存Rustテストも実行したが、全workspace試験、全Leanモジュール、全修正の組合せを実行したわけではない。Linux cgroup、Windows Job Object、DBの実クラッシュ、PID再利用、全非同期順序は今回の検証対象外。

## 構造的な主張と最小反例

主張は「finally内でtyping由来として置換する名前は、その場所へ到達する全経路でtyping由来である」。`docs/development.md` の制御フロー合流契約に対応する。[モデル化前の対応表](correspondence.md)に、前提、公開入力と観測、modeを記録した。

宣言された意図は、全到達可能exitで一致するimportだけを保持すること。実装は明示的なraise/return等のexitを記録する一方、通常の呼出しが例外になった入口をfinallyの合流へ渡していなかった。

```python
Sequence = set
try:
    try:
        hazard()
        from typing import Sequence
    finally:
        def record(value: Sequence[int]):
            pass
        observed = record.__annotations__['value']
except KeyError:
    pass
```

同じソースを外部から渡す2種類のhazardで実行すると、成功時のobservedは `typing.Sequence[int]`、KeyError時は `set[int]` になる。全入口でtypingという前提は成立しない。それでもCLIは `Sequence[int] → list[int]` の候補を1件生成した。

最小入力は2操作 `[mayRaise, importTyping]`。初期状態custom → callの例外入口custom、正常継続custom → import後typingで、finally入口は `[false, true]`。期待は候補0、実際は1。classificationは **confirmed bug**。

Lean生成の3fixtureをdebug/releaseそれぞれで再生した。各binaryで **1 mismatch / 2 match**。importをcallより前へ移した正例は1候補、明示的raiseの負例は0候補で一致した。[releaseの全観測](replay-release.json)と[debugの全観測](replay-debug.json)に候補descriptorとCPythonの結果を保存している。比較対象はこの主張に必要な候補数で、全descriptorについてLeanの独立期待値と照合したとは扱わない。

[実run結果](incorrect-mutant-run.json)では、外部hazardをKeyErrorにし、observedがset[int]であることを検査した。baseline成功、killed=1、score=1.0、complete=true、exit=0となった。typingを置換したつもりの候補が、実際にはsetをlistへ変え、スコアへ入る。再現方法は[#545の本文](issue-implicit-exception.md)にある。

## Leanで確認したこと

[AuditModel.lean](AuditModel.lean)は意味とカーネル検査可能な定理だけを持つ。[AuditMain.lean](AuditMain.lean)はライブラリからimportされない実行入口であり、有限探索とJSONL生成を行う。[corpus.jsonl](corpus.jsonl)はLean生成で、手編集していない。

イベントはimport成功、通常式の成功/例外、明示的raiseの3種類。束縛はtyping/customの2値に抽象化し、import自体の失敗、動的hook、実際の例外値は除外した。全順序を最短優先・安定順序で列挙し、対称性による削減はしていない。

| 最大深さ | 列挙トレース数 | トレース長合計 | 暗黙例外を捨てるモデルとの相違 |
| ---: | ---: | ---: | ---: |
| 0 | 1 | 0 | 0 |
| 1 | 4 | 3 | 0 |
| 2 | 13 | 21 | 1 |
| 3 | 40 | 102 | 5 |
| 4 | 121 | 426 | 18 |

トレース長合計はソースイベント数の集計で、ユニーク状態数でも全内部実行ステップの計数でもない。各深さを順番に処理した。最短反例は深さ2であり、固定witnessとしてモデルにも残した。

カーネル検査済みの定理:

- 例外を起こし得るcallは、直前の束縛をfinallyの入口に残す。
- custom状態からcallで始まる任意長の後続列では、全入口typingにはならない。
- eligibleなら全入口がtypingである。入口の重複はtrustを変えない。
- 空の状態で各ループが本体を2回走査するコストモデルでは、最深部の訪問数は `2^n`。
- 注釈ごとにimport全体を1回コピーする下界 `I*A` は、両入力を2倍にすると4倍。

これらはLeanモデルの定理。Rust全体に対する無欠陥証明ではない。有限探索の121トレース全てを実CLIで実行したわけでもない。

感度確認: 暗黙の例外入口を捨てる壊したモデルが、固定反例でfalse→trueの誤判定をすることを検査した。importとcallの順序を逆にした正例も検査した。transactionの原子性や永続IDの再利用はこのモデルに存在しないため、その2リスク群の壊したモデルは対象外。入口重複の冪等性は別の定理として確認した。

対応分類: 新規3fixtureはstrict。121トレース全体と性能コストのRust内部計数はmodel-only。今回、新規internal-fixture adapterは作っていない。setup失敗は後述のinfrastructure errorとして分離した。

## 性能の実測

[measure.py](measure.py)で、同HEADのrelease binaryを使って2入力形状×3サイズ×2演算子設定×3反復、計36回を測定した。[全36回の結果](performance.json)にraw時間、サンプルRSS、終了、候補数、入力bytes、binary SHA-256を保存した。全件exit 0、候補0、truncated=false。

| 入力 | サイズ | 型解析ありの中央値ms | 型解析なしの中央値ms |
| --- | ---: | ---: | ---: |
| import数=注釈数 | 512 | 43 | 44 |
| 同上 | 1,024 | 129 | 44 |
| 同上 | 2,048 | 409 | 43 |
| ループ深さ | 18 | 182 | 42 |
| 同上 | 19 | 345 | 43 |
| 同上 | 20 | 649 | 45 |

時間は既存guardの監視起動・終了確認を含む。短い対照では固定費が支配的。RSSは10ms間隔のプロセスツリーのサンプルで、allocator peakではない。未観測値は `sampled_tree_rss_kib=null` と扱う。raw guardの `peak_rss_kib=0` を実メモリ0とは解釈しない。

各CLI実行を10秒・1GiBに制限した。ループは予備調査で6、8、10、12、続いて12〜18を確認し、本測定で18→19→20を一段ずつ増やした。予備調査にはlist注釈で候補数も増えるimport fixtureがあったため、本測定はint注釈・候補0へ変更した。予備60回と本測定36回を同じ結果として合算していない。

[#546の根拠](issue-loop-cost.md)は本体の再帰的な二重走査。[#547の根拠](issue-flow-clones.md)は文転送の全状態コピー。後者はピーク保持量を下げた#479と両立する。#491のcallback内cloneの既存ゲート4件は成功しており、callback外の処理量を追加で測る必要がある。

短いanalyzer-timeoutも[別途2条件](timeout.json)確認した。10ms/100msの設定に対し26ms/106msでexit 2と診断を返す。timeout不動作という不具合は認めていない。

## 実行した確認と資源上限

既存Rustテストは **44 passed、0 failed**。全workspace試験を実行したという意味ではない。

```sh
cargo build --offline -p hoimin-cli --bin hoimin
cargo build --offline --release -p hoimin-cli --bin hoimin
cargo test --offline -p hoimin-cli --test lean_progress_input_oracle --test lean_nested_try_flow_oracle --test source_encoding
cargo test --offline -p hoimin-core --test lean_report_sequence_oracle --test source_encoding --test candidate_policy
cargo test --offline -p hoimin-cli --lib performance_cost -- --nocapture
python3 tools/performance_shapes.py check
```

順にCLI integration 11件、core integration 29件、CLI内部cost 4件。性能台帳の29形状の整合性チェックも成功したが、全26性能ゲートを再実行したわけではない。

新規Leanの最終検証は以下で再現できる。出力先は未使用の絶対パスを指定する。

```sh
python3 docs/audits/2026-09-14-post-integration/verify_lean.py --output /tmp/hoimin-lean-recheck
python3 docs/audits/2026-09-14-post-integration/replay.py --binary target/release/hoimin --output /tmp/hoimin-replay.json
python3 docs/audits/2026-09-14-post-integration/measure.py --binary target/release/hoimin --output /tmp/hoimin-perf-recheck
```

Leanは既存toolchainを使い、各コマンド20秒・2GiB、50ms間隔のguardで直列に実行する。最終実行はモデル検査2,844ms、探索・鮮度437ms。サンプル最大RSSはそれぞれ663,712KiB、682,512KiB。[model統計](lean-model.json)、[search統計](lean-search.json)、[探索ログ](lean-search.log)。無制限heartbeatやnative_decideは使わず、既定のLean heartbeat制限を残した。より大きな探索は実行していない。

既存progressモデルも、proof build、corpus鮮度、5種類の壊した分類の感度を再確認した。formal/HoiminOracleをcwdに、次の3コマンドをそれぞれguard付きで実行した。

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 50 --stats /tmp/hoimin-audit-progress-build.json -- lake build HoiminOracle.ProgressInputProofs
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 50 --stats /tmp/hoimin-audit-progress-check.json -- lake exe generate_progress_input -- --check corpus/progress-input.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 50 --stats /tmp/hoimin-progress-sensitivity.json -- lake exe generate_progress_input -- --sensitivity
```

## 実行上の失敗と未検証事項

初回のLean guardはsandbox内でpsを実行できず、34msでmonitor_error/exit126になった。許可されたsandbox外のプロセス監視で再実行した。最初の新規Leanコンパイルではroot指定不足による入力パスエラーがあり、明示的なroot指定で解消した。Rust試験も2回、packageまたはtest target名の指定誤りでcargo起動時に失敗し、存在する正しいtarget名で再実行した。これらはinfrastructure errorであり、意味の反例や成功数に含めない。自動承認による拒否はなかった。

所有者判断として必要なのは、#545で暗黙例外の束縛をどの粒度で保守的に近似するか、#546/#547で意味を維持しながらどの再解析・コピーを省くかである。いずれもIssueに受け入れ条件を記載した。修正・commit・PR作成は今回行っていない。
