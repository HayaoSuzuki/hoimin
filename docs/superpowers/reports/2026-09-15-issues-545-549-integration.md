# Issues #545–#549 統合検証（2026-09-15）

各Issueのworktree・OKF・設計書・実装計画書・段階別レビュー記録は各PRに収録した。各段階で最低5回のセルフレビューを実施した。以下は5件を合わせた追加検証である。

## 対象

- #545: PR #552、efbba8a + e69fb31（共通CI前提 d523533）
- #546: PR #554、3262d77（共通CI前提 1942b5d）
- #547: PR #553、28d68ce（共通CI前提 f6a676b）
- #548: PR #551、85e0c51（共通CI前提 017afef）
- #549: PR #550、4783b114 + 4f84bd4（共通CI前提 51c9fd2）

統合用worktreeは `.worktrees/integration-545-549`、検証対象コミットは `24723e5`。各PRはmainを基点に独立して作成した。共通前提は、mainのCIイベントに既に存在するmerge_groupをテストの期待値へ追加する1行である。

## 検証結果

- `cargo test --offline --workspace`: 2,133成功、22既存ignore、失敗0（86 test suites）。hoimin-cliをcleanしてから再ビルドし、他worktreeのキャッシュ混入を避けた。
- `cargo test --offline -p hoimin-cli --features contracts --lib analyzer`: 231成功、3既存ignore。
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` と `cargo fmt --all -- --check`: 成功。
- Python自動テスト: 92成功。プロセス観測に必要な権限を持つ環境で実行した。
- Lean: implicit-finally、collection-annotation、valid-pythonの各generatorのbuild、corpus freshness、sensitivityを全て確認した。
- 登録済みの追加性能ゲート5件: 全て成功し、それぞれ正確に1テストを実行した。
- release wheel: オフラインビルド成功。`tests/wheel_smoke.py`で隔離環境へのインストールと実行を確認した。
- release CLI: 外側のtry/finallyと二重ループ、break/continue/pass、import前後を組み合わせた6ケースで候補数を確認した。
- release CLI: module/class、for/while、型注釈の有無を組み合わせた深さ20のループ8ケースで候補数を確認した。

## 統合セルフレビュー（追加5回）

1. **変更の保存**: 5件の差分と統合差分を照合した。OKFのsources、Lakeのgenerator、CI、性能レジストリの登録を全て保持した。
2. **例外状態と所有権移動**: 暗黙例外の入口状態をstatement実行前に取得し、例外状態を合流させてからfallthroughをtakeする順序を確認した。
3. **ループ再利用と例外経路**: 安定したtransferの再利用後にも暗黙例外が外側のfinallyへ届くことを公開CLIの6ケースで確認した。
4. **測定の実体**: キャッシュをcleanした全体テスト、実行件数を検査する5性能ゲート、release実行を確認した。経過時間だけから計算量を断定していない。
5. **証拠と限界**: テスト件数・ignore・contracts・Lean有限モデル・wheelを区別した。独立した統合コードレビューでもblocking findingはなかった。Lean検証はPython全体の証明ではなく、ループの性能上限も任意の状態変化に対する普遍的保証ではない。

## CI追跡

PR #553の最初のランダム順序ジョブでは、変更していない `workspace::owned::tests::lease_only_deleting_root_from_final_marker_cleanup_is_resumed` が回収件数0/期待1で失敗した。同じテストは統合全体実行および単独再実行で成功した。対象ファイルに本PRの差分がないことを確認し、失敗ジョブを再実行した。原因を特定したとは扱っていない。最終CI状態は各PRのChecksを参照。
