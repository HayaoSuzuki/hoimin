# 監査コード・ケースの正式な検証への移行

## 目的と設計

`docs/audits/2026-09-14-*` に保存していた検証を、通常CIで失敗を検出できるRustテストとLeanモデルへ組み込む。基準となる実装は #545〜#549 をマージした `1a74858085b69541b9e24c0ad64b49b2572b3124`、監査資料を保存したコミットは `9b8d539`。元の監査ログの対象は修正前の `f11013542ccd735ab9741b5079c0b39a517df256` であり、今回の成功結果と区別する。

候補可否と実行経路の期待値はLeanから生成する。監査用Pythonスクリプトの結果出力に依存せず、既存Rust adapterから公開CLIとCPythonを実行し、不一致をassertで失敗させる。性能は既存の操作数計数を使う。製品の動作を変更する必要はない。

実装は、原文対応の確認、モデル・生成器の移行、Rust adapter追加、CI・性能台帳登録、検証とOKF更新の順に行った。

## 入力と移行先

| 監査 | 原文入力 | 既存再利用 | 追加 | 正式コーパスの合計 |
| --- | ---: | ---: | ---: | ---: |
| 暗黙例外 | 3 | 0 | 3 | 9 |
| 具体型名の参照先 | 15 | 8 | 7 | 121 |
| スライスを含むtuple | 14 | 0 | 14 | 80 |
| 合計 | 32 | 8 | 24 | 210 |

[暗黙例外](../../../formal/HoiminOracle/corpus/implicit-finally.jsonl)、[型注釈](../../../formal/HoiminOracle/corpus/collection-annotation.jsonl)、[有効Python](../../../formal/HoiminOracle/corpus/valid-python.jsonl)をJSONとして比較し、旧186ケースの全フィールドとIDを保持したこと、重複IDがないこと、監査32入力のsourceがバイト単位で一致することを確認した。追加ケースの期待値も元監査と一致する。スライスは意味上の形状に既存の検証があったが、監査原文の関数定義と末尾カンマを含めた14入力を追加した。

[暗黙例外adapter](../../../crates/hoimin-cli/tests/lean_implicit_finally_oracle.rs)は9入力の公開planを検査する。新しい3入力について通常・KeyError経路の6観測をCPythonと照合し、最小反例は公開runでもbaseline成功、killed=0、mutants空、complete=true、元ファイル不変を要求する。0件実行は成功にならない。

[型注釈adapter](../../../crates/hoimin-cli/tests/collection_annotation_builtins.rs)と[有効Pythonadapter](../../../crates/hoimin-cli/tests/valid_python_corpus.rs)は既存の候補・評価・コンパイル検証を追加ケースにも適用する。

## モデルと定理

| 監査の性質 | 正式な移行先 |
| --- | --- |
| 暗黙例外入口の保持、任意の後続列、全入口の信頼、入口の重複 | [ImplicitFinallyModel](../../../formal/HoiminOracle/HoiminOracle/ImplicitFinallyModel.lean) |
| 許可にはbuiltinが必要、shadowed／unknownの拒否 | [CollectionAnnotationModel](../../../formal/HoiminOracle/HoiminOracle/CollectionAnnotationModel.lean) |
| sliceを先頭に持つ任意の要素列を拒否 | [ValidPythonModel](../../../formal/HoiminOracle/HoiminOracle/ValidPythonModel.lean)の `slice_is_rejected` |
| 旧ループ再走査の2^n、import数・注釈数の両軸を倍にするとコピー量4倍 | [PerformanceCostProofs](../../../formal/HoiminOracle/HoiminOracle/PerformanceCostProofs.lean) |

暗黙例外には、指定したhazardの実行入口が保守的な入口集合に含まれる定理を加えた。深さ0〜4の列挙はそれぞれ1／4／13／40／121列、列長の合計0／3／21／102／426、壊れたモデルとの相違0／0／1／5／18を感度検査で要求する。121列すべてをCPythonで実行したという意味ではない。

新モデル2件は[ライブラリ](../../../formal/HoiminOracle/HoiminOracle.lean)と[CI](../../../.github/workflows/ci.yml)に依存順で登録した。生成器3件のビルド・鮮度・感度検査は既存CIの対象である。plan／verify／progressの追加調査は既存RustテストとProgressDecisionモデルに対応しており、重複モデルを追加しなかった。

