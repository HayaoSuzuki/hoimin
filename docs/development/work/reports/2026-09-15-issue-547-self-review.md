# Issue #547: セルフレビュー記録

対象版: `f11013542ccd735ab9741b5079c0b39a517df256`。各行は順に実施した別の点検であり、テスト実行回数の代替ではない。

## Worktree（5回）

1. mainとorigin/mainのSHA一致を確認。古いbaseによる差分を避けた。
2. `git worktree list` で元checkoutを確認。未追跡の監査資料を修正対象へ含めない。
3. `.worktrees` がignoreされていることを確認。入れ子作業ファイルの誤追加を防ぐ。
4. `perf/issue-547` をorigin/mainから専用worktreeへ作成。他Issueの変更を含めない。
5. 専用worktreeでbaseline library試験を開始。結果と以降の回帰試験を区別して記録する。

## 設計（5回）

1. Issue本文と `KnownImports` の実cloneを照合。callback内だけの計数では不足するため全collectorを対象とした。
2. `visit_suite_flow` の状態の複製元・消費先を追跡。fallthroughだけをtakeし、abrupt exitの複製を保持する。
3. suite末尾の `self.imports` を参照する利用箇所を確認。末尾の一回のcloneを残す設計に修正した。
4. 関数・class復元と到達不能文を確認。既存のinherited状態とscope復元には変更を加えない。
5. COW・永続mapとの比較と保証範囲を確認。一般の分岐/固定点の線形保証と誤読しないよう単一路に限定した。

## 実装計画（5回）

1. 設計の各条件をTask 1〜3へ対応付けた。回帰・意味・資料・PRの順を明示。
2. red試験が実clone境界を観測するか確認。既存counterをresetして全collectorを囲む。
3. I/Aを同時にだけ増やす欠点を確認し、独立した直積の入力へ変更した。
4. 実装断片の型を実APIと照合。`Option<KnownImports>::take` と `std::mem::take` は既存Defaultに適合。
5. 文書一覧・出典hash・PRテンプレートの更新漏れを点検しTask 3へ列挙した。

## 実行記録

以降の結果は実行後に追記する。

## 実装（5回）

1. `git diff` でproductionの変更を確認。5か所のfallthrough返却とsuite内2か所に限定され、公開候補の構造を変更していない。
2. callbackの参照期間を確認。`record` と名前無効化が完了してからtakeするため、候補生成中のimport参照を空にしない。
3. `merge_abrupt` の消費を確認。fallthroughをtakeした残りを渡してもbreak・continue・terminateのVecは保持される。
4. suite末尾と到達不能文の復元を再確認。末尾cloneを保持し、abrupt返却時のself.importsを保持している。既存Lean内部対応試験を含むlibrary 637件が成功した。
5. `cargo clippy --workspace --all-targets --all-features -- -D warnings` とfmtの成功を確認。productionの新規依存・unsafe・snapshot蓄積がない。

## OKF（5回）

1. 既存analyzer概念に同じ話題を追記し、重複概念の作成を避けた。
2. 設計書・報告書を各原文一覧に登録。planはリポジトリ規約に従い原文一覧の対象外。
3. 参照時点のspec/reportをuntrackedとしてSHA-256を記録。将来のコミット番号を仮記入していない。
4. PyYAMLを用いた全20 Markdownの形式/予約ファイル検査と、今回の3出典の脚注・リンク・hash検査が成功。最終編集後にも再検査する。
5. 本文を設計・計数・意味検証の順に読み直した。単一路に限定したコピー上界と一般の性能保証を区別し、過去の監査結果を今回の成功へ読み替えていない。

## 検証の途中経過

- baseline library: 636 passed / 12 ignored。
- red: I=8/A=32で82 clone callsを検出し失敗。
- ownership移動後library: 637 passed / 12 ignored。
- 全collectorの独立I/A計数と選択/非選択の実候補: 2 passed。
- 最初のworkspace試験: worktreeの `.venv/bin/python` がなくanalysis_depth 7件失敗。既存CPython 3.14環境を参照するsymlinkを追加した。production不具合として扱わない。
- shared targetで複数Issueが同じCLIを上書きし得ることを点検し、APFS cloneによるworktree別targetに分離。最終証拠には分離後の結果を使う。

