# #1172 reduce 仕様の Lean 監査と実装前オラクル

> 実装後の追記: `reduce` の製品実装と952行の内部判定adapterを追加した。[実装・検証記録](../../reviews/2026-11-19-issue-1172-reduce.md)を参照。以下は実装前監査の記録であり、当時の model-only 分類と未実装という記述を保存している。

## 実装後に追加した 128 ビット境界の検証

有効な短縮率 `0 < n < d` と正の測定値に対して、`pairPass_iff_threshold` は、一組の短縮率判定が `reduced ≤ ⌊(d − n) × original / d⌋` と同値であることを示す。定理の入力は自然数である。[Lean の証明](../../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecProofs.lean)

Rust の `pair_pass` には、Lean から生成した `u128` 上限付近の6入力を追加した。積が `u128` に収まらない場合も含め、閾値ちょうどの3入力と、1 tick 超過の3入力を個別に比較する。積を飽和させる対照と、128 ビットで折り返す対照は、どちらも超過した入力を誤って受理する。既存の状態遷移の深さは3のままで、コーパスは計958行となった。[生成ケース](../../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecCases.lean) · [Rust の照合](../../../../../crates/hoimin-core/tests/lean_reduce_spec_oracle.rs)

```sh
(cd formal/HoiminOracle && lake exe generate_reduce_spec -- --check corpus/reduce-spec.jsonl)
(cd formal/HoiminOracle && lake exe generate_reduce_spec -- --sensitivity)
cargo test --locked -p hoimin-core --test lean_reduce_spec_oracle --test reduce_policy
```

## 結果

`reduce` の採用判定と成果物保持をモデル化し、22 個の一般定理を Lean 4.32.2 の kernel で検査した。952 行の期待値付き JSONL オラクルを Lean から生成し、再生成との一致を確認した。意図的に壊した 14 種の判定をすべて検出した。

合意仕様のモデル化した中核について、矛盾は見つからなかった。ただし、監査で見つかった次の 3 点は、ユーザーの追加依頼に基づき [境界条件の補足](clarifications.md) として確定し、モデルにも反映した。検証の成功を「仕様全体にバグがない」という証明に読み替えてはいけない。

| 項目 | 小さい反例 | 確定した補足 |
| --- | --- | --- |
| 入れ子の保持指定 | 保持した inner の ID を残しても、outer の範囲を消すとコードが失われる | 保持定義と交差する除去範囲も禁止する |
| 率とゼロ時間 | 0% では等速を通せる。100% とゼロ tick では式だけが成立しうる | 有理数の率を `0 < n < d`、観測時間を正に限定する |
| 締切との同時完了 | 完了 tick と deadline が等しい場合、優先順位次第で結果が変わる | `completedAt < deadline` を要求し、同時なら停止を優先する |

入れ子の保持は Q10 の意味を実装するための規則である。率は `0 < n < d` とし、観測済みゼロ時間は測定不成立として停止する。未観測の枠は区別する。締切と同時の完成は停止を優先する。現在の生成行はすべて `contract=agreed`。初回監査の snapshot は履歴として残し、[補足後の仕様](specification-current.md) を別に保存した。

製品の `reduce` は未実装であり、Rust/Python 実装との一致は未検証である。本番コードの変更、ベンチマーク、実際のテスト削除は行っていない。全行の mode は `model-only` で、strict な match/mismatch は 0 件。

## 入力仕様と監査対象

