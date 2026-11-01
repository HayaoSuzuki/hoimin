## 確認した問題

`try` 内の通常の式が例外を送出して後続の import を飛ばすと、`finally` 内の型注釈が typing 由来ではない束縛を参照する。それでも解析器が `type_list_sequence` を生成する。

確認対象: `f11013542ccd735ab9741b5079c0b39a517df256`、macOS arm64、CPython 3.14.7、同HEADからビルドしたrelease CLI。優先度案: P2。

## 最小再現

以下を `subject.py` として保存する。plan は `hazard` を実行しない。

```python
Sequence = set
try:
    try:
        hazard()
        from typing import Sequence
    finally:
        def record(value: Sequence[int]):
            pass
        observed = record.__annotations__['value']
except KeyError:
    pass
```

```sh
hoimin plan --root /path/to/project --file subject.py \
  --operators type_list_sequence --allow-best-effort-memory -- true
```

期待: 0候補。実際: 7行目の `Sequence[int]` を `list[int]` にする候補1件。

CPythonで同一ソースを `exec` し、外部から渡す `hazard` を `pass` と `raise KeyError()` の2種類にすると、`observed` はそれぞれ `typing.Sequence[int]` と `set[int]`。どちらも例外を外へ漏らさず終了する。従って finally の全入口で typing provenance は成立しない。

対照:

- import を `hazard()` の前へ移す: 1候補、両実行で typing。期待と一致。
- `hazard()` を明示的な `raise KeyError()` にする: 0候補。期待と一致。

## スコアへの影響を実runで確認

同じソースに対して、以下のテストを実行する。

```python
import runpy
def hazard():
    raise KeyError()
ns = runpy.run_path('subject.py', init_globals={'hazard': hazard})
assert ns['observed'] == set[int]
```

これをproject内の `check.py` として保存し、`hoimin run --root /path/to/project --file subject.py --operators type_list_sequence --allow-best-effort-memory --min-free-space 1B -- /absolute/path/to/python3.14 check.py` で実行する。今回の実測では同じテスト本文をPythonの `-c` 引数として渡した。

baseline成功、mutant 1件がkilled、score=1.0、complete=true、CLI exit 0を観測した。typing.Sequenceへの変異として計上された候補が、実際にはset[int]をlist[int]へ変えている。

## 原因と契約

[開発契約](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/docs/development.md#L492) は、到達可能な全exitで一致するimportだけを合流後に保持するとしている。

[`visit_try`](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/crates/hoimin-cli/src/analyzer/rust.rs#L4987) は body のfallthrough/明示的abrupt/handlerを合流する。通常のcallなどからの暗黙の例外exitを `apply_finally` の入力へ加えていない。後者は渡されたexitだけをintersectionするため、飛ばされたimportを既知として扱う。

既存のNestedTryFlowモデルは「与えられたexitをfinallyへ送る」規則を検証している。今回、既存の公開対応テスト2件は成功したが、この不足経路はその前提に入っていない。

## Leanの確認

追加の最小モデルでは初期provenanceをcustomとし、import成功・通常式の成功/例外・明示的raiseを表す。3イベント、深さ0〜4の121トレース（列挙トレース長合計426）で、暗黙例外を捨てるモデルとの相違は18件。最短反例は `[mayRaise, importTyping]`（深さ2）。finally入口は `[custom, typing]` になる。

カーネル検査済みの定理で、custom状態のmayRaiseが先頭にある任意の後続列では、全入口typingという主張が成立しないことを確認した。Lean生成3fixtureをrelease CLIへ適用し、1 mismatch / 2 match。CPythonの6実行で束縛を観測した。モデルはimport自体の失敗、動的hook、全Python構文を証明しない。

## 対応案・受け入れ条件

- 暗黙の例外でfinallyへ入る束縛を保守的に合流し、既知でない名前の候補を抑制する。
- call、subscript、attributeなど、明示的raise以外の例外入口を最小ケースとして保持する。
- 上記の負例と、importが呼出しより前にある正例、明示的raiseの対照をCLI/Lean対応試験へ追加する。
- 正常経路だけの型候補や、既存のbreak/continue/return/finallyの対応検証を維持する。

今回は監査と起票のみで、production修正は行っていない。