## テスト（5回）

1. redログのI=8/A=32で82 clone callsという実測を確認。空入力だけで失敗を示す初稿を修正し、非空の再現を先に検査した。
2. 新しい直接collector試験16組と公開analyzerの選択/非選択18組を照合。候補0だけでなく `list[int] → Sequence[int]` とsnapshot非保持をassertした。
3. 性能台帳の実行をPython側から検査し、prefix filterが既存のexact条件を満たさないことを発見。完全名と `--exact` を持つ2ゲートに修正した。
4. コピーしたCargoキャッシュに他worktreeのdefault library実行ファイルが残っていた。初回workspace成功の2100件は最終証拠から除外し、`cargo clean -p hoimin-cli` 後に追加テスト名を含む完全な再ビルドを実施した。contracts側は追加2テストの実行をログで確認した。
5. Python全体の失敗を分類した。mainでも再現するmerge_group期待値を共通の1行修正（f6a676b）で直し、psを使う監視はsandbox外で検証した。修正後のPython92件と監視単体5件は成功した。contracts一回目の既存cleanup待機の時間超過は再実行で解消し、変更していない。

## PR準備（5回）

1. 問題を単一路の状態コピーとして記述し、一般の分岐・固定点の計算量改善と混同しないことを確認した。
2. `origin/main` との差分には専用Issueの修正と共有CI期待値1行だけが含まれる。mainの未追跡監査資料、.venv symlink、ビルド成果物を含めない。
3. 回帰試験のRedとclean再ビルド後のGreenを区別し、他worktreeの実行ファイルを使った結果は最終証拠から除外した。
4. PRテンプレートのChange/Validation/OKFと、設計・計画・レビュー報告への参照を照合した。
5. source/entryのコピー計数とallocator/RSS/時間を区別し、未測定の数値や未公開のPR番号を仮記入していない。公開後にhead/base/状態をGitHubから確認する。

## 独立レビュー

別担当がdiff、設計、計画、全call siteを読み、重大な所有権・意味の指摘はなかった。軽微な指摘として性能台帳の2ゲートの入力軸説明が同文だったため、直接collectorの0/8/32/128と、実候補試験の8/32/128に追加import/注釈を含む条件を別々に記述した。

## 最終結果（2026-09-15、macOS arm64）

- `cargo clean -p hoimin-cli` 後の `cargo test --offline --workspace`: 84 suite、2102 passed / 22 ignored。libraryに追加2件が実在し実行されたことを確認した。
- `cargo test --offline -p hoimin-core --features contracts`: 319 passed / 2 ignored。
- `cargo test --offline -p hoimin-cli --features contracts`: 1768 passed / 20 ignored。追加2テストのdefault/contracts両方の実行を確認した。
- Python全体: 92 passed（資源監視のpsが必要なためsandbox外）。
- 追加性能ゲート: registryの読み込みと本番 `run_gate` 経由で2ゲート各1テスト成功。0件の見かけの成功を認めないチェックも通した。
- workspace/vendorのfmt、workspace all-targets/all-features Clippy、vendor library Clippy成功。

コマンドはworktree rootから実行し、`CARGO_TARGET_DIR` は同worktreeのtargetを指定。主なログは `/private/tmp/hoimin-547-workspace-clean.log`、`hoimin-547-cli-contracts-final.log`、`hoimin-547-core-contracts.log`、`hoimin-547-python-final.log`。ログの永続保存を前提にしないため、実行対象と集計を本書に残す。

新規Leanモデルは追加していない。既存Lean生成の分岐・loop・finally対応試験を上記Rust suiteで実行した。コピーの計数上界は追加fixtureの実Rust観測であり、形式証明やallocator/RSSの上界ではない。release/wheelの組合せ確認は5件の統合検証で別記する。
