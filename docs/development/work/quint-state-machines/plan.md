# 状態機械検証拡張の実装計画

> 実装は領域別に進め、テストと本番対応を同時に確認する。`superpowers:executing-plans` または独立領域の分担で実施する。

**目的:** 予算・レポート・完了台帳の3領域を同じ資源制限付きのモデル検査と実装照合へ接続する。
**設計:** [design.md](design.md)。Quint 0.33.1、Apalache 0.62.4 を介した TLC、Rust rstest/proptest、既存 libFuzzer campaign。

## 共通制約とレビュー項目

- 製品の API を増やさず、公開境界と内部 fixture を区別する。
- 各コマンド20秒・RSS 2048 MiB、TLC heap 1 GiB・worker 1、重いモデル検査は逐次実行。
- 1テスト関数1履歴、rstest の各展開を1ケースとする。生成入力も1履歴単位で縮小。AAA、if/for は driver 内に閉じ込める。
- 誤った namespace の曖昧な読み取り、無効入力後の隠れた状態変更、例外を負の対照成功とする処理、contracts 時だけ起きる panic、CI からの adapter 漏れを重点確認する。

## Task 1: 共通実行と予算の拡大

- [x] 複数モデル・指定シナリオの生成と鮮度拒否、負の対照の失敗伝播を動作テストする。namespace reader は失敗を確認してから修正する。
- [x] `tools/verify_quint.py` を登録済みの source/main/mutant/prefix の逐次実行へ拡張する。未導入、異常終了、freshness mismatch は失敗にする。
- [x] `formal/quint/budget.qnt` に workload 定数と BudgetExtended を追加する。
- [x] `crates/hoimin-core/tests/support/quint.rs` で namespace と ITF 数値を解釈する。異なる完全修飾名が同じ短縮名になる入力は拒否する。
- [x] 4回予約の履歴を生成し、`quint_budget.rs` で実装に再生する。

## Task 2: Report の公開境界

- [x] `formal/quint/report.qnt` に有限レポート状態、エラー分類、負の対照を作る。
- [x] `tests/support/report_model.rs` がイベント組立と独立 journal を所有する。`tests/quint_report.rs` は ITF 由来期待値との照合、`tests/report_model.rs` はリテラルと PBT を扱う。
- [x] `fuzz/fuzz_targets/report_lifecycle.rs` と seed を既存 campaign へ登録する。モデルを production parser で期待値化しない。
- [x] 実装の不正な終了許可・sequence 判定を隔離コピーで変え、対応する検査の失敗を確認する。

## Task 3: Completion の内部台帳

- [x] `formal/quint/completion.qnt` に issue/complete/retire と正負の対照を作る。
- [x] `src/machine/quint_completion.rs` を test-only child module にし、実際の台帳メソッドを操作する。phase 遷移の全体証明とは呼ばない。
- [x] セット・履歴形式の参照と compressed floor/gaps の照合、リテラル、PBT、ITF replay を実装する。
- [x] retired 優先判定と gap 連結の実装 mutation を検出する。

## Task 4: 統合とコミット

- [x] CI に全 adapter の実行を含める。新規 fuzz target の campaign テストを更新。
- [x] 設計・計画・実装・テストの各5回レビューを記録し、software-design-philosophy の失敗診断を修正する。
- [x] 正負の TLC、fixture 再生成一致、対象テスト（通常/全 features）、Python/Rust 全体、必要な lint、実 fuzz を確認する。
- [x] `test/quint-state-machines` に拡張分をコミットし、清潔な作業状態と commit ID を確認する。
