# Leanの活用方針と既存資産の再確認

[監査全体へ](README.md)

対象HEAD: `623dd808612dbc34775e16814845eec0bc52dff9`。2026-09-11、macOS arm64。`lean-formal-audit` / `lean-test-oracle` の手順に従って、既存モデル・対応テスト・今回のIssueの関係を確認した。

## 活用する中心

Leanを、同じ契約を複数の実装経路で確認するための期待値の生成元にする。

```text
契約と前提の対応表
  → 小さいLeanモデル
  → 最短反例・意図的に壊した規則の検出
  → 必要な不変条件のモデル上の証明
  → Lean生成のversioned corpus
  → Rust / 実CLI / SQLite / Python / native backendとの対応確認
  → match / mismatch / infrastructure errorの記録
```

既存の証明を増やすだけでなく、証明済みモデルの入力前提を実装が本当に満たしているかを検証する。今回の新規モデル・adapterはまだ実装していない。ここで実行確認したのは後述の既存の終了判定モデルとcore adapter。

## 具体例: #483が既存の証明を通り抜ける理由

`MutationScoreExitPolicyProofs.lean:172` の `composed_complete_iff` は、completeの必要十分条件をモデル上で証明している。error/timeout/OOM/process-limit/not-runやrun-level failureがあればcompleteにはならない。

既存の生成コーパスにも `summary_timeout` があり、timeout 1件は `expected_complete=false`、`expected_exit_code=4`。core adapterはsummary/exit関数をこの期待値へ対応させる。

しかし `ProgressDecisionModel.lean:17` は `Report.usable mutants` または `Report.unusable` を入力として受け取る。raw JSONのcomplete/counts/exitが矛盾しないか、JSON readerが適切にusableへ分類したかは、そのモデルに入る前の条件。

今回の#483の最小再現では、killed 1件・timeout 1件の実レポートのcompleteだけをtrueへ変更すると、readerがusableと判定し、自己比較がsaturatedになる。既存の完了規則とreader分類の間が接続されていない。

必要な追加は次の契約。

- raw reportの意味的整合性を表すpredicate。
- 正当なincompleteと、矛盾した入力を区別する結果。
- 不整合な入力をusableへ分類しない接続条件。
- readerからcomparisonへ渡す際に、不正入力をstall連鎖へ混入させない条件。

これは「現在のLeanモデルがraw readerの不具合を既に証明した」という意味ではない。既存規則を再利用して、この境界のモデルとpublic adapterを追加する計画。

## 対応worksheet

以下のmodeは**追加するcaseの計画上の分類**。今回の既存モデル再確認を除き、対応検証は未実装・未実行。

| 前提・観測 | Lean表現・再利用先 | productionへの設定 | 公開観測 | mode |
| --- | --- | --- | --- | --- |
| coherentなsummaryのcomplete/exit | MutationScoreExitPolicy、counts/flags | 現行core公開型 | summarize/exit結果。既存adapterあり | strict |
| 矛盾したcomplete/counts/exitの分類 | 新しいreader整合性predicate＋既存完了規則 | v2/v3 JSONファイル | progressのexit、usable、latest.state | strict |
| backendとheader/resumeのmode一致 | 新しいmetadata伝搬状態＋Session | 自然なincomplete session→resume | header/baseline/mutant/保存mode | strict |
| protected sourceとmetrics出力先の衝突 | 正規化済みidentity、check/commit | 同じ実パスでCLI起動 | 拒否、baseline未実行、元bytes不変 | strict |
| Windows aggregate/per-root制限 | Budgetの所有scopeと使用量 | native環境でjobs=2の有限負荷 | Job Object設定とprocess outcome | model-only |
| generic/walrusのsource/destination binding | AnnotationScope/BindingFlowの拡張 | 有効なPythonソース | analyzer候補＋Python runtime | strict |
| mapping literal keyの等値性 | 対応するliteral意味値とkey集合 | 有効なmatch fixture | 候補とCPython compile | strict |
| Unicode/BOM/newlineから実spanへの接続 | CandidateSpan＋位置解釈 | 同じ入力bytes | plan descriptor、共有validator | strict |
| 索引照会の操作数 | 新しいcost model | 所有する内部counter seam | 実際の参照/走査回数 | internal-fixture |
| 任意Natのcost/無制限trace | 数学的model、一般不変条件 | 実機の有限整数/資源へそのまま設定不可 | modelだけ | model-only |
| backend設定・観測・起動に失敗 | semantic observationなし | setup失敗 | エラーの記録 | infrastructure-error |

Windowsのcaseは今回の環境ではmodel-onlyから開始する。同じ前提を設定して必要なnative観測が揃った部分だけstrictへ昇格させる。OSの未実行をLean model成功で埋めない。

## 優先順とIssueへの反映

