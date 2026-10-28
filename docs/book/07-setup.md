# 第7章 実習環境の準備

実習では、AIエージェントがhoiminを実行し、読者が調査結果を判断する。
まず、変異を加えない状態で教材を動かす。
ここで失敗する場合は、変異の調査へ進まず環境を整える。

## 教材の展開

[教材ZIP](assets/shipping-workshop.zip)をダウンロードし、新しい作業フォルダへ展開する。
以後、`shipping.py` と `pytest.ini` がある `shipping-workshop` フォルダを作業ルートとする。
hoimin本体のリポジトリ内で実習する必要はない。

```text
shipping-workshop/
  shipping.py
  SPEC.md
  pytest.ini
  requirements.txt
  tests/test_shipping.py
  solutions/test_shipping_improved.py
```

`solutions/` は比較用の解答例である。
通常の `python -m pytest -q` は `pytest.ini` の指定によって `tests/` だけを実行する。
自分の調査を終えるまでは、解答例をテストに追加しない。

## Pythonと依存関係

Python 3.14とuvを用意する。
uvが未導入なら[公式インストール手順](https://docs.astral.sh/uv/getting-started/installation/)に従う。
作業ルートで次を実行する。

```console
uv venv --python 3.14
uv pip install --python .venv -r requirements.txt
```

仮想環境を有効にするコマンドはシェルによって異なる。

**PowerShell 7:**

```powershell
.venv\Scripts\Activate.ps1
```

**Bash（macOSまたはLinux）:**

```bash
source .venv/bin/activate
```

有効化できない場合は、仮想環境のPythonをフルパスで指定できる。
以降の `python` がこの環境を指すことを `python -c "import sys; print(sys.executable)"` で確認する。

```console
python -m pytest -q
```

初期教材では9件が成功する。
インポート、依存関係、収集のエラーを解決してから先へ進む。

## hoiminの版

hoiminはテスト環境とは別のツールとして導入できる。

```console
uv tool install --python 3.14 hoimin==0.14.0
hoimin --version
```

既存のインストールを変えたくない場合は、`uvx --python 3.14 --from hoimin==0.14.0 hoimin --version` で指定した版を呼び出せる。
その場合、以後の `hoimin` の代わりに同じ `uvx` の接頭辞を使うようエージェントへ伝える。
計画作成と再検証の途中で版を変えない。

配布版0.14.0と、原稿作成時点の開発版mainでは、出力形式と終了コードに差がある。
配布版の検証結果は既定でJSONになり、生存がある完了実行は終了コード1になる。
開発版ではJSONLになり、生存があっても完了実行は0になる。
この違いをエージェントへ伝え、状態と完了情報を構造化報告から確認させる。
開発版の資料を配布版の仕様としてそのまま適用しない。

本書は、公開ヘルプやJSONLの全項目を覚えることを求めない。
細かな指定はエージェントが[hoiminの利用ガイド](https://hayaosuzuki.github.io/hoimin/docs/usage/)で確認する。
配布スキルが利用できる環境では、[Agent skillsの説明](https://hayaosuzuki.github.io/hoimin/docs/agent-skills/)に従って導入してもよい。
スキルの導入自体は、hoiminのインストールや実行を行わない。

## 実行予算とOSの違い

この実習では1ワーカー、1回の検証につき最大5候補、全体時間上限1分から始める。
作業領域上限は8GiB、空き容量の留保は10GiBとする。
元のフォルダと一時領域の双方に10GiBを超える空きが必要である。
資源制限は、ホスト全体で資源枯渇が起こらない保証ではない。

WindowsではJob Object、委譲されたcgroup v2を利用できるLinuxではcgroupによるメモリ制限を利用する。
macOSではメモリ上限を強制できず、cgroupを利用できないLinuxでもプロセスツリー全体の強制上限とは異なる。
後二者では `--allow-best-effort-memory` の明示的な選択が必要になる。
その制約を受け入れるかは人間が決め、エージェントが自動で制限を緩めないようにする。
最新の適用範囲は[資源制限の説明](https://hayaosuzuki.github.io/hoimin/docs/usage/#limits-and-defaults)で確認する。

## 演習7

Python、pytest、coverage.py、hoiminの版とOSを記録する。
エージェントが別のシェルから起動する場合にも、同じ仮想環境のPythonを使えるか確認してほしい。

[解答例](answers.md#exercise-7) / [第8章 カバレッジ100%の料金計算](08-workshop.md)
