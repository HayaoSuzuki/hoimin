# Issue #491: 性能検証基盤の実装・レビュー記録

## 結果と範囲

8次元・11形状の台帳、7個の既存Rustテストを実行する通常ゲート、baseline/candidateのrelease計測器、通常CI・手動CI、手順書を追加した。基準は `165a2d284a1af92eb02ffd214ba8c0070c2f3808`。本PRは共通基盤の段階であり、10個の依存ゲートのactive化、追加のLean計数モデル、全8次元の漸近的な回帰阻止は未完了である。Issue #491を自動closeしない。

## 各工程のセルフレビュー

| 工程 | 第1回：要求・対応 | 第2回：境界・互換性 | 第3回：修正後の確認 |
| --- | --- | --- | --- |
| OKF | 台帳・設計・実行報告を別々の出典とした。 | 既存Lean証拠と今回実測の範囲を分け、pendingを未検証と記載した。 | 出典脚注・相対リンク・digestを照合し、カタログ入口から到達できる構造にした。 |
| 設計 | 8次元とN/2N/4NをIssueに照合した。 | 時間閾値、全体定数メモリ、未マージテスト成功という過大な主張を除いた。 | sampled RSSの未観測null、POSIX限定、guard固定費を明記した。 |
| 実装計画 | 通常テストのRED、gate、実CLI比較、Lean確認、PRをTaskへ対応した。 | 0件テスト、空候補、timeout、監視失敗の検査を追加した。 | 一時入力と証拠保存先の所有、既存出力の上書き禁止、依存PRの昇格条件を確認した。 |
| 実装 | registryとfixture、CLI契約の対応を読み直した。 | 独立レビューで初期化失敗のartifact欠落とRSS集計欠落が見つかり修正した。 | 追加レビューでtimeoutログ消失、失敗CIのartifact不保存、top1予備planのN件未検証を修正し、独立再レビューで残る指摘なしを確認した。 |
| テスト | missing moduleのRED後に実装し、invalid registry、0件一致、誤候補数を検査した。 | 全11形状の198回実CLI実行で出力意味を検査し、最終変更後にtop1の18回を追試した。 | 限定変異でサイズ1正常系の不足を発見した。境界テスト追加後に同一IDをkilledにし、通常テストを再実行した。 |
| PR | diffの対象をPython計測器、workflow、文書に限定し、Rust本体の変更を混入させないことを確認した。 | 本文で7 active/10 pending、既存Leanのみ、時間閾値なしを明記した。 | base=main・専用branch・変更ファイル・本文の依存関係を公開前に照合した。公開後のURLとremote照合は計画書へ追記する。 |

独立レビューは別agentが行った。最終の3指摘について修正差分を再確認し、残るblockerなしとの報告を受けた。上表のセルフレビュー回数とは別の確認である。

## テストと実測

- Pythonの計測器テスト13件、workflow契約28件。全Pythonテスト79件が成功した（8.733秒）。
- `python tools/performance_shapes.py gate`：7個のactiveテストを実行して成功。10 pendingは成功数に含めない。
- release比較：11形状×3サイズ×2実行ファイル×3反復=198回、すべて意味検証成功。集計を [measurement summary](../../performance/2026-09-14-measurement-summary.json) に保存した。完全なstdout/stderr/raw記録は実行環境の `/private/tmp/hoimin-491-measurements-final/`。最終top1追試18回も成功した。
- baselineは `165a2d2` からbuildし、candidateは #456 の `c6e6630`（Rust実装 `0a96b62`）からbuildした保存済み実行ファイル。パス名にissue-456を含むbaseline cacheを使ったため、パス名でrevisionを推定しない。双方のdigestを集計に記録した。
- macOS 15.7.7 arm64、Python 3.14.7、Rust 1.98.0。並列Rust buildとguard固定費を含むため、この比較だけで性能向上や漸近的な上限を結論しない。短命プロセスのRSSはnullとなる場合がある。
- Lean：既存 `generate_bounded_candidate_discovery` のbuild（20秒/2GiB上限）成功、10.918秒、809968KiB。`--sensitivity` は9種類の壊した実装を検出、`--check corpus/bounded-candidate-discovery.jsonl` は一致。Rust adapter `ordered_target_spools_match_the_lean_terminal_cases` も成功。モデル/corpusの内容は変更していない。最初に指定した `--check-sensitivity` は未対応のため失敗し、正式な `--sensitivity` で追試した。

## 限定ミューテーション検証

対象は `tools/performance_shapes.py:70-72`、operatorは `compare_order`、profileはCLI既定値、追加fingerprint入力なし。test argvは `/Users/hayao/RustroverProjects/hoimin/.venv/bin/python -m unittest discover -s tests -p test_performance_shapes.py`。jobs=1、workspace=8GiB、free reserve=10GiBを維持した。

完全なplanは2候補を含み、サイズ下限の `1 <= size` を `1 < size` にする `m1_1bc7c2e7d0de04c98e4e860cdc91d2cdb170386b26f004c39a4d3244785ab87c` のみを選択した。最初はsurvived。全形状でサイズ1の生成が成功し、Pythonとして構文が有効であることを確認するテストを追加した。通常テスト成功後、先の一時planをcleanup済みだったため同条件で再planし、同じIDを再検証した。結果はcomplete=true、killed=1、survived=0、exit=0。もう1候補は今回の下限契約調査の対象外として未検証であり、全体のmutation scoreや飽和は主張しない。各manifestは専用mktempとtrapで削除した。verifyの最終execution cleanupはcleanでdisk stopなし。

## 未完了の範囲

依存Issue #453/#456/#457/#461/#463/#470/#474/#475/#479/#482が統合された後、各テストの実名と計数点を確認してpendingをactiveへ変更する。現時点で全8次元を決定的な通常ゲートが覆うとは述べない。新しいbinding/writeコストのLeanモデルやN/2N/4Nの追加生成corpus、Linux/Windows実機の性能比較は本実行で済ませていない。
