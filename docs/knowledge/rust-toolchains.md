---
type: Playbook
title: Rustの固定版と最低対応版の更新
description: 最新stableへの追従、nightly検証、更新PRと配布確認の手順を示す。
status: draft
sources:
- id: development
  resource: ../development.md
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: 1f7b4ff1e18fa3f7d843f302b0f4b06fe0433f620c719acaa23cbb7f1b86e670
- id: pin
  resource: ../../rust-toolchain.toml
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: b2c19d86958ed6810741488db5cd352856f303c53bcf0ea774e72f94ee4d201e
- id: manifest
  resource: ../../Cargo.toml
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: 5d93c3c03282087fd84d1f0d5fe7d8ad766415b46aecd164b70e4df7420a0157
- id: ci
  resource: ../../.github/workflows/ci.yml
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: bd0a671109b8b6ec2f7b0c30194c82e78e256c0be89312b8be1c030efe017510
- id: fuzz
  resource: ../../tools/ci_fuzz.py
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: 4c3cbbb0ff22ca733bb63d2e015b9ad8e44404e4ca04b5653122313fc86a7e71
- id: contracts
  resource: ../../tests/test_ci_workflow.py
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: d789699a2ce636f18705bfd33d03be887cde4a6a73d2d4dd2813be58ab2e60a9
- id: renovate
  resource: ../../renovate.json
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: clean
  sha256: 76c3a8f9641ecffcbd61ff8c9c7f768e9f05060a0cec73ebd4f58048fd8789d4
- id: rules
  resource: ../../infra/github/Pulumi.yaml
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: c45f2337c5d61a266203a500654460fb59cba2c86f90bc1299993d94b9a7f458
---

# stableと最低対応版

hoiminの開発・通常CI・wheelビルドでは、`rust-toolchain.toml`の具体的なstable版を使う。`Cargo.toml`の最低対応Rustも同じ版に揃える。ソースからビルドする利用者にはその版以降を要求する。対応するビルド済みwheelを使う場合、Rustの導入は不要である。[^development][^pin][^manifest]

固定版と最低対応版の一致はworkflow契約テストで確認する。旧版コンパイラ専用のMSRVジョブを廃止し、固定版で品質検査とテストを行う。最新版を使うcanaryは、次の更新に必要な修正を把握するために残す。[^contracts][^ci][^development]

# 更新の手順

公式リリース一覧で最新版を確認し、固定版と最低対応版を同じPRで更新する。Renovateのrust-toolchain managerで固定版の更新を検知できるが、Cargoの最低対応版まで自動更新されるとは仮定しない。担当者が同じPRで揃える。通常の更新には、既存Renovate設定の公開後7日間の待機期間を適用する。[^development][^renovate]

整形・Clippy・全workspaceテスト・contracts付きテスト・Python側の品質検査を実行する。各OSのwheelビルドとsmoke testにはリリースworkflowのPRプレビューを使い、non-Linux CIを最終refに対して一度実行する。具体的なコマンドは開発ガイドを正本とする。[^development]

# nightlyと必須チェック

fuzzとテスト順序のランダム化には日付固定nightlyを使う。stable更新時にもnightlyでビルドできることを確認する。nightlyの日付を変える際は、CI、定期fuzz、実行スクリプト、キャッシュキー、契約テスト、文書のコマンド例を揃え、fuzzとランダム順テストを実行する。[^development][^ci][^fuzz]

ジョブ名の変更は`infra/github/Pulumi.yaml`の必須チェックにも反映する。このPulumi設定は`tokyogas-tech/hoimin`向けであり、`HayaoSuzuki/hoimin`の設定を管理するものではない。適用時は対象リポジトリと現行workflowを確認し、開発ガイドからリンクした運用手順に従う。[^rules]

# 検証範囲

このページはIssue #739で採用した更新方針を記録する。個別のRust版やOSでの実行成功を保証する資料ではない。各更新PRで検証結果と未確認事項を記録する。過去のMSRV設計書は当時の判断として残し、現行の更新手順にはこのページと開発ガイドを使う。

[^development]: [development.md](../development.md)。
[^pin]: [rust-toolchain.toml](../../rust-toolchain.toml)。
[^manifest]: [Cargo.toml](../../Cargo.toml)。
[^ci]: [ci.yml](../../.github/workflows/ci.yml)。
[^fuzz]: [ci_fuzz.py](../../tools/ci_fuzz.py)。
[^contracts]: [test_ci_workflow.py](../../tests/test_ci_workflow.py)。
[^renovate]: [renovate.json](../../renovate.json)。
[^rules]: [Pulumi.yaml](../../infra/github/Pulumi.yaml)。
