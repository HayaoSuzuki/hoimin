---
type: Contract
title: Rust依存のライセンスと取得元
description: 両workspaceの拒否型ポリシーとbuild script差分レビュー。
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: policy
  resource: ../dependency-policy.md
  revision: 948cc626c0c649f3defa70a6b7b0997d65262f66
  working_tree: modified
  sha256: 00f4a2fdc1edf081158f4e35ed39ded9a357f36bf76a7e7d605c63e547735761
- id: config
  resource: ../../deny.toml
  revision: 948cc626c0c649f3defa70a6b7b0997d65262f66
  working_tree: modified
  sha256: 25b6404c2b676490b995f872907d7aeecab889b26cc4a6dcf202fbf6cca74e69
- id: workflow
  resource: ../../.github/workflows/dependency-audit.yml
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: modified
  sha256: baf4dd85d13e34ebf2f217cfa00a3a67914a41b380da2c6d5f3e284185731d15
---

# 検査範囲

通常workspaceとfuzz workspaceを、固定lockfile・全features・target絞込みなしで検査する。
通常・build・開発依存と他OS向け依存を含むが、全crateが全配布物に含まれるとは限らない。[^policy][^workflow]

# 許可と例外

未許可ライセンスと未知registry/git取得元を拒否する。
ELv2はhoimin自身の版範囲だけ、NCSAはlibfuzzer-sys 0.4.13だけに限定する。
ローカルpathはリポジトリ差分でレビューし、機械検査成功を安全性やライセンス義務の履行と同一視しない。[^policy][^config]

# build scriptの変更

custom-build targetの一覧を依存更新とともに再生成し、追加・削除・版更新をレビューする。
第一段階では手動差分レビューとし、独自解析器やbuild script拒否ゲートは作らない。
proc macroやscriptの内容の安全性は保証しない。[^policy]

依存・feature・toolchain・本体のminor版が変わったら、一覧と限定例外を再確認する。
脆弱性監査とSBOMは別の検査・証拠として維持する。[^policy][^workflow]

[^policy]: [依存ポリシー](../dependency-policy.md)。
[^config]: [許可設定](../../deny.toml)。
[^workflow]: [依存監査workflow](../../.github/workflows/dependency-audit.yml)。

The optional `contracts` 0.6.11 proc macro has an exact-version MPL-2.0 exception; the global allow-list remains unchanged. See [typed results and contracts](typed-results-and-contracts.md).[^policy][^config]
