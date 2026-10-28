# 日本語教科書の編集と公開

日本語教科書の正本は [docs/book/index.md](book/index.md) に置く。
製品の英語リファレンスを生成する `tools.docs_site` の公開allowlistとは独立したMkDocsコンテンツである。
本文は12章、実習仕様、演習解答、プロンプト、参考文献からなる。
hoiminの内部実装は説明しない。

## 原稿と教材の検証

依存関係を用意したPython環境で、リポジトリルートから実行する。

```console
python tools/build_mutation_book_assets.py
python -m tools.check_mutation_book
python -m mkdocs build --strict -f mkdocs-book.yml -d site/book-preview
```

ビルドにはMkDocs Materialが、教材検証には `docs/book/examples/shipping/requirements.txt` のpytestとcoverage.pyが必要である。
新しい環境でまとめて実行する場合は、次の形を使える。

```console
uv run --python 3.14 --no-project --with-requirements docs/book/examples/shipping/requirements.txt --with mkdocs-material==9.7.7 python -m tools.check_mutation_book
uv run --python 3.14 --no-project --with mkdocs-material==9.7.7 python -m mkdocs build --strict -f mkdocs-book.yml -d site/book-preview
```

ZIP生成は固定した7ファイルだけを取り込み、キャッシュ、仮想環境、実習報告は含めない。
検証は配布ZIPを新しい一時フォルダに展開し、正本との一致、初期テスト、解答例、丸め前の行と分岐の100%、代表的な変異での検出差を確認する。
この手動変異実験はhoiminによる実行とは別である。
ほかの変異も検出するとは主張しない。

## 個人サイトへの取り込み

公開先は `HayaoSuzuki/HayaoSuzuki.github.io` の `/mutation-testing/` とする。
既存の `/books/` は書評欄なので、教科書の配置には使わない。
公開先のビルドでhoiminリポジトリの固定コミットを別パスへチェックアウトし、その `docs/book/` をサイトの `docs/mutation-testing/` へコピーする。
`examples/` はコピーせず、教材は本文からリンクしたZIPとして配布する。
章順は同梱の `.pages` を使う。

公開先に必要な変更は、`.github/workflows/ci.yml` の取り込みと `docs/.pages` の入口である。
原稿を公開先へ手編集で複製しない。
ローカルで個人サイトをビルドする場合も、同じ固定コミットから取り込む必要がある。
サイト側の取り込み用スクリプトに記録されたコミットを更新することが、教科書改訂の公開操作になる。

## リモートとブランチ

両方のリポジトリで `origin` の意味が異なる。

| 作業 | リポジトリ | 作業ブランチ |
| --- | --- | --- |
| 原稿と教材 | `HayaoSuzuki/hoimin` | `docs/japanese-mutation-testing-book` |
| サイトへの取り込み | `HayaoSuzuki/HayaoSuzuki.github.io` | `docs/mutation-testing-book` |

push前に `git remote -v` と `git branch -vv` を確認する。
hoimin側のコミットをそのリポジトリへpushして到達可能にしてから、公開先の固定コミット取り込みを検証する。
公開先のmainへのマージで既存のGitHub Pagesワークフローが公開する。
hoiminのmainへのマージだけでは、個人サイトの固定コミットは自動更新されない。

## 初稿の検証記録

2026-10-28、Windows、Python 3.14.9、pytest 9.1.1、coverage.py 7.16.2で確認した。

- ZIP展開後の初期テストは9件成功、実行可能行15/15、分岐12/12。
- 初期テストと解答例を合わせて27件成功。
- 送料無料境界、重量境界、会員割引、遠隔地加算、空注文条件の手動変異5種類は初期テストで生存し、解答例を加えるとアサーションで検出。
- 下限制御の除去という等価な変更、例外メッセージという仕様が許す変更は、初期テストと解答例の両方で成功。
- PyPI配布版hoimin 0.14.0の `compare_order` により送料無料境界の1候補を生成。同じ計画と候補IDで、初期テストでは生存、解答例追加後は検出。両方のベースライン成功と実行完了を確認。

配布版0.14.0と開発版mainの仕様差を第7章とプロンプトに記載した。
バージョン番号だけで開発中のコードと同一の出力契約と判断しない。
他のOS、他のエージェントの会話結果、実習で選ばれる全候補は未検証である。
