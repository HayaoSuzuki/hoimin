# Quint 導入のセルフレビューと実行記録

2026-11-25。対象は main `242ed683` に対する作業差分。参考リポジトリは
`/Users/hayao/RustroverProjects/pytest-allpair/formal`。各工程を5回レビューし、
下記の修正と実行確認を行った。設計と実装の評価には software-design-philosophy
の8診断（責務、入口の単純さ、情報隠蔽、契約コメント、レビュー、知識の所有、
境界の説明、設計改善）を使った。

## 設計の5回レビュー

| 回 | 確認と判断 | 修正・確認結果 |
| --- | --- | --- |
| D1 | 既存の証明と対象を比較。算術を重ねるだけでは異なる検証手法を追加する効果が小さい。 | Lean / Creusot と別に公開台帳の予約・解放の遷移を選んだ。 |
| D2 | モデル、実装 adapter、実行資源の責務を確認。終了制御の複製は変更箇所を増やす。 | 既存のプロセスツリーガードを再利用。製品へ検証用 API を追加しない。 |
| D3 | 有限性と実装への対応を再確認。小さな ID 上限を実装の枯渇と同一視すると誤った契約になる。 | 成功予約2回までの workload と明記。u64 枯渇と liveness は対象外。 |
| D4 | 実行資源と初回導入を確認。Quint の既定値は heap 8 GiB・worker auto。 | heap 1 GiB・worker 1・20秒・RSS 2048 MiB。CI のダウンロードは検査時間枠の前に置いた。 |
| D5 | 8診断で設計全体を再確認。モデルだけの成功では実装の検証にならない。 | ITF の直接再生を独立した必須チェックにし、入口と限界を README に集約。新設部分は8/8、10/10。失敗診断は残っていない。 |

評価は今回の検証構成に対する作者の判断であり、既存コード全体の評価ではない。

## 実装計画の5回レビュー

| 回 | 確認と判断 | 修正・確認結果 |
| --- | --- | --- |
| P1 | 公開 API と観測可能な状態を確認。内部 map の読み出しに依存しない計画が必要。 | reserve/release の結果、予約 ID、reservation、reserved、limits を使う。 |
| P2 | 検証の依存順序を確認。モデル検査と Rust 照合を区別する。 | typecheck → TLC 正常系・負の対照 → ITF 再生成 → 公開 API 再生の順。 |
| P3 | 検出能力を検証する方法を確認。正常系だけの実行では弱い。 | Rust の上限チェックを除去する隔離 mutation と、改変 ITF の鮮度拒否を実行する計画を追加。 |
| P4 | CI の選択と集約を確認。新 job を足すだけでは必須検査から漏れる。 | formal 選択へ接続し result の needs と selected_jobs に追加。選択テストを先に失敗させた。 |
| P5 | 冷えた環境、固定版、テスト工程を再確認。 | npm lock、action の SHA、公式 TLA archive の SHA-256、既存 action 許可リストと Rust job 分類まで更新。8診断を満たす入口と所有権を計画に記載。計画の最終評価は8/8、10/10。失敗診断は残っていない。 |

## 実装の5回レビュー

| 回 | 確認と判断 | 修正・確認結果 |
| --- | --- | --- |
| I1 | Quint の型・構文・有限遷移を実ツールで確認。 | 予約語 `exists` を局所変数に使った初稿を `found` に修正。typecheck と TLC を通過。 |
| I2 | pinned CLI の実際の既定値を確認。 | run の backend を typescript と明示し、別の Rust evaluator の暗黙取得を避けた。seed 固定で再生成一致。 |
| I3 | Java 終了時の guard 126 を再現。単に待つと RSS 監視が欠ける。 | 親終了後最大1秒の終了待ちと reparent された同一 process group の RSS 計上を実装。期限・RSS 超過の回帰試験で確認。 |
| I4 | オラクルの独立性と実装への対応を再確認。 | ITF の期待値を計算し直さない adapter と、全履歴 journal / u128 集計の PBT oracle を分離。実装 mutation を両経路で検出。 |
| I5 | CI、設定、依存、文書を再確認。 | TLA toolchain は公式固定 archive を検証して準備。生成物を ignore。8診断では新設部分8/8、10/10。既存 guard の lifecycle は一つの所有スコープを維持した。 |

