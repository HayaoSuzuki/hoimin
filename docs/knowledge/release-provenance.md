---
type: Contract
title: GitHubリリースの由来証明
description: 検証済み配布ファイルと署名対象の一致、ソース同一性、公開停止と実機確認。
status: draft
sources:
- id: workflow
  resource: ../../.github/workflows/release.yml
  revision: a6965910bbc745e74aeb539628b3e2dc66c2a9d3
  working_tree: modified
  sha256: aed6b62488b3c90397739c36e793b1e143356163a9f6b50a2071c9a45a5c605b
- id: validator
  resource: ../../tools/release.py
  revision: a6965910bbc745e74aeb539628b3e2dc66c2a9d3
  working_tree: modified
  sha256: 6799cf1bea1690e390e7bfa6b184ce0306105c655c95e8873b28c14b670c2184
- id: guide
  resource: ../releases.md
  revision: a6965910bbc745e74aeb539628b3e2dc66c2a9d3
  working_tree: modified
  sha256: 076faf654acd7068709b049448c26f9c732d6c58864f5fb32b03cf87e2930b21
- id: review
  resource: ../reviews/2026-10-10-issue-750-release-provenance.md
  revision: a6965910bbc745e74aeb539628b3e2dc66c2a9d3
  working_tree: untracked
  sha256: d0ce58daecf505205b3322c341c48b4590af1a97d6282542e8e80cc07935518e
---

# 対象ファイルと公開条件

GitHub Releaseの由来証明は、archive 3、wheel 3、SBOM 6、SHA256SUMS 1の計13ファイルを対象とする。
validateが生成するimmutableな `verified-release` artifactを、証明jobと公開jobが同じrunから取得する。
両jobは完全なinventory、SBOM内の配布物digestとcommit、checksum一覧のバイト列を再検証し、入力を修復・再生成しない。[^workflow][^validator]

証明jobは公式Actionを完全SHAで固定し、contents read、id-token write、attestations writeだけを使う。
PRと手動previewでは署名・証明登録を実行しない。
生成したbundleで全ファイルを検証し、改変bytes、別repository、異なるsource SHAの拒否も確認してから公開を許可する。
検証・証拠保存の失敗は公開を止め、既存の公開済みReleaseを書き換えない条件も維持する。[^workflow]

# ソースとワークフローの同一性

prepareのcommit、checkoutのHEAD、イベントのsource SHAを一致させる。
source SHAとworkflow SHAは意味が異なるため、証明書の `--source-digest` と `--signer-digest` で別々に照合する。
workflow pathとrepositoryも利用者の期待値で制限する。
checkout操作だけでOIDCのsource identityが変わるとは仮定しない。[^workflow][^guide]

ビルド時に予約versionへmanifestとlockfileを調整する既存の処理は維持する。
証明はその処理を含むworkflowとソースを識別するもので、変更のない作業ツリー、再現可能ビルド、無脆弱性、OSコード署名を保証しない。[^guide]

# 検証証拠と再確認

ローカルの回帰、shell制御、厳格lint、変異検証と、各段階5回以上のセルフレビューは[検証記録](../reviews/2026-10-10-issue-750-release-provenance.md)に記載する。
ローカル試験でGitHub OIDC証明書を発行したとは扱わない。[^review]

マージ後の最初の実runで、証明書のsource/workflow identity、13ファイルの署名検証、拒否ケース、公開物とverified-releaseのbyte一致、SBOM内digestを確認する必要がある。
確認前に完了条件をすべて満たしたと主張しない。
trigger、権限、公式Action、GH CLI、artifact取得、配布inventoryを変更した際も再確認する。[^guide][^review]

[^workflow]: [リリースワークフロー](../../.github/workflows/release.yml)。
[^validator]: [checksumとSBOMの検証](../../tools/release.py)。
[^guide]: [利用者の照合手順と再実行](../releases.md#github-build-provenance)。
[^review]: [Issue #750の検証記録](../reviews/2026-10-10-issue-750-release-provenance.md)。
