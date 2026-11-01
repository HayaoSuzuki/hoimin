# Issue #549 実装計画

1. 専用worktreeのbranch、HEAD、差分、出典、書込み先を確認する。
2. analyzerのOKFに直接Sliceの適格条件を追記し、設計書と本計画を作る。
3. ValidPythonModelにtupleの許可モデル、証明、有限列挙と壊したモデルを追加する。ValidPythonAuditMainから既存schemaのcorpusを再生成する。
4. production未変更でvalid_python_corpus_correspondenceを実行し、余計な候補による失敗を保存する。
5. collect_tuple_literalに直接Sliceのguardを加える。visitorの子探索は維持する。
6. Lean freshness、全valid_python_corpus、import-only run、fmt/clippyを実行する。失敗は原因を特定して再確認する。
7. 証拠と限界を報告し、OKFの出典hash・設計/報告一覧を検査する。各段階の5回の自己レビューを記録し、commitとPR本文を準備する。

実行ログは /private/tmp/hoimin-549-* に分離する。最終検証では専用worktreeのtargetを使う（初期の共有targetから分離した）。期待値はLean生成元だけで定義し、生成JSONを手編集しない。

## 実装箇所と再検証

- `crates/hoimin-cli/src/analyzer/rust.rs`: `collect_tuple_literal` の既存適格条件へ `tuple.elts.iter().any(|element| matches!(element, Expr::Slice(_)))` を追加する。`walk_expr` を止めない。
- `formal/HoiminOracle/HoiminOracle/ValidPythonModel.lean`: 直接要素2種・長さ1〜3の14入力を列挙し、11の壊したモデルとの相違を検査する。
- `formal/HoiminOracle/ValidPythonAuditMain.lean`: 型付きの適格条件から25例の期待値を生成し、`--check corpus/valid-python.jsonl` で一致を確認する。
- `crates/hoimin-cli/tests/valid_python_corpus.rs`: 公開planの候補集合と元/変異後ソースのcompileを既存adapterで確認。追加import-only runはexit成功、baseline成功、候補0、killed0、complete、元ファイル不変をassertする。

最終コマンド: `cargo test --offline -p hoimin-cli --test valid_python_corpus -- --nocapture`、`cargo clippy --offline -p hoimin-cli --test valid_python_corpus -- -D warnings`、`cargo fmt --all -- --check`。
