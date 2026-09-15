## 問題

`--include` / `--exclude` を変更してworkerにコピーされる補助ファイルが変わっても、session fingerprintが一致し、旧条件のkilled/survivedを再利用する。現条件で新規実行した結果と逆の判定になる。

対象HEAD `5e631ef`、macOS arm64、CPython 3.14.7。debug/release両方の公開CLIで再現した。対象ソース、テスト、補助ファイルの内容、test argv、import roots、実行件数・資源上限は変更していない。補助ファイルには両runとも `--fingerprint-file strict.flag` を付けており、明示fingerprintでも防げない。

## 最小再現

`subject.py`:

```python
def value():
    return (1+2)+(3+4)
```

`check.py`:

```python
from pathlib import Path
import subject
if Path('strict.flag').exists():
    assert subject.value() == 10
```

rootに内容 `strict` の `strict.flag` を置く。session DBはroot外に置く。以下のPYTHONは同じ絶対パスを指定する。

```sh
# 1. 3候補のうち1候補を実行し、不完全runとして保存する。
hoimin run --root PROJECT --file subject.py --operators binary_add_sub \
  --jobs 1 --max-mutants 1 --max-workspace-size 8GiB --min-free-space 10GiB \
  --fingerprint-file strict.flag --allow-best-effort-memory \
  --session SESSION.sqlite3 --format json -- PYTHON check.py

# 2. 同じ条件に --resume --exclude strict.flag を追加して実行する。
# 3. 比較用に、2から --session SESSION.sqlite3 --resume を除いて新規実行する。
```

| 実行 | コピー内のflag | 先頭候補 | score | 保存済み結果 |
| --- | --- | --- | ---: | --- |
| 初回 | あり | killed | 1.0 | 新規実行 |
| flag除外でresume | なし | killed | 1.0 | 初回run_idを再使用、terminationなし |
| flag除外で新規実行 | なし | survived | 0.0 | 実プロセスExit(0) |

3実行ともbaselineは成功、候補IDも同一。`--max-mutants 1` により残り2候補がnot_runとなり、complete=false、exit=4なのは意図した条件である。実行後のcleanupは正常。

逆に、除外を外すと古いsurvivedを再利用し、新規実行のkilledを見逃す。`.ignore`でflagを除外して `--include strict.flag` を追加・削除する場合も両方向で再現した。このinclude fixtureでは、`--include subject.py --include check.py` を両runへ共通に指定する。

## 原因

[FingerprintInput::from_config / fingerprint](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-core/src/resume.rs#L18) はsources、明示fingerprint入力、targets、operators、profile、test argv、limits、resource mode、import rootsを持つが、`selection.includes` / `selection.excludes` を持たない。補助ファイルのroot上のpath/hashが同じでも、workerへのコピー有無は変えられる。

[SessionHandler::load](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/session/mod.rs#L158) はそのfingerprintで不完全runを選び、状態機械は保存済みのkilled/survivedを再利用する。[READMEの再開契約](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/README.md#sessions-and-resume)が禁止する、不互換な条件の結果混在となる。

#350はjobs/max-outputによる過剰な無効化の解消であり、今回のコピー内容を変える設定とは区別が必要。#472/#516のsession自身のコピー除外とは別で、今回のDBはroot外、除外対象は利用者の補助ファイルである。

## 検証

Leanでコピー有無と出力上限を分離し、正しく保存された結果という仮定の下で、コピー条件を比較するresumeの判定が現条件の新規実行と一致することを証明した。コピー条件が違えば再利用しないこと、任意の出力上限の違いだけでは互換性を失わないこともモデル内で証明した。

有限16条件で、コピー条件をfingerprintから落とす壊した規則を8条件で検出した。Lean生成7ケースは、各ケースで初回run→SQLite再開→現条件の新規runを実行した。debug/release各21 runで、3 match / 4 mismatch / 実行基盤エラー0。コピー設定を変更した4ケースだけが不一致で、設定据置き2ケースとmax-outputのみ変更する1ケースは一致した。ファイル内容の不変、SQLiteのrun_id/fingerprint、候補ID、baseline、status、cleanupも照合した。

関連する既存テスト64件は成功し、subprocess用fixtureの1件は予定どおりignoredだった。Leanはこのfixtureの小モデルを証明しており、Rust全体やhashの衝突耐性を証明しているわけではない。

## 受け入れ条件

- 判定へ影響するコピー方針を再開互換性へ含める。正規化したinclude/exclude設定を含めるか、影響するコピー入力を比較するかを設計で決める。
- コピー条件を変えたresumeで旧runのkilled/survivedを混在させず、現条件の新規実行と同じ判定にする。
- 同一コピー方針での再利用と、jobs/max-outputなど既存の運用設定の互換性を維持する。
- fingerprint schema更新と旧sessionの扱いを決め、exclude/includeの追加・削除を公開CLIの回帰ケースに含める。

全証拠・モデル・再現スクリプトは作業ツリー `docs/audits/2026-09-15-resume-copy/` に保存した（起票時点では未コミット）。再現は `python3 docs/audits/2026-09-15-resume-copy/replay.py --binary target/release/hoimin --output /tmp/resume-copy.json`。
