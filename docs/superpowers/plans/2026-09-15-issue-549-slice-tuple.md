# Issue #549 実装計画

1. 専用worktreeのbranch、HEAD、差分、出典、書込み先を確認する。
2. analyzerのOKFに直接Sliceの適格条件を追記し、設計書と本計画を作る。
3. ValidPythonModelにtupleの許可モデル、証明、有限列挙と壊したモデルを追加する。ValidPythonAuditMainから既存schemaのcorpusを再生成する。
4. production未変更でvalid_python_corpus_correspondenceを実行し、余計な候補による失敗を保存する。
5. collect_tuple_literalに直接Sliceのguardを加える。visitorの子探索は維持する。
6. Lean freshness、全valid_python_corpus、import-only run、fmt/clippyを実行する。失敗は原因を特定して再確認する。
7. 証拠と限界を報告し、OKFの出典hash・設計/報告一覧を検査する。各段階の5回の自己レビューを記録し、commitとPR本文を準備する。

実行ログは /private/tmp/hoimin-549-* に分離する。Cargo cacheのみ親workspaceのtargetを使う。期待値はLean生成元だけで定義し、生成JSONを手編集しない。