| 順序 | 対象 | Leanで検証すること | 残す実物の検証 |
| --- | --- | --- | --- |
| 1 | [#483](https://github.com/tokyogas-tech/hoimin/issues/483)、#460 | reader適格性、complete/exit/statusの整合性 | v2/v3 raw入力→実progress CLI |
| 2 | [#490](https://github.com/tokyogas-tech/hoimin/issues/490)、#484/#487/#488 | 所有権、mode伝搬、再利用、aggregate予算の契約 | 実ファイル、SQLite、native backend |
| 3 | [#489](https://github.com/tokyogas-tech/hoimin/issues/489)、#481/#485/#486 | scope可視性、候補適格性、key一意性、span | CPython compile/runtimeとRust候補 |
| 4 | [#491](https://github.com/tokyogas-tech/hoimin/issues/491) | 抽象的な走査回数・保持量・増加の上界 | release time、peak heap/RSS、I/O、native stack |

#483/#489/#490/#491の本文へ、Lean再利用先、追加predicate、生成corpus、対応mode、broken variant、実物と照合する受け入れ条件を追記した。

## モデルと探索の範囲

新しいreaderモデルは、既存の7 status、空/単一/代表的mixed summary、complete/exit/baselineの意味的な境界から始める。条件はLean側で一度定義し、JSON expected valuesをadapterで手計算しない。

concurrencyではread/check/commitを別eventにする。最初から大きい全探索をせず、小さいworker数・identity数・trace深さで最短反例を固定し、必要な性質に到達したところで停止する。探索した深さ/alphabet/state/transition数と、任意長traceについてモデル上で証明した性質を別記する。

感度検証の例:

- completeを無条件に信用する、inconclusiveを無視する、exit優先順位を逆転する。
- validationの前に出力commitする、reused resultへ既定hardを補う。
- walrusをcomprehension-localにする、generic type parameterを無視する。
- 数値の等値性をkeyの文字列表記の一致へ置き換える。
- 1照会ごとに全binding履歴を再走査する。

atomicity、重複/再利用、境界/優先順位の適用可能なfamilyを確認する。適用不能なら理由を残す。モデルを実装の不具合へ合わせて弱めない。

## 今回の再確認結果

以下は既存 `MutationScoreExitPolicy` に限定した結果。新しいreader modelやRust全体の証明ではない。

| 確認 | 結果 | elapsed ms | peak RSS KiB |
| --- | --- | ---: | ---: |
| 既存Model/Proofs build | 成功 | 5,323 | 675,488 |
| 48-row corpus freshness | 成功 | 6,836 | 774,784 |
| broken variantsの感度 | 13/13検出 | 553 | 48,784 |
| core adapter | 4 tests成功 | test実行0.00s | 未計測 |

各Lean commandは単独実行、外部deadline 20秒、aggregate RSS上限768MiB（786,432KiB）、250ms sampling。上限変更、大きな追加探索、放棄した探索はなし。最大測定値は上限に近かったため、追加の探索規模を拡大していない。

既存corpusは42 strict rows（summary 10・Boolean policy 32）、internal composed 5、model-only exact fraction 1。このターンのcore adapterは公開summary/exitへの対応を再確認した。全caseが実CLIやOSを通ったわけではない。

実行コマンド（Lean commandのcwdは`formal/HoiminOracle`）:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 \
  --sample-ms 250 --stats /tmp/hoimin-lean-followup-proof-stats.json \
  -- lake build HoiminOracle.MutationScoreExitPolicyProofs
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 \
  --sample-ms 250 --stats /tmp/hoimin-lean-followup-corpus-stats.json \
  -- lake exe generate_mutation_score_exit_policy -- --check corpus/mutation-score-exit-policy.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 \
  --sample-ms 250 --stats /tmp/hoimin-lean-followup-sensitivity-stats.json \
  -- lake exe generate_mutation_score_exit_policy -- --sensitivity
```

repository rootで:

```sh
cargo test --offline -p hoimin-core --test lean_mutation_score_exit_policy_oracle
```

モデル/proof/corpus/production sourceは変更していない。scopeやmetadata、path identity、costを新たに証明したとの主張もしない。

## 仕様として確定が必要な点

- Windowsの公開契約をrun-wideのまま維持するか、per-rootとして変更するか。Leanは選んだ契約の帰結を示せるが、製品の契約を選ぶものではない。
- reused resource metadataはhistorical policyかcurrent policyか。#487の再現ではどちらもbest_effortだが、一般的な意味を定義する必要がある。
- dynamic class namespace等、静的に推定できないbindingをどの範囲でUnknownとするか。
- 不正JSONは拒否、正当なincompleteはunusableという境界を維持する。欠落したrun-level flagを推測してcompleteへ補正しない。

これらは各Issueで扱う仕様上の判断点であり、今回の既存資産の確認やIssue追記を止める理由にはしていない。
