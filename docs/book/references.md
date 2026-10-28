# 用語と参考文献

## 用語

- **行カバレッジ**：測定対象の実行可能な行のうち、テストで実行した割合。
- **分岐カバレッジ**：分岐から進む先のうち、実行したものを測る指標。測定単位はツールの定義を確認する。
- **テストオラクル**：結果の正しさを判定する基準や仕組み。
- **ミュータント**：元コードへ小さな変更を加えたプログラム。
- **変異演算子**：どのようにコードを変更するかを定める規則。
- **検出（killed）**：変異によってテストが失敗したという結果。テスト改善の証拠として用いる場合は、失敗の理由も確認する。
- **生存（survived）**：変異後もテスト実行が完了し、成功したという結果。等価性やテスト不足をそれだけで確定しない。
- **等価ミュータント**：指定した入力領域と観測対象について、元コードと同じ振る舞いになるミュータント。
- **ベースライン**：変異を加えない元コードを、比較の基準となる条件で実行した結果。
- **RIPR**：到達、状態の差、伝播、判定という検出条件を考えるモデル。
- **planとverify**：hoiminの候補発見と、保存した候補の検証を行うコマンド。

## 理論を読むための文献

Paul Ammann and Jeff Offutt, *Introduction to Software Testing*, Second Edition, Cambridge University Press, 2016。
[著者による書籍サイト](https://www.albany.edu/faculty/offutt/softwaretest/)に教材がある。
本書の第4章で使うRIPRと、構文に基づくテスト基準を体系的に学べる。
[著者作成のミューテーションテスト講義資料](https://www.cs.cornell.edu/courses/cs5154/2021sp/resources/MutationTesting.pdf)も参照できる。

Yue Jia and Mark Harman, “An Analysis and Survey of the Development of Mutation Testing,” *IEEE Transactions on Software Engineering*, 37(5), 649–678, 2011。
[論文のDOI](https://doi.org/10.1109/TSE.2010.62)。
変異演算子、等価ミュータント、実行費用など、手法全体の背景を調べるための文献である。
本書は、この研究の歴史全体や各最適化手法の実装を説明するものではない。

## 操作と測定の一次資料

- [coverage.pyの分岐測定](https://coverage.readthedocs.io/en/latest/branch.html)：何を分岐として数え、表示上の割合へどう反映するか。
- [pytestのパラメータ化](https://docs.pytest.org/en/stable/how-to/parametrize.html)：複数の入力と期待値を表で指定する方法。
- [uvの導入](https://docs.astral.sh/uv/getting-started/installation/)：Python環境とツールを用意する際の入口。
- [hoimin 0.14.0の利用ガイド](https://github.com/HayaoSuzuki/hoimin/blob/v0.14.0/docs/usage.md)：本書の対象版のコマンドと資源制約。
- [hoiminの現行利用ガイド](https://hayaosuzuki.github.io/hoimin/docs/usage/)：現行版の操作。対象版との差に注意する。
- [hoiminの製品方針](https://hayaosuzuki.github.io/hoimin/docs/product-principles/)：仕様に基づくテスト改善とローカル利用の位置づけ。
- [hoiminのAgent skills](https://hayaosuzuki.github.io/hoimin/docs/agent-skills/)：エージェント向けの手順と導入方法。
- [ミューテーションテストツール「hoimin」の実装](https://hayaosuzuki.github.io/articles/2026/2026-10-07-hoimin/)：著者による開発動機の説明。

本書の実行例の版と、オンライン文書の最新版は一致しない場合がある。
再現するときは、教材、ツールの版、実行コマンドを一緒に記録する。

[目次に戻る](index.md)
