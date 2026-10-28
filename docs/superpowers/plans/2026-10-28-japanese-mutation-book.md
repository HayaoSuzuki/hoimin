# 日本語ミューテーションテスト教科書の作成計画

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** カバレッジが高くてもバグを見逃す技術者に向け、理論、手作業の実験、AIエージェントとhoiminによる実習を日本語で提供する。

**Architecture:** 正本は `docs/book/`。専用MkDocs設定で単独ビルドでき、個人サイトは固定コミットから同じディレクトリを取り込む。製品の英語リファレンスや公開allowlistは変更しない。

**Tech Stack:** Markdown、MkDocs Material、Python 3.14、pytest、coverage、hoimin 0.14.0。

**Spec:** 本会話で承認された「理論と実践」「人間向け出力を前提にしない」「コピー可能なプロンプト」「多少複雑な正しい実装と100%カバレッジの弱いテスト」「内部実装は一切説明しない」。

## Global Constraints

- 対象はテスト改善を求めるソフトウェアエンジニア。Pythonの基礎と単体テスト経験を前提にする。
- スコア最大化やCIゲートを目標にしない。仕様を独立した判断根拠とする。
- 生存、等価性、タイムアウト、実行失敗を区別する。
- hoiminリポジトリのoriginと個人サイトのoriginを変更しない。
- 既存の利用者変更をコミットに含めない。作業ブランチ以外にpushしない。

## Review Focus

- 丸め前の実測カバレッジが100%か。弱いテストが代表変異を見逃し、解答例が検出するか。
- コピーした教材がリポジトリ外の新規フォルダでも動くか。
- hoiminのJSONLを読者に読ませず、エージェントの根拠を評価できるか。
- 環境差で変わる候補ID、件数、出力を固定結果として説明していないか。
- 個人サイトへの取り込み元がhoiminの固定コミットで、公開先とpush先が明示されているか。

## Tasks

- [x] 1. 教材の仕様、弱いテスト、解答例を用意する。初期テストと改善例の成功、行と分岐の100%、代表変異での検出差を検証する。
- [x] 2. 12章、仕様、演習解答、用語、参考文献、コピー可能なプロンプトを執筆する。専用MkDocs設定と教材ZIPを用意する。
- [x] 3. 単独ビルドとリンク、教材ダウンロード、実hoiminによる改善前後の検証を行う。検証結果を記録する。
- [ ] 4. hoiminの作業ブランチをコミットし、個人サイトの作業ブランチに固定コミット取り込みとナビを追加する。個人サイト全体をビルドする。
- [ ] 5. 独立レビューで事実、教材、導線、公開設定を点検し、必要な修正を行う。認証が利用できれば各リポジトリの作業ブランチをそれぞれのoriginにpushし、ドラフトPRを用意する。mainへのマージは行わない。

## Execution record

- 2026-10-28: `origin/main` = `7c2a79168c8698f85cc90d7efd6bf16a183e08b3` をfetchして確認。作業ブランチを作成した。既存変更は `.gitignore` の `.venv` 追加。
- 2026-10-28: 個人サイトの `books` は書評欄のため、公開パスは `/mutation-testing/` とする。原稿はhoimin側にのみ保存し、個人サイトのビルドが固定コミットを取り込む。

- Verification: downloaded ZIP:9 initial /27 with solutions pass;15/15 statements,12/12 branches;5 defect mutants detected only after improvements;equivalent/permitted changes remain accepted.
- Verification: hoimin0.14.0 same compare_order candidate survived before and killed after, both baselines pass.
- Verification: standalone strict MkDocs build,770 local links/anchors,product documentation build,and16 existing documentation tests pass.
- Review: independent read-only review found no critical/important issues. Fixed empty-order exception in the proposed property and platform-dependent ZIP metadata.
