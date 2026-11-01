# Python 3.14 と厳格な ty 型チェックの設計

## 目的

hoimin の Python 実行環境を Python 3.14 に一本化し、`ty` を静的型チェックの必須品質ゲートとして導入する。
型チェックはアナライザ実装と実行可能なテストコードの両方に適用する。

## 対象と非対象

対象は `pyproject.toml`、`uv.lock`、Python 3.14 に関する CI・README・開発者文書、`python/` と `tests/` の型エラーである。
Rust の MSRV、CLI の JSONL 契約、mutation 演算子、実行時依存の追加は対象外とする。

## Python バージョン契約

パッケージの `requires-python` は `>=3.14,<3.15` に固定する。
Python 3.12 と 3.13 の classifier、CI matrix、README の互換性表記を削除し、すべて Python 3.14 のみを表す。
リリース wheel とローカル開発の検証も Python 3.14 を使用する。

## ty の構成

`ty` は `dev` dependency group に追加する。
設定は `pyproject.toml` の `[tool.ty.environment]` と `[tool.ty.src]` に集約し、Python version を `3.14`、検査対象を `python` と `tests` に固定する。

未解決 import、型不一致、未注釈の public interface を包括的に無視しない。
テストダブルなど型システム上正確に表せない箇所だけは、最小のファイルまたは行に限定して根拠をコメントで残す。

## 品質ゲート

ローカルと CI の Python 品質ゲートは次を実行する。

1. `uv run ruff check python tests`
2. `uv run ruff format --check python tests`
3. `uv run ty check`
4. `uv run pytest`

`ty check` の検査対象には実装とテストを含める。
CI は Python 3.14 のみをセットアップし、同一コマンドを実行する。

## 検証

Python 3.14 の新規環境で `uv sync --frozen` を実行し、Ruff、ty、pytest（テストコードを含む 100% coverage）、wheel smoke を通す。
README と package metadata に Python 3.12/3.13 のサポート表記が残らないことを検索で確認する。