## テストの5回レビュー

| 回 | 確認と判断 | 修正・確認結果 |
| --- | --- | --- |
| T1 | 手書きシナリオを独立ケースとしてレビュー。 | 失敗予約、二重解放、未知 ID、ゼロ量、種類独立を rstest で個別展開。テスト本体の手書き if / for を使わず AAA を記載。 |
| T2 | PBT の生成域と縮小をレビュー。 | 256ケース、最大127操作、0・小値・u64::MAX・MAX-1・任意 u64。mutation は「容量0で1を予約」の1操作まで縮小。 |
| T3 | fuzz の入力形式と制限をレビュー。 | 10バイト操作、最大256操作、3種類、繰返し解放と u64 境界の seed。既存 campaign の全体締切・RSS・timeout を継承。 |
| T4 | 偽の成功と失敗伝播をレビュー。 | 124/125/126・signal・設定失敗を負の対照成功にしない単体試験。隔離 ITF の ID を99へ変更すると実際の再生成検査が拒否。adapter 自身にも期待結果と予約量を改変する2ケースを追加した。 |
| T5 | 全体 suite と lint、既存契約への影響を再確認。 | 初回 Python は新 action と新 Rust job の許可リスト2件が失敗し修正。初回 Rust は既存 shell setup の6秒制限がタイムアウトし、並列数4で全体を再実行。最終結果は下記に記す。 |

## 実行の証跡

- `python tools/verify_quint.py`：型検査、TLC 正常系、過剰予約の負の対照、4履歴の再生成一致を確認。
- 公式 `apalache.tgz` v0.62.4 の SHA-256 は `8ac8488422c30d552735bae11b92a2d014d8e02e8608702acc08f9a2d8a9abb2`。GitHub release asset の digest と一致する実ファイルを隔離キャッシュへ展開して再確認した。
- TLC：6403生成、712到達状態、queue 0、エラーなし。負の対照は量3の予約で不変条件違反、Quint exit 1。正常系と負の対照の guard はいずれも `child_exit`。
- Rust 実装 mutation：隔離コピーで `check_budget_amount(...)?` を削除。Quint 由来4ケースすべて、リテラルの失敗予約ケース、PBT が失敗。通常のチェックアウトは変更していない。
- libFuzzer：nightly-2026-10-08、seed 1172、10秒指定、242607実行・11秒、異常なし。入力ごとの timeout 5秒・RSS 1024 MiB。生成 corpus とコミットする seed は分離した。
- 資源ガードの34テストが成功。親終了後の timeout / RSS 超過と残存プロセスの強制終了を含む。
- 独立レビューを試みたが、reviewer のツール host 不在で checkout を読めなかった。その後、提示した driver と guard のソース・モデル概要に限定したレビューでは重要指摘なし。checkout 全体の独立レビューを完了したという意味ではない。

新規 fuzz target は `rustfmt --check --edition 2024 fuzz/fuzz_targets/budget_ledger.rs` が成功。fuzz workspace 全体の format check は既存 `fuzz/fuzz_targets/python_reduce.rs` の未整形で失敗した。今回の機能変更対象には含めず、差分を作っていない。

Python 全体の最終実行：`python -m pytest -q` は2767成功・35スキップ（144.30秒）。

`ruff check`、`ruff format --check`、`ty check`、workspace の `cargo fmt --check`、新規 Rust テストの `cargo clippy -D warnings`、workflow lint、ドキュメントの strict build が成功。Rust 全体の再実行 `RUST_TEST_THREADS=4 cargo test --locked --workspace --no-fail-fast` は238 suite、18708成功・失敗0・62 ignored。初回にタイムアウトした shell setup テストも成功。その後、AAA の明確化と adapter の負の対照2件を追加し、対象13ケースを通常設定・contracts 有効の両方で再確認した。最終 adapter でも隔離した実装 mutation により4履歴と PBT が失敗することを再確認した。