- 元仕様: [Issue #1172](https://github.com/HayaoSuzuki/hoimin/issues/1172)。[固定した本文](specification.md) の SHA-256 は `94f1d12ee069e0f39cc6c6a8c42728968ab7dfa3a5711549ced219a07a8469e7`。
- 作業開始時の repository HEAD: `7b773e598917cb057592fb7b16e70f6a536d99c0`。新しい検証ファイルは working tree の変更である。
- 主張: 元の検出結果、保持指定、完全な反復測定、同じ実行条件を満たした案だけを認証し、締切前に認証した案のうち観測中央値の良いものを保持する。未確認の候補、停止後のイベント、再通知は不適切な案の出力につながらない。
- 設計評価: [software-design-philosophy による評価](design-review.md)。仕様設計は 7/8、8.75/10。実装変更から呼出側を守れることの実証は製品実装後に残る。
- 除外: Python の AST から実バイト範囲への対応、decorator の除去、構文・encoding の保存、実際の runner、OS の時刻・終了応答、cleanup、物理的な diff 書込み、将来の実不具合検出能力。

通常テストとミュータントの結果、各時間値、証跡の identity は入力として受ける。Lean はこれらの実行結果を予測しない。identity の一致は実バイト・環境・コマンドに結び付いた不変のハンドルであること、span は同じ元ソースの正しい区間であることを adapter の前提とする。自然数の演算は正確だが、将来の固定幅整数の overflow は別途防ぐ必要がある。

## 対応表

モデル着手前の詳細な [correspondence worksheet](correspondence.md) を保存した。

| 仕様上の観測 | Lean 表現 | 製品側での構成・観測 | 分類 |
| --- | --- | --- | --- |
| 対象・保持定義 | ID、retained/removed の分割、半開区間 | reduce の対象指定と実際の diff が必要。未実装 | model-only |
| 固定した検出結果 | 一意の selected IDs、完全な結果集合、ID ごとの outcome | 将来の before/after JSON。現行 verify を代用しない | model-only |
| 反復時間 | 3 tick の中央値、3 組すべての正確な有理数閾値判定 | 実測または開示した仮想時計 fixture が必要 | model-only |
| 証跡の対応 | source/plan/command/environment/timeout/variant の identity | 実バイトと固定条件への結付けが必要 | model-only |
| 締切・停止・成果物 | 論理時計、停止状態、認証済み best | 将来の orchestration と成果物出力。未実装 | model-only |

baseline の成功と成果物の認証は別である。宣言された Q17 は kill/survived の完全一致を要求するため、件数・score・元 kill の包含だけを比較する実装は不適合となる。Q22 は探索中の有望案と最終確認済みの案を区別するため、両者を同じ best スロットへ未検証で保存してはいけない。

## 一般定理の範囲

[モデル](../../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecModel.lean) と [証明](../../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecProofs.lean) に、次の 22 定理がある。

- `pairPass_strict`、`finalPass_strict`: 有効な率と正の tick のもとで、採用条件を満たす各測定が実際に小さい。
- `certified_protected`、`certified_exact`、`certified_identity`、`certified_three_pairs`: 認証が要求する保持・検出結果・identity・3 組の最終確認。
- `exactResults_lookup`: 選択した任意の mutant ID の結果が一致する。
- `certified_frozen_timeout`、`certified_original_kill`: 認証は凍結した timeout と元の非ゼロ kill を必要とする。
- `chooseBest_safe`、`chooseBest_minimum`、`chooseBest_idempotent`: 認証済み案の選択は安全で、一回の選択で両候補以下の観測中央値となり、同じ証跡の再通知で結果が変わらない。同率では古い案を残す実装上の規約を使う。
- `step_preserves`、`run_preserves`、`all_trace_artifact_safety`: 任意長のイベント列でも、保存した案は認証済みかつ締切前完成である。有限探索の深さ 3 に限定した主張ではない。
- `stopped_absorbing`、`deadline_no_publish`: 停止後は再開せず、締切以上の時刻に新しい案を publish しない。以前の認証済み案は保持できる。
- `kept_interval_not_removed`、`invalid_config_no_publish`、`measurement_error_stops`: 保持区間との交差、無効な設定、観測済みゼロ時間に対する認証・停止規則。
- `empty_span_no_overlap`、`overlap_symmetric`: 空区間と交差の基本性質。

`Safe` は「実際に Python ソースが保全された」という述語ではない。認証された入力証跡と締切を満たす案だけが内部の出力対象になる、という決定モデルの不変条件である。モデルの良い案が認証され、より速い認証済み案が選ばれる正の対照も実行しているため、常に拒否するだけのモデルでは検査を通らない。

`sorry`、独自 `axiom`、`native_decide`、無制限 heartbeat は使用しない。各定理の heartbeat 上限は 20,000。外部 consumer から全 22 定理を import し、依存公理は Lean 標準の `propext`、`Classical.choice`、`Quot.sound` の範囲で、`sorryAx` はないことを確認した。

## 有限探索と感度

[専用 executable](../../../../../formal/HoiminOracle/ReduceSpecAuditMain.lean) でだけ列挙・JSON 生成を行う。固定事例は [ReduceSpecCases.lean](../../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecCases.lean) に置き、標準ライブラリのルートには追加 import しない。

| 領域 | 件数と境界 |
| --- | --- |
| 認証 | 44 ケース。保持、組合せ削除、結果欠落・重複・新規 kill、各 identity、未確認、率、ゼロ時間、u64 超の時間 |
| 一組の時間比較 | 144 ケース。分子 0/1/20/21、分母 20、元時間 0/1/20/100、削減時間 0/1/19/20/94/95/96/100/101 |
| 検出結果 | 二つの ID、それぞれ killed/survived/inconclusive。before/after の全 81 組 |
| 中央値 | tick 1/2/3 の三つ組、全 27 組 |
| 区間交差 | 四つの端点を 0..3 から選ぶ全 256 組。空・逆向き区間を含む |
| 状態遷移 | 7 イベント、深さ 0..3 の全 400 列、延べ 1134 遷移。各中間状態も出力 |
| 合計 | 952 行。全行 `agreed`、mode は model-only |

イベントは、有効案、有効な高速案、最終確認未完了案、別 variant の証跡、観測済みゼロ時間、停止、締切と同時の完成。対称性削減は使用していない。順序は長さ優先・固定イベント順である。壊した早期 publish モデルの最短反例は、最終確認が二組しかない候補を通知する 1 イベント。空列は反例でない。

次の 14 種をすべて検出した。反例の入力、分類、対応状況、owner question は [counterexample ledger](counterexamples.md) にある。

1. 最終確認前の publish（atomicity）
2. 別 variant の証跡流用（identity）
3. 削減後に再計算した timeout の流用（provenance）
4. 結果の重複を件数で通す（uniqueness）
5. 元の kill 保存だけで新規 kill を許す（preservation）
6. 個別に削除可能な二定義を同時削除する（composition）
7. 外側の除去で内側の保持定義を消す（retention）
8. 一組の退行を中央値で隠す（timing）
9. 0% で等速を採用する（boundary）
10. 100% とゼロ tick を採用する（boundary）
11. 割引時間を先に丸めて等速を採用する（arithmetic）
12. 締切との同時完成で publish する（precedence）
13. 無効な短縮率の設定を受け入れる（configuration）
14. 観測済みゼロ時間を単に候補棄却として無視する（measurement）

## 実装前オラクルの使い方

[生成 corpus](../../../../../formal/HoiminOracle/corpus/reduce-spec.jsonl) の期待値はすべて Lean が計算する。schema は 1。認証、時間比較、結果集合、中央値、区間、状態遷移という六つの kind を持つ。future adapter の入出力・分類・追加の Python 編集試験については [consumer contract](adapter-contract.md) に定めた。

将来の adapter は実際の reduce の入口、または明示した内部 fixture に入力を渡し、実装の観測を期待値と比較する。実装相当の計算を adapter 内に再実装してはならない。構成できない前提は model-only のまま残し、strict に見せるために別の前提を使わない。

本作業では adapter を捏造せず、実装不在を成功試験に数えていない。crash、setup 失敗、timeout、時計制御不能は semantic mismatch と区別する。仕様補足は確定したが、実装との対応が構成できない行は model-only のまま残す。

## 再現コマンドと検証結果

repository root から実行する。各 Lean コマンドは直列とし、20 秒・2048 MiB のプロセスツリー guard を使う。

```bash
cd formal/HoiminOracle
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 100 --stats /tmp/reduce-build.json -- lake build +ReduceSpecAuditMain:o
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 100 --stats /tmp/reduce-check.json -- lake exe generate_reduce_spec -- --check corpus/reduce-spec.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 100 --stats /tmp/reduce-sensitivity.json -- lake exe generate_reduce_spec -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 100 --stats /tmp/reduce-stats.json -- lake exe generate_reduce_spec -- --stats 3
```

corpus を更新する場合だけ、同じ guard のもとで `lake exe generate_reduce_spec -- --output corpus/reduce-spec.jsonl` を使う。手で期待値を書き換えない。

検証結果:

- 対象モデル・22 定理・固定 witness・生成器のビルド成功。
- 無効な率と観測済みゼロ時間について、補足前の失敗と補足後の成功を確認。未観測の final pair 枠をゼロ時間と誤認しない例も保持した。
- 深さ 1 → 2 → 3 と一段ずつ増やして実行し、各段階の資源を確認。最終版は 400 列・1134 遷移、反例なし。
- 952 行の corpus freshness 成功。別の一時コピーに余分な行を加えると `stale reduce specification corpus` で拒否することも確認。
- 14 種の broken variant を検出。正の対照と、最短反例の長さ 1 も確認。
- 外部 Lean consumer の全 22 定理の import と依存公理確認成功。
- `git diff --check` 成功。CI の既存直列 module build と generator freshness/sensitivity に新しい対象を追加した。hosted CI 自体は実行していない。

## 資源と残る制約

[実測記録](resources.json) を保存した。初回監査の corpus 生成は 5.7 秒、観測した peak process-tree RSS は約 762 MiB。補足後の深さ 1 はコンパイルを含め 6.3 秒、約 758 MiB、深さ 2 と 3 は約 0.5 秒で完了した。初回監査と補足後の実行を記録内で区別した。サンプリング間隔は 100 ms であり、瞬間的な peak を完全には保証しない。

既存ライブラリ全体のビルドは RSS 約 3025 MiB で 2048 MiB の上限を超え、guard が停止した。残存子プロセスがないことを確認し、上限を増やして再実行していない。全体ルートへの不要な追加 import を除去し、対象モジュールと外部 consumer を検証した。したがって既存ライブラリ全体のビルド成功は主張しない。この事象と、sandbox による最初の監視不能は infrastructure error として記録した。

C1–C3 の仕様補足は確定した。ここで生成した同じ前提を実装で構成できる単位から adapter を接続する。Python 編集と OS 資源制御の実装対応は、それぞれ別の証拠が必要になる。
