# Apple Silicon wheel 検証

## Goal

Apple Silicon Mac で `hoimin` の macOS arm64 wheel をビルドし、ローカルの
wheel を `uvx --from` で実行できることを継続的に確認する。PyPI への公開と
タグリリース時の配布 artifact はこの変更の対象外とする。

## Scope

- `tests/wheel_smoke.py` が macOS で生成される arm64 wheel を選択できるように
  する。
- GitHub Actions の通常 CI で `macos-14` を Rust テストと wheel smoke の対象に
  追加する。
- Apple Silicon のローカル環境では、既存の
  `uv run maturin build --release` が生成した wheel を smoke test が検証する。
- wheel smoke は、選択した wheel を `uvx --from <wheel> hoimin --help` と隔離
  virtual environment へのインストールで検証し、実行可能な `hoimin` CLI を
  確認する。

## Non-goals

- PyPI への公開、Trusted Publishing、release workflow の変更。
- Intel macOS、Linux、または Windows から macOS arm64 wheel を作る
  クロスコンパイル。
- universal2 wheel の作成。

## Design

`wheel_smoke.py` のプラットフォーム別 wheel 選択に macOS を加える。macOS では
wheel filename に含まれる `arm64` tag を互換条件とし、Linux と Windows の既存
条件は変更しない。これにより、ネイティブ Apple Silicon で Maturin が出力する
`macosx_*_arm64` wheel が smoke test に渡る。

macOS の portable process backend は、子プロセスの process group と
`RLIMIT_CPU` を設定する。Darwin は仮想メモリ制限 `RLIMIT_AS` を拒否するため、
macOS の `pre_exec` では設定しない。このため macOS は `best_effort` resource
mode であり、`--max-memory` は強制されない。通常実行では
`--allow-best-effort-memory` を必須にし、CLI の diagnostic はこの制約を明示する。
Linux の `RLIMIT_AS`/`RLIMIT_CPU` 設定と cgroup v2 backend、Windows Job Object
backend は変更しない。

通常 CI の `rust` job と `wheel-smoke` job の OS matrix に `macos-14` を加える。
`macos-14` は GitHub-hosted Apple Silicon runner であり、Maturin はその runner
上でネイティブ arm64 wheel を生成する。release workflow は Windows と Linux の
ままにして、artifact upload と publish job の依存関係は変更しない。

## Validation

- Apple Silicon macOS で `uv run maturin build --release` を実行すると
  `target/wheels/` に arm64 macOS wheel が生成される。
- `uv run python tests/wheel_smoke.py` がその wheel を選択し、`uvx --from` と
  virtual environment の CLI 実行を成功させる。
- `cargo test -p hoimin-cli --test process_handler portable -- --nocapture` が macOS
  で通り、portable backend が `RLIMIT_AS` なしで child process を実行できる。
- CI の `Rust (macos-14)` と `Wheel smoke (macos-14)` が成功する。
- release workflow に差分がなく、PyPI 公開は発生しない。

## Error handling

macOS で `target/wheels/` に arm64 tag を含む wheel がない場合、smoke test は
生成済み wheel の名前を含めて失敗する。これにより、誤った target でのビルドや
wheel 生成失敗を明確に示す。