## 性能監査の入力

[操作数テスト](../../../crates/hoimin-cli/src/analyzer/rust/loop_transfer_tests.rs)に、監査と同じforループ深さ18／19／20と、import・int注釈数512／1,024／2,048を追加した。6入力は元の測定スクリプトが作るsourceと一致する。演算子選択・未選択の計12条件で候補、診断、truncationと訪問回数・clone回数・コピー要素数を検査する。

[台帳](../../performance/shapes.json)の `audit-promoted-546` と `audit-promoted-547` はactiveで、exact指定した各1テストの実行成功を確認した。旧監査の36回の時間・RSS測定は履歴として保持し、CIの時間閾値にはしない。既存の候補生成対照と旧再走査を有効にする感度テストも保持した。

## 実行した検証

2026-09-15、macOS、CPython 3.14.7、Lean 4.32.2で実施した。

- `cargo test --offline -p hoimin-cli --test lean_implicit_finally_oracle --test collection_annotation_builtins --test valid_python_corpus`: 227成功、既存ignore 3件。
- 同じ対象に `--features contracts`: 227成功、既存ignore 3件。
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`: 成功。
- `tools.performance_shapes.run_gate` で新規2ゲートを抽出して実行: 各1件成功。`tools/performance_shapes.py check`: 29形状の台帳検証成功。
- `.venv/bin/python -m unittest discover -s tests`: CIワークフロー・性能台帳・資源監視など92件成功。
- Lean: ライブラリの依存を順にビルドする91コマンド、変更した性能定理、3生成器のビルド・出力・鮮度・感度検査が成功。

Leanは既存resource guardで各コマンド20秒・RSS 2,048MiBに制限した。通常の並列依存ビルドがRSS上限を超えたため、依存順に1モジュールずつビルドした。上限を引き上げていない。最終ライブラリ検証の最大値は8.55秒・865,312KiB、valid-pythonの直列ビルドは3.336秒・807,328KiBだった。途中のLean証明・構文エラーと不足したoleanは修正・生成後に再検証した。

### 失敗を検出できること

新規runtimeテストをコーパス追加前に実行すると、期待する6観測に対して0観測で失敗した。保存されていた旧releaseバイナリのSHA-256が監査記録の `271bee25922fc981e81f16ab584c86209e41b8e4c0394ae8021617cbe4993408` と一致することを確認し、新しい正式コーパスの最小反例を渡すと候補1、公開runで誤ったkilled=1を再現した。現行バイナリを使う正式テストは候補0・killed=0で成功する。

## セルフレビュー

各段階を次の5回に分けて確認した。独立レビューでも、Windowsの起動方法、空実行防止、旧ケース保持、期待値とCI、性能計数の5観点に要修正事項はなかった。

| 段階 | 1回目 | 2回目 | 3回目 | 4回目 | 5回目 |
| --- | --- | --- | --- | --- | --- |
| 設計・実装計画 | 監査32入力を列挙 | 既存8入力の重複確認 | 期待値をLean生成に統一 | 製品変更の必要性を確認 | 既存followup検証との対応確認 |
| モデル・生成器 | 監査定理との対応 | 任意列と有限探索の区別 | 旧186レコード保持 | source原文とIDの検査 | 壊れたモデルの感度・資源制限 |
| Rust実装 | 空観測時の失敗 | subprocessの終了・timeout | baselineと誤killed検査 | 操作数リセットと両演算子条件 | OS依存パスと引数の確認 |
| テスト | コーパス追加前のRed | 旧バイナリのhashと反例 | default最終227件 | contracts最終227件 | exactゲート・Clippy・fmt |
| OKF・文書・PR | 現行と履歴を区別 | 9／121／80件の照合 | モデルと実装の保証範囲 | 出典hash・リンク・形式 | PRの主題と実際の差分の一致 |

## 限界

Leanの定理はモデルの仮定内の性質を示す。Python全構文や動的hook、import失敗、任意の実行時名前変更は証明していない。旧コストモデルの定理を現在のRust全体の性能上界へ一般化しない。今回の実行はmacOSであり、Windowsについてはコードレビューのみ。CIのLinux実行結果はPRのchecksを参照する。
