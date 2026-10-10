---
type: Playbook
title: PyPIへのwheel公開
description: TestPyPI検証を経る自動公開、Trusted Publisher登録、元のビルド証明と再実行。
status: draft
catalog_revision: 36aa4cd696f4db28cd4811a0995d85df89b56eb5
sources:
- id: guide
  resource: ../pypi-publishing.md
  working_tree: modified
  sha256: 238002fb03b6ea9fc82392e9fb0d4215d66dd4361d9bee22f8e625700dcb8003
  revision: 3dbf6dfdb107cb83c1834fa8d3bf057e95243326
- id: validator
  resource: ../../tools/pypi_release.py
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: clean
  sha256: 9b9000c2455426120e2be78994084014d26d2bc8eeaa840f08d6a6b01a05e895
- id: tests
  resource: ../../tests/test_pypi_release.py
  working_tree: modified
  sha256: 16e2e267eeeab4738108301763f0c3e197aaaa90b600f12d5f91dbdb31cd0507
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
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
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: modified
  sha256: 197b961819a9962c98b9442c0143d3ae9ac97169ae8118deec0d3b5e65f7c1f4
- id: ci-tests
  resource: ../../tests/test_ci_workflow.py
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: modified
  sha256: 30a5e9e68cc2b9c3f15809a75eb74525dfb4c045dd41b8803c666e6d52aabd2d
- id: provenance
  resource: ../../tools/pypi_provenance.py
  revision: c9f92eb721d8a858a2f55643a5c72723763174dd
  working_tree: modified
  sha256: 478191ff23756d78670499c0127f06133b7ff7ce3a93f13392738a8108ea86d8
- id: provenance-tests
  resource: ../../tests/test_pypi_provenance.py
  revision: c9f92eb721d8a858a2f55643a5c72723763174dd
  working_tree: modified
  sha256: 210238c8d1caeaafef9a9acaea02cea43a0eb3c1a1e062e879f554626d92308f
- id: review
  resource: ../reviews/2026-10-10-issue-771-pypi-provenance.md
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: modified
  sha256: a35194bb515f91d4aeb1d0f1176b729cca04ab4ff55de1864820e5e7f127c499
- id: bootstrap
  resource: ../reviews/2026-10-11-issue-771-hosted-verification.md
  revision: 3dbf6dfdb107cb83c1834fa8d3bf057e95243326
  working_tree: modified
  sha256: e42ecdafe5fb25831f6652d8ba97d6517571b7ef0a0dbaf9c19f6a7b42b77263
- id: bootstrap-evidence
  resource: ../reviews/2026-10-11-issue-771-hosted-verification.json
  revision: 3dbf6dfdb107cb83c1834fa8d3bf057e95243326
  working_tree: untracked
  sha256: 3d2f452405473746b7dfe837c8ed00f21453e05fc3dc3476565b3d1a19b37883
---

# 公開の対象と手順

`PYPI_AUTO_PUBLISH=true`を設定すると、mainへのマージ後にGitHub Releaseを作成し、`release.yml`の公開専用runを自動起動する。TestPyPIへの公開と3種類のwheel・両証明の取得検証に成功した場合だけ、同じwheelをPyPIへ公開する。変数が未設定なら自動公開しない。登録値とコマンドの正本は[公開手順](../pypi-publishing.md)に置く。[^guide]

準備ジョブは、タグのmainへの所属、GitHub Releaseの公開状態、3種類のwheelの名前・ハッシュ・メタデータ・ライセンス本文を確認する。検証後のwheelだけを公開ジョブへ渡し、公開ジョブにはソースのcheckoutやビルドを含めない。インデックス公開段階の`id-token: write`は、TestPyPIとPyPIの送信ジョブだけに付与する。[^release-workflow][^validator]

# アカウント側の設定

PyPIとTestPyPIそれぞれに、所有者`HayaoSuzuki`、リポジトリ`hoimin`、ワークフロー`release.yml`をTrusted Publisherとして登録する。Environmentは送信先と同じ`pypi`または`testpypi`を指定する。GitHub側で両Environmentにmainへのブランチ制限を設定する必要があり、ワークフローの追加だけでは有効にならない。[^guide]

Environmentに必須レビュアーを設定すると、公開は承認待ちになる。完全自動化する場合はmainへの制限を維持し、レビュアーなしで公開できる設定にする。登録と初回実機確認を終えてから、自動公開変数を有効にする。[^guide]

# 検証範囲と再確認条件

GitHub ReleaseのPR previewは配布入力の変更時に選択し、タグを指定しない手動実行では常に選択する。公開専用の手動実行はビルドを省略する。
previewのprepareはread権限とし、マージ済みcommitのタグ予約と公開だけをwrite権限にする。[^release-workflow]

GitHub Release用のビルドでは、PR検証と手動検証は実行イベントの`github.sha`、マージ後の公開は`pull_request_target`の`merge_commit_sha`を使う。PR更新時のペイロードには古い`merge_commit_sha`が入る場合があるため、検証対象の選択には使わない。checkout、後続ジョブへのコミット指定、同時実行のグループで同じ選択条件を用いる。[^release-workflow][^ci-tests]

