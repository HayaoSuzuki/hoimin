---
type: Contract
title: Rust依存のライセンスと取得元
description: 両workspaceの拒否型ポリシーとbuild script差分レビュー。
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: policy
  resource: ../dependency-policy.md
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: untracked
  sha256: b5cc6a71b5f60ff821dabede82729651f391f5b2fed823acef5da68b5099677f
- id: config
  resource: ../../deny.toml
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: untracked
  sha256: f3558abe6b2875940b85f606023443c7bb0ee4f5e33e859342f9b6081841d789
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
