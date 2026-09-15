---
type: Audit
title: 2026年9月の解析監査と回帰検証への反映
description: 修正前の監査証拠と#545〜#549の修正・正式なテストの対応を示す。
status: draft
catalog_revision: 1a74858085b69541b9e24c0ad64b49b2572b3124
audit_revision: f11013542ccd735ab9741b5079c0b39a517df256
sources:
- id: audit-post
  resource: ../../audits/2026-09-14-post-integration/README.md
  working_tree: untracked
  sha256: f163f5ff8fe76fcdf6ae807e6a5ec29b6af6a39ecefce89bc0ed9c1ede8ab6a7
- id: audit-builtins
  resource: ../../audits/2026-09-14-additional/README.md
  working_tree: untracked
  sha256: dd983964bea7840821b1d394739df065664e196cfc6eed8c927502038906acca
- id: audit-slices
  resource: ../../audits/2026-09-14-slice-tuples/README.md
  working_tree: untracked
  sha256: 8f72f9a1ea886adde140aded91197214bf5f8e42abaf17ce508f1ef2b77d7aaa
- id: audit-followup
  resource: ../../audits/2026-09-14-followup/README.md
  working_tree: untracked
  sha256: dcd21d616d07d195997b0c9cdf2b11e5f38f2298c00c65bb01840b5a71bebf43
- id: audit-integration
  resource: ../../superpowers/reports/2026-09-15-issues-545-549-integration.md
  working_tree: untracked
  sha256: deb7754fbd1acefc841e8d9b076c60da6aa3f34659409998498dc6f5ef86086e
- id: audit-promotion
  resource: ../../superpowers/reports/2026-09-15-audit-verification-promotion.md
  working_tree: untracked
  sha256: 3a3a79631919bc7f2d52b7e73c888345bf12050e7b5a2b198b7b24986b5191b1
---

# 修正前の観測と修正後の検証

2026-09-14の監査は `f110135` を対象とした。暗黙例外の入口欠落、ループ本体の重複走査、import状態のコピー、具体型名の誤認、スライスを含むタプルの変換を #545〜#549として報告した。保存済みの失敗ログはこの版の観測である。[^audit-post][^audit-builtins][^audit-slices]

5件の修正は2026-09-15にmainへマージされ、最後のマージコミットは `1a74858085b69541b9e24c0ad64b49b2572b3124`。以下は同コミットを基準に、PR #555で監査入力・定理・実行検証を追加した検証先を示す。監査32入力のうち8入力は既存ケースを再利用し、24入力を追加した。元の186ケースは内容を保持している。監査時のログは発見過程の証拠として残す。[^audit-promotion]

# 正式な検証への対応

| Issue・修正PR | 監査で検出した条件 | 正式な検証先 |
| --- | --- | --- |
| #545・[PR #552](https://github.com/tokyogas-tech/hoimin/pull/552) | importより前の暗黙例外からfinallyへ入る | [公開CLIテスト](../../../crates/hoimin-cli/tests/lean_implicit_finally_oracle.rs)、[Lean生成器](../../../formal/HoiminOracle/ImplicitFinallyAuditMain.lean)、[9ケース](../../../formal/HoiminOracle/corpus/implicit-finally.jsonl) |
| #546・[PR #554](https://github.com/tokyogas-tech/hoimin/pull/554) | 入れ子ループで本体走査を再帰的に繰り返す | [訪問回数・意味の比較テスト](../../../crates/hoimin-cli/src/analyzer/rust/loop_transfer_tests.rs)、[性能ゲート](../../../docs/performance/shapes.json)の `control-flow-reanalysis-*` 3件と `audit-promoted-546` |
| #547・[PR #553](https://github.com/tokyogas-tech/hoimin/pull/553) | import数と注釈数に依存する全状態コピー | [解析器内のコピー計数](../../../crates/hoimin-cli/src/analyzer/rust.rs)、[公開解析テスト](../../../crates/hoimin-cli/src/analyzer/rust_tests.rs)、[性能ゲート](../../../docs/performance/shapes.json)の `annotation-statement-transfer-*` 2件と `audit-promoted-547` |
| #548・[PR #551](https://github.com/tokyogas-tech/hoimin/pull/551) | 具体型名が再代入や型パラメータで隠れる | [候補とCPython評価のテスト](../../../crates/hoimin-cli/tests/collection_annotation_builtins.rs)、[Lean生成器](../../../formal/HoiminOracle/CollectionAnnotationAuditMain.lean)、[121ケース](../../../formal/HoiminOracle/corpus/collection-annotation.jsonl) |
| #549・[PR #550](https://github.com/tokyogas-tech/hoimin/pull/550) | tupleに含まれるSliceが不正なリスト式になる | [候補・コンパイル・import-only runのテスト](../../../crates/hoimin-cli/tests/valid_python_corpus.rs)、[モデル](../../../formal/HoiminOracle/HoiminOracle/ValidPythonModel.lean)、[生成器](../../../formal/HoiminOracle/ValidPythonAuditMain.lean) |

CIは[ワークフロー](../../../.github/workflows/ci.yml)からRustテストとLeanの生成器・鮮度・感度検査を実行する。性能ゲートの名前と実行引数は[性能台帳](../../../docs/performance/shapes.json)を正本とする。

スライスを含む有効Pythonコーパスは80件。暗黙例外の監査3入力には、Leanから生成した通常・例外経路の期待値をCPythonで照合する6観測と、公開 `run` の誤ったkilled計上を防ぐ検証を加えた。性能監査の6入力は演算子選択・未選択の計12条件で操作数を検査する。[^audit-promotion]

# 追加調査と証拠の限界

plan／verify／progressの追加調査では新しい独立した不具合を確認できなかった。plan・fingerprint・progressの既存Rustテストと、既存ProgressDecisionモデルの証明・コーパス鮮度・感度を確認した記録であり、実装全体の無欠陥を示さない。[^audit-followup]

統合検証報告は、5件を合わせた状態のRust 2,133件成功、Python 92件成功、Lean・性能ゲート・wheelの確認を記録している。これは過去の実行結果であり、文書を取り込んだことによる再検証結果ではない。[^audit-integration]

Leanの定理はモデルの仮定内で成立する。生成ケースとRustの一致、CPythonでの評価、操作回数の検査はそれぞれ対象が異なる。ループの性能上限を任意の状態変化へ一般化せず、macOS上の実行をLinux・Windows固有の資源管理の検証へ広げない。[^audit-post][^audit-builtins][^audit-slices][^audit-integration]

# 再確認が必要な変更

解析器の例外入口、ループの固定点、名前解決、tuple要素の判定を変更するときは、表の対応テストとモデルを確認する。元の再現入力を変更する場合は、当時の観測との対応が失われないよう新しい結果を別に保存する。資料の入口は[監査一覧](../../audits/README.md)。

[^audit-post]: [統合後のLean監査・性能調査](../../audits/2026-09-14-post-integration/README.md)。
[^audit-builtins]: [追加監査: 型注釈の具体型名の解決](../../audits/2026-09-14-additional/README.md)。
[^audit-slices]: [追加監査: 多次元スライスのtuple-to-list変換](../../audits/2026-09-14-slice-tuples/README.md)。
[^audit-followup]: [追加監査: plan / verify / progress](../../audits/2026-09-14-followup/README.md)。
[^audit-integration]: [Issues #545–#549 統合検証（2026-09-15）](../../superpowers/reports/2026-09-15-issues-545-549-integration.md)。

[^audit-promotion]: [監査コード・ケースの正式な検証への移行](../../superpowers/reports/2026-09-15-audit-verification-promotion.md)。
