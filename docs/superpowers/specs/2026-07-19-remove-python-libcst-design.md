# Python/LibCST 一括撤去の設計

## 目的

hoimin の解析経路を Rust アナライザだけにし、LibCST と解析専用 Python を削除する。
`uvx hoimin` による配布・起動は維持する。

## 構成

`hoimin-cli` はファイルを Rust で解析し、既存の candidate validation、spool、worker byte patch を使う。
CLI は `--python` を受け取らず、Python subprocess を起動しない。
Python は wheel を配布するための PEP 517/Maturin 実行環境としてだけ残り、runtime dependency ではない。

## 削除対象

- `python/hoimin_analyzer.py`、`python/tests/`、Python fixture。
- `libcst`、pytest、pytest-cov、pytest-randomly、Hypothesis、ty、Ruff を含む解析専用 Python 開発依存と設定。
- `--python` のCLI引数、Rust設定、実行前のPython/LibCST検証、Python/LibCST版取得。
- report event、JSON schema、resume fingerprint、session互換性にあるPython/LibCST version fields。
- LibCST同値比較テストと、Python helperを前提にしたwheel smoke/E2E/documentation。

## 保持対象

- `pyproject.toml` のMaturin build backend、Python 3.14制約、wheel metadata、`uvx`での実行可能バイナリ配布。
- Rustアナライザ、candidate schema、worker byte patch、run reportのそれ以外のフィールド。
- Rust unit/integration/E2Eテスト、wheel buildとinstalled-wheel smoke test。

## 永続化と互換性

run eventの環境情報は `os` と `hoimin` のみを報告する。
resume fingerprintからPython/LibCST versionを削除する。
既存のSQLite sessionは互換外とし、旧schema/versionを移行しない。
新しいsessionは新fingerprintとschemaで作成する。

## 検証

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `uv run maturin build --release`
- installed-wheel smoke test（`--python`なしの`hoimin run`）
- `uvx`相当の配布済みCLI起動確認

## 非対象

Rustアナライザの演算子集合、candidate schema、workerのmutation適用、reportのPython/LibCST以外の構造は変更しない。
