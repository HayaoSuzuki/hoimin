## 問題

`contextlib.suppress` が本体途中の例外を抑制すると、後続の型注釈で使うimport状態が不正になる。`Sequence` が `set` のままの実行経路があるのに、`type_list_sequence` は `Sequence[int] → list[int]` の候補を生成する。実runでもこの候補がkilledとなり、score=1.0、complete=true、exit=0に入る。

確認対象: `5e631ef`、macOS arm64、CPython 3.14.7。HEADから `cargo build --offline -p hoimin-cli` と `cargo build --offline --release -p hoimin-cli` を行い、debug/release両方で再現した。

## 最小再現

一時プロジェクトの `subject.py`:

```python
from contextlib import suppress
Sequence = set
with suppress(KeyError):
    hazard()
    from typing import Sequence
def record(value: Sequence[int]):
    pass
observed = record.__annotations__['value']
```

```sh
hoimin plan --root /path/to/project --file subject.py \
  --allow-best-effort-memory --operators type_list_sequence -- true
```

期待: 候補0件。実際: `Sequence[int] → list[int]` の1件。`hazard` はplanでは実行されない。実行の比較は同じsourceへ外からcallableを与える。

```python
def hazard():
    raise KeyError()
ns = {'hazard': hazard}
exec(compile(open('subject.py').read(), 'subject.py', 'exec'), ns)
assert ns['observed'] == set[int]
```

これを `check.py` として実行すると成功する。次のrunではbaseline成功、killed=1、survived=0、score=1.0、complete=true、exit=0となる。

```sh
hoimin run --root /path/to/project --file subject.py \
  --allow-best-effort-memory --operators type_list_sequence \
  --format json -- /absolute/path/to/python3.14 check.py
```

hazardが正常終了した場合の同じ元ソースでは、注釈は `typing.Sequence[int]`。例外時は `set[int]` なので、全到達経路でtyping由来という契約を満たさない。

## 原因と既報との差

[visit_with](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L5297) は `visit_suite_flow` の本体fallthroughをそのまま後続へ渡す。`__exit__` 自体が投げる例外は追跡するが、本体の例外を抑制してwith後へ戻る経路をnormalへ合流していない。さらに、暗黙例外の追跡はfinallyがあるときにだけ有効となる。

#545はfinallyの入口への暗黙例外合流で、修正済み。今回のfixtureにはtry/finallyがなく、必要なのはwithにおける例外から正常継続への変換である。`docs/development.md` の「全到達可能exitで一致するimportだけを保持する」契約に反する。

## 形式検証と確認範囲

Leanモデルで抑制された例外をnormalへ加える規則を定義し、任意長のイベント列について、候補許可ならモデルの各runtime観測がtyping由来であることを証明した。初期custom状態から `mayRaise` で始まる任意の後続列の拒否も証明した。例外合流を省く壊した規則は `[mayRaise, importTyping]` で検出される。

Lean生成の5fixtureを公開CLIで再生し、debug/releaseそれぞれ4 match / 1 mismatch。CPythonの10観測はすべてLean期待値と一致。比較対象は候補数と注釈のtyping判定であり、Rust実装全体の証明ではない。import失敗、任意の動的hook、async、return/break/continueは今回のモデル対象外。

監査証拠は作業ツリーの `docs/audits/2026-09-15-with-finally/` に保存した（起票時点では未コミット）。

## 受け入れ条件

- 本体で例外が起き、context managerが抑制して後続へ進む場合にも安全なimport合流を行う。
- 上記のcall-before-importは0候補にする。import-before-call、正常importの正例は候補を保持する。
- 明示raise、複数with item、独自context manager、async withの扱いを明文化して回帰検証する。動的managerの抑制可否が不明な場合は、抑制する経路も保守的に考慮する。
- 既存のfinally、例外handler、break/continue/returnの経路を混同しない。
- 上記実runでtyping変異として不適切なkilledを数えない。
