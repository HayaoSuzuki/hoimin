---
type: Playbook
title: ドキュメント公開
description: mainの検索可能なサイト、公開権限、初回設定と再実行。
status: draft
catalog_revision: 921e9afc03c2ca5152b876640813250e7c63b8dc
sources:
- id: guide
  resource: ../documentation.md
  revision: 4a1c86f0902aa300e846e1daa3b18f4282f15861
  working_tree: modified
  sha256: 445b302a3f05555175486e497bdac9b9d1e2081ebcdf9968f9a78807acfbb690
- id: builder
  resource: ../../tools/docs_site.py
  revision: 4a1c86f0902aa300e846e1daa3b18f4282f15861
  working_tree: modified
  sha256: a8b25c1152a7f2cfa964c74f0b4959bd462710f738bd78c120161c5f9ca38fc3
- id: workflow
  resource: ../../.github/workflows/documentation.yml
  revision: 921e9afc03c2ca5152b876640813250e7c63b8dc
  working_tree: clean
  sha256: d77b9e98f3c3ccdd85eff3dfa4bf0947547255f9754622a1df2a8fa1cd01fa05
- id: tests
  resource: ../../tests/test_docs_site.py
  revision: 4a1c86f0902aa300e846e1daa3b18f4282f15861
  working_tree: modified
  sha256: c2bc8b2d65bf14bdbc1f076351b5c3cd5b79b80c441525ec94565ab1f2067768
- id: design
  resource: ../superpowers/specs/2026-10-12-issue-749-documentation-pages-design.md
  revision: 921e9afc03c2ca5152b876640813250e7c63b8dc
  working_tree: clean
  sha256: 683ffc6122e2372c7495fd02835de0abfd03ab8e7bd72770e6fc17de4807e56b
---

# 公開対象と更新手順

[公開ガイド](../documentation.md)を手順の正本とする。Markdownはhoimin内の既存の場所で編集し、PRのDocumentationチェックとHTML artifactを確認してmainへマージする。サイトはmainを対象とし、各ページにビルド元のコミットを表示する。初回はworkflowのマージ後にPagesのSourceをGitHub Actionsへ設定する。公開先は `https://hayaosuzuki.github.io/hoimin/` であり、既存のユーザーサイト側へファイルをコピーする必要はない。[^guide][^design]

# 公開権限と検証

全PR、merge group、mainへのpush、手動実行で軽量なビルドを行う。ビルドの権限はcontents: readだけとし、公開用artifactの作成とdeployは正規リポジトリのmainへのpushまたはmainを指定した手動実行に限る。pages: writeとid-token: writeはdeploy jobだけに与える。mainのビルドから公開まで同じ同時実行グループで管理し、古い実行を取り消す。[^workflow]

公開する14ページは明示的な許可リストから一時ディレクトリへコピーする。検索対象に研究・監査・OKFのページは含めない。そこへのMarkdown参照はファイルの存在を確認したうえで、元のコミットを指定するGitHubリンクにする。欠けたファイルをGitHubリンクへ隠して通すことはせず、内部アンカーもMkDocsのstrict検証でエラーにする。生成HTMLも検証し、同じサイトの絶対URLとHTMLのhref/srcにある欠けたページやアンカーを拒否する。HTMLのURLは生成後のパスを指すため、ソースへの参照にはMarkdownリンクを使う。コード例や外部URLを実行・取得しない。[^builder][^tests]

# 失敗時の確認と保証の範囲

ビルド失敗時はMarkdownのリンクとアンカーを修正する。公開の再試行にはDocumentationの手動実行でmainを選ぶ。ローカルで成功することと、Pages設定・Environment承認・実際の公開成功は別々に確認する。初回公開後にトップページ、検索、plan/verify、結果、資源制限への導線を確認し、それまでは公開済みと扱わない。リリース版ごとの履歴切替は今回の対象外である。[^guide][^design]

許可リスト、URL変換、Markdown拡張、Pages公開条件、同時実行制御を変更した場合は、対応するリンク・権限・表示を再検証する。出典の存在確認にはリポジトリ内の文書を使い、外部の任意入力を取り込む機能は追加しない。[^builder][^workflow]

[^guide]: [公開手順](../documentation.md)。
[^builder]: [ビルド処理](../../tools/docs_site.py)。
[^workflow]: [Documentation workflow](../../.github/workflows/documentation.yml)。
[^tests]: [リンク変換の回帰テスト](../../tests/test_docs_site.py)。
[^design]: [承認済み設計](../superpowers/specs/2026-10-12-issue-749-documentation-pages-design.md)。
