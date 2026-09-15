## 確認した問題

`collection_list_tuple` が多次元subscriptのスライス集合を通常のtuple literalと誤認し、コンパイルできないリスト表記へ変換する。候補が実行されるとSyntaxErrorをkilledに数え、スコアを上げる。

対象HEAD: `f11013542ccd735ab9741b5079c0b39a517df256`。macOS arm64、CPython 3.14.7、同HEADのdebug/release CLI。優先度案: P2。

## 最小再現

`subject.py`:

```python
def f(x):
    return x[:,]
```

```sh
hoimin plan --root /path/to/project --file subject.py \
  --operators collection_list_tuple --allow-best-effort-memory -- true
```

期待: スライス記法をリスト要素へ移す候補は生成しない。

実際: `original=":,"`, `replacement="[:,]"` の候補1件。変異後は `return x[[:,]]` となり、CPython 3.14のcompileでSyntaxErrorになる。

他の再現例:

- `x[1:2, 3] → x[[1:2, 3]]`
- `x[:, :] → x[[:, :]]`

対照: `x[1, 2] → x[[1, 2]]` はコンパイルできる。通常の式タプルと、slice要素を含む添字タプルの区別が必要。

## 実runでの影響

```sh
hoimin run --root /path/to/project --file subject.py \
  --operators collection_list_tuple --allow-best-effort-memory \
  --min-free-space 1B -- /absolute/path/to/python3.14 -c 'import subject'
```

同HEADのreleaseで、baseline Exit(0)、killed=1、score=1.0、complete=true、CLI exit 0を観測した。テストはfを一度も呼ばない。生成した候補のSyntaxErrorだけでkilledになる。

## 原因

- [collect_tuple_literal](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/crates/hoimin-cli/src/analyzer/rust.rs#L2652) はoperator選択、Load context、例外型・注釈範囲を確認するが、要素がExpr::Sliceであるケースを排除しない。
- [tuple_to_list_replacement](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/crates/hoimin-cli/src/analyzer/rust.rs#L3318) はtupleのソース表記全体を `[...]` で包む。
- RuffのExprTupleは通常のtuple expressionに加え、多次元添字も表す。そこに含まれるcolonはリスト内の式として使えない。

#548の名前解決や#545の例外経路とは別原因。#489の有効Pythonコーパスに、このExprTuple×Slice×collection_list_tupleの組合せを追加する必要がある。

## Lean・CPythonによる確認

Leanで要素を通常の式/sliceに分け、全要素が式の場合だけリスト変換を許可するモデルを定義した。許可から全要素が式であることを導く定理と、sliceを含む固定反例をカーネル検査した。

2要素種×長さ1〜3の14入力を最短優先で列挙し、全tupleを許可する壊したモデルに対して11反例、最短は `[slice]`。空の添字は範囲外。

Lean生成14fixtureをdebug/releaseの公開planへ渡すと、各binaryで3 match / 11 mismatch。元14ソースは全てCPython compile成功。生成候補のうち同じ11件がSyntaxError、通常の式だけの3件はcompile成功。candidateのraw spanとoriginalの一致も確認した。

既存 `cargo test --offline -p hoimin-cli --test valid_python_corpus` は206 passed、0 failed、3 ignoredだった。既存成功を今回の新しい構文の保証とは扱わない。

Leanの証明はモデル内の性質。全Python構文やRust全体の健全性を証明したとは主張しない。

## 対応案・受け入れ条件

- tuple-to-list候補では、直接のslice要素などリスト内に移せないAST要素を除外する。
- 単一slice+comma、slice/通常式混在、複数slice、start/stop/stepの各形をCPython compile付きの回帰テストへ追加する。
- 通常の式tuple、合法なstarred tuple、ネストした通常のtuple候補は維持する。
- 実runのimport-only fixtureで、SyntaxError候補に由来するkilledがなくなることを確認する。

今回は監査と起票のみ。production修正は行っていない。
