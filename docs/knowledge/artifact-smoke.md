---
type: Contract
title: 配布archiveとwheelの実行検証
description: 検証済み配布物の外部実行、Linux ABIと公開停止条件。
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: guide
  resource: ../artifact-smoke.md
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: untracked
  sha256: 355a4196f5670079df227c369f537a06b555704af9aae07d6c87f2bb2e94b758
- id: workflow
  resource: ../../.github/workflows/release.yml
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: modified
  sha256: 1432888965f0e344e82875b96a59b2ffb06c6185d1c63a5cd710d2d44bf7ee1f
- id: archive
  resource: ../../tests/archive_smoke.py
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: untracked
  sha256: 448785c2eea428352cb841592c2e6b965ea2e660d505df227a622d382ebd4794
- id: wheel
  resource: ../../tests/wheel_smoke.py
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: modified
  sha256: 1613215c80e781d479eb1a3058b3e093af7b46d580c9671ffbccf707d9bed970
---

# 配布物からの実行

checksumとSBOMを検証したverified-releaseを別jobで取得し、checksumを再照合してから既存3platformのarchiveを実行する。
既知の通常ファイルだけを受け付け、binaryの内容を固定の一時pathへコピーする。
checkout外のfixtureを使い、別binaryへのfallbackを設けない。[^guide][^workflow][^archive]

wheelとarchiveで小さいmutation fixtureを共有し、version/help、JSON結果、元sourceの保全を確認する。
既存のhost wheel検証に証跡を追加し、archive jobで同じwheel検証を重複しない。[^wheel][^guide]

# Linuxの下限と公開条件

standaloneはUbuntu 22.04で実行し、要求GLIBC symbolを2.35以下として記録する。
wheelはdigest固定manylinux2014環境でCPython 3.14・glibc 2.17を確認し、auditwheelのABI情報と実行結果を別々に記録する。
全Linux環境を保証する検査ではない。[^guide][^workflow]

証跡は配布物に混ぜずCI artifactとして保存する。
archiveとABI jobの成功をattestationの必要条件とし、その証明を必要とする公開も失敗時は進まない。[^workflow]

image・配布layout・対象OS・fixture・公開依存が変わったら条件と証跡を再確認する。
実機で成功した範囲はPR/レビュー記録で確認し、文書だけから実行成功を推定しない。[^guide]

[^guide]: [実行検証の運用](../artifact-smoke.md)。
[^workflow]: [release workflow](../../.github/workflows/release.yml)。
[^archive]: [archive helper](../../tests/archive_smoke.py)。
[^wheel]: [wheelと共通fixture](../../tests/wheel_smoke.py)。
