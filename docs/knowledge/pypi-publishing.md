---
type: Playbook
title: PyPIへのwheel公開
description: 手動公開の条件、Trusted Publishingの登録値、検証と再実行の範囲を示す。
status: draft
catalog_revision: 36aa4cd696f4db28cd4811a0995d85df89b56eb5
sources:
- id: guide
  resource: ../pypi-publishing.md
  working_tree: modified
  sha256: 360d9a07d70ecffad7583352bfe8f266d858587aa9c9b8e665e3266121786c3e
  revision: 252654579792ede15081c8913e9b0ea59dd71a68
- id: workflow
  resource: ../../.github/workflows/publish-pypi.yml
  revision: 6ec69f4ac926c887003e5509bb393289f0bc9e71
  working_tree: modified
  sha256: 455ba3e258022c8131acb9b193b4daa5e9acd2b198bb5f66aa85855e758d39a2
- id: validator
  resource: ../../tools/pypi_release.py
  revision: 6ec69f4ac926c887003e5509bb393289f0bc9e71
  working_tree: modified
  sha256: 9b9000c2455426120e2be78994084014d26d2bc8eeaa840f08d6a6b01a05e895
- id: tests
  resource: ../../tests/test_pypi_release.py
  working_tree: modified
  sha256: 7ef177aeb7745b1dd32c8b87f8478490f2f3ae0daf4187273791aa938572004d
  revision: 252654579792ede15081c8913e9b0ea59dd71a68
- id: wheel-smoke
  resource: ../../tests/wheel_smoke.py
  revision: ff50918cb14ff39c67c0a594b665fd29d73080b4
  working_tree: modified
  sha256: 4132f96832912a24f6c7af14bff21fb53b3650f60d39c73aa4d1e9e56cf84369
- id: wheel-smoke-tests
  resource: ../../tests/test_wheel_smoke.py
  revision: ff50918cb14ff39c67c0a594b665fd29d73080b4
  working_tree: modified
  sha256: e8653a2473cc383c6c99911cc9f095beb3133fb6233f74c5e85ae71845f220ba
- id: release-workflow
  resource: ../../.github/workflows/release.yml
  revision: 6ec69f4ac926c887003e5509bb393289f0bc9e71
  working_tree: modified
  sha256: 35efb0eb0e4f31293bdd38edf9c9163795ba8d1e4d9f7635426bf047a194700e
- id: ci-tests
  resource: ../../tests/test_ci_workflow.py
  revision: 6ec69f4ac926c887003e5509bb393289f0bc9e71
  working_tree: modified
  sha256: 3fced57ffcda06eb7129821d19e10ef061348940ffa12cf253815498350879ee
---

# 公開の対象と手順

`HayaoSuzuki/hoimin` の `main` から `publish-pypi.yml` を手動実行し、公開済みの安定版タグと送信先を指定する。送信先の既定値はTestPyPIで、PyPIへの公開は別の実行で選択する。登録値とコマンドの正本は[公開手順](../pypi-publishing.md)に置く。[^guide]

準備ジョブは、タグのmainへの所属、GitHub Releaseの公開状態、3種類のwheelの名前・ハッシュ・メタデータ・ライセンス本文を確認する。検証後のwheelだけを公開ジョブへ渡し、公開ジョブにはソースのcheckoutやビルドを含めない。`id-token: write`はこのジョブだけに付与する。[^workflow][^validator]

# アカウント側の設定

PyPIとTestPyPIそれぞれに、所有者`HayaoSuzuki`、リポジトリ`hoimin`、ワークフロー`publish-pypi.yml`をTrusted Publisherとして登録する。Environmentは送信先と同じ`pypi`または`testpypi`を指定する。GitHub側で両Environmentにmainへのブランチ制限を設定する必要があり、ワークフローの追加だけでは有効にならない。[^guide]

必須レビュアーを利用できる場合は、公開ジョブの承認者を設定する。非公開リポジトリをGitHub ProまたはTeamで利用する場合、この機能は使えない。その構成では、手動実行後に検証が通ると、追加の承認待ちなしで公開する。[^guide]

# 検証範囲と再確認条件

GitHub ReleaseのPR previewは配布入力の変更時に選択し、手動実行では常に選択する。
previewのprepareはread権限とし、マージ済みcommitのタグ予約と公開だけをwrite権限にする。[^release-workflow]

GitHub Release用のビルドでは、PR検証と手動検証は実行イベントの`github.sha`、マージ後の公開は`pull_request_target`の`merge_commit_sha`を使う。PR更新時のペイロードには古い`merge_commit_sha`が入る場合があるため、検証対象の選択には使わない。checkout、後続ジョブへのコミット指定、同時実行のグループで同じ選択条件を用いる。[^release-workflow][^ci-tests]

ビルド後のwheelスモークテストは、`Elastic-2.0`と移管先のRepository・Issues・Changelog URLを確認してから、隔離環境へのインストールとCLI実行を検証する。メタデータの回帰テストでは、旧`MIT`ライセンスと旧組織のURLを拒否する。ライセンスや公開URLを変更するときは、この期待値も更新する。[^wheel-smoke][^wheel-smoke-tests]

ライセンス本文の比較は、Windowsのcheckoutで生じるCRLFとLFの違いだけを許容する。本文や空白の変更は拒否し、wheel自体のバイト列とSHA-256照合は変更しない。[^validator][^tests]

ローカルテストは、不足・破損したwheel、異なる版やライセンス、重複したメタデータ、無効なタグを拒否する条件を検証する。実際のOIDC認証、Environment承認、PyPIへのアップロードはローカル検証の対象外であり、初回TestPyPI公開時に確認する。[^tests][^guide]

ワークフローや検証コードを修正した場合は、mainへのマージ後に新しい手動実行を開始する。GitHubのRe-runは元のコミットを使うため、コード修正を反映しない。[^guide]

公開ジョブは既存ファイルを自動的にスキップしない。一部だけアップロードされた場合は、登録済みのファイルとハッシュを調べ、必要なら新しい版を公開する。workflow名、リポジトリ所有者、Environment名、Python対応範囲、wheelのプラットフォーム名、ライセンス本文を変えたときは、登録値と検証条件を読み直す。[^guide][^validator]

[^guide]: [PyPI公開手順](../pypi-publishing.md)。
[^workflow]: [手動公開ワークフロー](../../.github/workflows/publish-pypi.yml)。
[^validator]: [配布物の検証処理](../../tools/pypi_release.py)。
[^tests]: [公開準備の回帰テスト](../../tests/test_pypi_release.py)。

[^wheel-smoke]: [wheelのスモークテスト](../../tests/wheel_smoke.py)。
[^wheel-smoke-tests]: [wheel検証の回帰テスト](../../tests/test_wheel_smoke.py)。

[^release-workflow]: [GitHub Releaseのビルド・公開ワークフロー](../../.github/workflows/release.yml)。
[^ci-tests]: [CIワークフローの回帰テスト](../../tests/test_ci_workflow.py)。
