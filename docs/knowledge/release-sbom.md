---
type: Contract
title: リリース配布物のSBOM
description: 配布物別Cargo依存グラフ、由来の記録と不完全なリリースを拒否する条件。
status: draft
sources:
- id: release
  resource: ../releases.md
  working_tree: modified
  sha256: fa3e61a63955b3c3cc2c2143a9f9d519335db26427ee28c5b6cacc02fb8132ea
  revision: 313b84f104be0b8db29586b2e18b2ac073fc8eef
- id: design
  resource: ../superpowers/specs/2026-10-10-issue-741-sbom-design.md
  working_tree: untracked
  sha256: 9025a3ca80f190d69b0df391bbb536df0304ad9948db1910420fab5bf7c4d6b9
- id: review
  resource: ../superpowers/reports/2026-10-10-issue-741-sbom.md
  working_tree: untracked
  sha256: 535aa03987354c2611f7b1573348ed59d920fbc11656c405d4d8b99b3f805257
---

# 配布物との対応

リリースは3 platformのstandaloneとwheelに対応する計6文書を公開する。CycloneDX JSON 1.5でCargoの通常・build推移依存を記録し、対象配布ファイルのSHA256、release commit、調整後Cargo.lockのSHA256、targetとfeaturesを併記する。Linuxのwheelはmanylinuxコンテナ内で取得し、standaloneのホスト環境と区別する。[^release]

# 公開条件と由来

必須SBOMの欠落、schema不適合、metadata不一致、graph参照不整合を拒否してからSHA256SUMSを生成する。PRと手動実行も集約artifactで検証する。既存の公開済release保護とPyPIのwheel限定アップロードを維持する。[^design]

vendored parserはupstream packageとVCS revision、Ruff修正由来、release commitで固定したpathとREADMEを記録する。Cargo由来のgraphはOSライブラリを含む完全なbinary inventoryではなく、build依存が配布binaryへ含まれるという主張でもない。[^release]

# 証拠と再確認条件

セルフレビュー、回帰テスト、Hypothesis、構造変異fuzz、Leanモデル証明と256ケースの実装照合を[検証記録](../superpowers/reports/2026-10-10-issue-741-sbom.md)に分けて記録する。Leanの証明はモデルの公開ゲートに限る。hosted CIによるWindows/Linux native buildと実際の公開はローカル証拠に含めない。[^review]

target、features、Maturin image、generator、schema、parserを更新した場合はbuild設定とSBOM metadataの対応を再確認する。[^release]

[^release]: [運用と取得方法](../releases.md)。
[^design]: [Issue #741 設計](../superpowers/specs/2026-10-10-issue-741-sbom-design.md)。
[^review]: [Issue #741 検証記録](../superpowers/reports/2026-10-10-issue-741-sbom.md)。
