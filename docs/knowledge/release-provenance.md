---
type: Contract
title: GitHubリリースの由来証明
description: 検証済み配布ファイルと署名対象の一致、ソース同一性、公開停止と実機確認。
status: draft
sources:
- id: workflow
  resource: ../../.github/workflows/release.yml
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: modified
  sha256: 197b961819a9962c98b9442c0143d3ae9ac97169ae8118deec0d3b5e65f7c1f4
- id: validator
  resource: ../../tools/release.py
  revision: a6965910bbc745e74aeb539628b3e2dc66c2a9d3
  working_tree: modified
  sha256: 6799cf1bea1690e390e7bfa6b184ce0306105c655c95e8873b28c14b670c2184
- id: guide
  resource: ../releases.md
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: modified
  sha256: cab75b11fc1b7ca03f24c37b845344b839cfdfd3f636b9e94776ea42079a58d9
- id: review
  resource: ../reviews/2026-10-10-issue-750-release-provenance.md
  revision: c08574663a363228fed310fbf75157831b4a5669
  working_tree: modified
  sha256: 70e05665ae643e033fadb37a5499b175cf5e8ddce881192b13fc783e3a4f809f
- id: hosted
  resource: ../reviews/2026-10-10-issue-750-hosted-verification.json
  revision: c08574663a363228fed310fbf75157831b4a5669
  working_tree: untracked
  sha256: ae58498410a4d1d5ce6918244e4af5b78667d82daba568e5c3502ed4943a9c8f
---

# 対象ファイルと公開条件

GitHub Releaseの由来証明は、archive 3、wheel 3、SBOM 6、SHA256SUMS 1の計13ファイルを対象とする。
validateが生成するimmutableな `verified-release` artifactを、証明jobと公開jobが同じrunから取得する。
両jobは完全なinventory、SBOM内の配布物digestとcommit、checksum一覧のバイト列を再検証し、入力を修復・再生成しない。[^workflow][^validator]

証明jobは公式Actionを完全SHAで固定し、contents read、id-token write、attestations writeだけを使う。
PRと手動previewでは署名・証明登録を実行しない。
生成したbundleで全ファイルを検証し、改変bytes、別repository、異なるsource SHAの拒否も確認してから公開を許可する。
検証・証拠保存の失敗は公開を止め、既存の公開済みReleaseを書き換えない条件も維持する。[^workflow]

PyPI向けには、同じビルドjobでwheelごとの単一subject証明も登録する。GitHub Releaseの13ファイルの一覧は維持し、公開時に新しいビルド証明を作らない。TestPyPIからPyPIへの公開条件と設定は[PyPIへのwheel公開](pypi-publishing.md)を参照する。[^workflow]

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

マージ後の実run `38052986762` でv0.3.4を公開し、証明書のsource/workflow identity、13ファイルの署名検証、4種類の拒否ケース、公開物とverified-releaseのbyte一致、SBOM内digestを確認した。
この実測のソースとworkflowのSHAはともに `c08574663a363228fed310fbf75157831b4a5669` だった。
証明書属性とdigestは[実測JSON](../reviews/2026-10-10-issue-750-hosted-verification.json)に保存した。[^hosted]
trigger、権限、公式Action、GH CLI、artifact取得、配布inventoryを変更した際も再確認する。[^guide][^review]

[^workflow]: [リリースワークフロー](../../.github/workflows/release.yml)。
[^validator]: [checksumとSBOMの検証](../../tools/release.py)。
[^guide]: [利用者の照合手順と再実行](../releases.md#github-build-provenance)。
[^review]: [Issue #750の検証記録](../reviews/2026-10-10-issue-750-release-provenance.md)。
[^hosted]: [v0.3.4の実証明書属性・公開物digest・拒否結果](../reviews/2026-10-10-issue-750-hosted-verification.json)。