ビルド後のwheelスモークテストは、`Elastic-2.0`と移管先のRepository・Issues・Changelog URLを確認してから、隔離環境へのインストールとCLI実行を検証する。メタデータの回帰テストでは、旧`MIT`ライセンスと旧組織のURLを拒否する。ライセンスや公開URLを変更するときは、この期待値も更新する。[^wheel-smoke][^wheel-smoke-tests]

ライセンス本文の比較は、Windowsのcheckoutで生じるCRLFとLFの違いだけを許容する。本文や空白の変更は拒否し、wheel自体のバイト列とSHA-256照合は変更しない。[^validator][^tests]

ローカルテストは、不足・破損したwheel、異なる版やライセンス、重複したメタデータ、無効なタグを拒否する条件を検証する。実際のOIDC認証、Environment承認、PyPIへのアップロードはローカル検証の対象外であり、初回TestPyPI公開時に確認する。[^tests][^guide]

ワークフローや検証コードを修正した場合は、mainへのマージ後に新しい手動実行を開始する。GitHubのRe-runは元のコミットを使うため、コード修正を反映しない。[^guide]

既存の3ファイルと両証明をすべて検証できた場合だけ、アップロードを再実行せず検証済み公開を再利用する。無条件の`skip-existing`は使わない。一部だけアップロードされた場合は、登録済みのファイルとハッシュを調べ、必要なら新しい版を公開する。workflow名、リポジトリ所有者、Environment名、Python対応範囲、wheelのプラットフォーム名、ライセンス本文を変えたときは、登録値と検証条件を読み直す。[^guide][^validator]

[^guide]: [PyPI公開手順](../pypi-publishing.md)。
[^validator]: [配布物の検証処理](../../tools/pypi_release.py)。
[^tests]: [公開準備の回帰テスト](../../tests/test_pypi_release.py)。

[^wheel-smoke]: [wheelのスモークテスト](../../tests/wheel_smoke.py)。
[^wheel-smoke-tests]: [wheel検証の回帰テスト](../../tests/test_wheel_smoke.py)。

[^release-workflow]: [GitHub Releaseのビルド・公開ワークフロー](../../.github/workflows/release.yml)。
[^ci-tests]: [CIワークフローの回帰テスト](../../tests/test_ci_workflow.py)。

# ビルド証明と公開イベント

各wheelのSLSA証明はビルド時に生成し、元の署名済みstatementを書き換えずPEP 740へ変換する。
PyPIはアップロード元のTrusted Publisherで両証明を検証するため、ビルドと公開はともに`release.yml`を使う。
公開ジョブを再利用workflowに分けるとOIDCの`job_workflow_ref`が変わるため、公開ジョブは同じファイルへ直接定義する。[^provenance][^review]

ビルドの`pull_request_target`イベントからはPyPIへ公開できない。
マージ後の限定ジョブが`actions: write`でmainの`workflow_dispatch`を起動し、公開専用runで元のtag・source・workflow・wheel digestを照合する。
公開時の証明書のsource revisionをビルド元のrevisionと混同しない。[^release-workflow][^provenance]

記録済みの実署名による拒否試験と、外部サービスを差し替えた制御フロー試験を区別する。
2026年10月11日（JST）、形式修正後のmainからv0.3.6をTestPyPIとPyPIへ公開し、両インデックスの各3wheelを取得して検証した。
SHA-256はGitHub Releaseと一致し、SLSAとPublishの署名検証も成功した。
ビルド元は`c9f92eb`、公開実行は`3dbf6df`であり、元のビルドstatementはバイト単位で保持されていた。[^bootstrap][^bootstrap-evidence]

改変bytes・別repository・別source・別workflow・署名破損の5ケースが拒否されたことを確認した後、`PYPI_AUTO_PUBLISH=true`を設定して読み戻した。
初回失敗と復旧の経緯、実行ID、公開証明の属性は実機記録とJSONに残している。[^provenance-tests][^bootstrap][^bootstrap-evidence]

v0.3.7も両インデックスへの公開と取得検証に成功した。PyPI公開直後の版情報APIが404を返したため、反映後に失敗した検証ジョブだけを再実行した。自動公開は有効だが、インデックスの反映遅延からの自動復旧は未実装である。[^bootstrap]

[^provenance]: [証明の変換と公開後検証](../../tools/pypi_provenance.py)。
[^provenance-tests]: [証明と公開条件の回帰テスト](../../tests/test_pypi_provenance.py)。
[^review]: [Issue #771の設計・検証記録](../reviews/2026-10-10-issue-771-pypi-provenance.md)。

[^bootstrap]: [Issue #771の実機bootstrap記録](../reviews/2026-10-11-issue-771-hosted-verification.md)。

[^bootstrap-evidence]: [両インデックスの検証結果JSON](../reviews/2026-10-11-issue-771-hosted-verification.json)。
