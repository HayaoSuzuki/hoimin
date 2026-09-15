# 追加監査: plan / verify / progress

> 保存時の位置づけ（2026-09-15）: 以下は修正前の `f110135` に対する監査記録である。#545〜#549は修正済み。現在のテストとの対応は[監査結果と正式な回帰検証](../../knowledge/audits/analysis-2026-09.md)を参照。
対象コミット: `f11013542ccd735ab9741b5079c0b39a517df256`

この追加調査では、既報 #545–#549 と独立した新しい不具合は確認できなかった。新規Issueは作成していない。実装全体の無欠陥を証明したという意味ではない。

## 調査対象

- `plan.rs`: manifestのヘッダー、候補ID・sequence、source fingerprint、要求された候補の再解析と比較。
- `progress/input.rs` と `progress/input/jsonl.rs`: summaryとmutant結果の整合、baselineの扱い、イベント順序、異なるrunや重複IDの排除。
- `progress/compare.rs`: 候補集合の一致条件、回帰と改善の優先順位、停滞履歴のリセット。
- `fingerprint_inputs.rs`: globとexact指定の重複除去、負のパターン、再検証。ファイル数とパターン数に応じた照合処理は残るが、今回その性能影響は測定していない。
- 解析器の式境界: generator引数、mappingのtupleキー、`not`の括弧保持などをコード上で確認。追加の失敗再現は得られなかった。

## 実行した検査

```sh
cargo test --offline -p hoimin-cli --test plan --test fingerprint_inputs --test lean_progress_decision_oracle --test lean_progress_input_oracle --test progress
```

終了コード0。fingerprint_inputsは28件、Lean decisionは3件、Lean inputは4件、planは72件、progressは72件が成功。planの子プロセス用fixture 1件は直接実行の対象外としてignored。

Leanの既存モデルを使い、次の3コマンドを直列で実行した。各コマンドには `tools/lean_resource_guard.py` による20秒・2048MiB・50ms間隔の監視を適用した。すべて終了コード0。ログと資源統計をこのディレクトリに保存した。

```sh
lake build HoiminOracle.ProgressDecisionProofs
lake exe generate_progress_decision -- --check corpus/progress-decision.jsonl
lake exe generate_progress_decision -- --sensitivity
```

感度検査は7項目すべてtrue。既存decisionコーパスは `strict` 17件、`internal-fixture` 6件、`model-only` 1件。既存inputコーパスは `strict` 282件。Rustテストの件数とコーパスのケース数は異なる。

## 対応関係と限界

今回新しいモデルは追加していない。既存の `ProgressDecisionModel` / `ProgressDecisionProofs` / `ProgressDecisionCases` と `lean_progress_decision_oracle.rs` の対応付けを利用した。`strict` は公開progress経路との照合、`internal-fixture` は内部比較関数との照合、`model-only` はモデル内だけの検査として区別する。証明の成立対象はLeanモデルであり、Rust実装全体ではない。

planとfingerprintは既存Rustテストおよびコード調査で確認した範囲に限る。全workspaceテスト、全非同期interleavingの探索、新規の性能ベンチマークは今回実施していない。既報の名前解決とスライス変異に由来する例は別Issueへ分割しなかった。製品コードは変更していない。
