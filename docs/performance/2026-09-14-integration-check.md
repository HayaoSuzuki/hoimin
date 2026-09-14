# 性能修正の統合確認（2026-09-14）

10件の実装修正を組み合わせたときの競合と動作を確認するため、`165a2d284a1af92eb02ffd214ba8c0070c2f3808` からローカルの専用worktreeを作った。各Issueの`crates/`差分だけを適用し、文書・CIは取り込んでいない。公開PRは独立したままであり、mainへのマージや統合branchのpushは行っていない。

| Issue | PR | 取り込んだ差分を持つhead |
| --- | --- | --- |
| #453 | [#533](https://github.com/tokyogas-tech/hoimin/pull/533) | `7b7192f8a2e1935c503bd9be1020e9b13f6b6858` |
| #456 | [#523](https://github.com/tokyogas-tech/hoimin/pull/523) | `d5fcfa29b377803f4389a9f5f848faa8937078d6` |
| #457 | [#522](https://github.com/tokyogas-tech/hoimin/pull/522) | `6cd8b989a282dcb18a2888e8f1a346ff4e387a96` |
| #461 | [#529](https://github.com/tokyogas-tech/hoimin/pull/529) | `11dd162101c4351dfdd15cb52ba112bdf1fe9cf2` |
| #463 | [#526](https://github.com/tokyogas-tech/hoimin/pull/526) | `33d9080ce4080b52cc9d6bbfb581d322b5c91e8b` |
| #470 | [#524](https://github.com/tokyogas-tech/hoimin/pull/524) | `55edef2f856d347b26db99d0cdb8ababf638abd9` |
| #474 | [#525](https://github.com/tokyogas-tech/hoimin/pull/525) | `c8847ba5241c379b10c20880bdd8e3a72d05e4da` |
| #475 | [#528](https://github.com/tokyogas-tech/hoimin/pull/528) | `15f0d54ea2b8a1eba4317b04649a3d7f7c46079b` |
| #479 | [#531](https://github.com/tokyogas-tech/hoimin/pull/531) | `1b16fc05855b14eb87eae52e2ccac03bf259b63c` |
| #482 | [#532](https://github.com/tokyogas-tech/hoimin/pull/532) | `58092b31994e43227e2292e6ba97f51874ea36e4` |

## 競合解消とレビュー

1. 解析器の #461/#470/#479 と、対象選択の #474/#475 で隣接するテスト・計測用宣言の追加が競合した。双方のhelper、counter、テスト、異なる名前のrelease計測を保持し、import順をrustfmtで整えた。productionの設計変更は必要なかった。
2. 別のレビューで、全10件が追加したRustテスト関数46件を抽出して統合treeと照合した。欠落は0件だった。#453最終headのtarget/fs.rsとtarget_handler.rsも統合treeとSHA一致を確認した。
3. 統合状態のfmt、Clippy、全workspaceテストを実行した。最初の実行は成功したが出力をファイルに保存していなかったため、同条件でログを保存して再確認した。結果と正確な件数は次節に記録する。

## 検証条件

ローカルworktree: `/Users/hayao/RustroverProjects/hoimin/.worktrees/performance-integration-check`。`CARGO_TARGET_DIR=/private/tmp/hoimin-target-build/issue-453`、`CARGO_INCREMENTAL=0`、jobs=2。Pythonは既存の3.14.7環境へのworktree内bridgeを使用した。

```sh
cargo fmt --all -- --check
cargo clippy -j 2 --workspace --all-targets --all-features -- -D warnings
cargo test -j 2 --workspace --all-features
```

最終のログ保存付き実行は、Clippy exit 0、全workspaceテスト exit 0。76件のtest resultを集計し、1,814 passed、0 failed、19 ignoredだった。ignoredを成功数には含めていない。

ログは実行環境の `/private/tmp/hoimin-performance-integration-clippy.log` と `/private/tmp/hoimin-performance-integration-tests.log` に保存した。統合状態の `git diff HEAD -- crates` のSHA-256は `e7406672f3ae38d8e9bf9876f7b2786b7e0505e00751a3cb50ad88769eda75a7`。実測時間やOS別の資源上限の証明とは別の確認であり、将来のmainや異なる競合解消結果の成功を保証しない。

#466は対象コード削除済みの記録、#491は共通検証基盤であり、今回組み合わせた10件のアルゴリズム変更には含めない。#491のpending gateは、この統合用treeだけで実行できることを理由にactiveへ変更せず、依存PR統合後の昇格を必要とする。
