## 改善したい挙動

値を伴わない名前の型注釈を実行時の再代入と区別し、builtinや無条件module importのaliasについて、既知の参照先を維持したい。

既存の[collection operator設計](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/docs/superpowers/specs/2026-08-06-collection-and-structural-mutation-operators-design.md)は保守的な候補欠落を許容している。そのため、今回の起票は候補精度の改善であり、不適切な候補生成の安全性違反ではない。

対象HEADは `5e631ef`、macOS arm64、CPython 3.14.7。debug/release両方で確認した。#558は遅延注釈評価と実際の再代入、#560は代入の評価順序であり、今回の値を代入しない注釈とは条件が異なる。

## 最小再現

`subject.py`:

```python
any: int
observed = any([])
```

```sh
hoimin plan --root /path/to/project --file subject.py \
  --operators collection_any_all --allow-best-effort-memory \
  --jobs 1 --max-workspace-size 8GiB --min-free-space 10GiB -- true
```

実際は候補0件。注釈行を除くと `any → all` の1候補となる。どちらの元ソースもobservedはFalseであり、値なし注釈ではanyの束縛は作られない。改善後は両方とも候補を保持できる。提案する置換を適用したPythonは正常に実行でき、observedがTrueになる。

同じ候補欠落を次の入力でも確認した。

```python
all: int  # 置換先の注釈だけでも any → all が消える
observed = any([])
```

```python
class C:
    any: int
    observed = any([])
```

```python
import operator as op
op: object
observed = op.add(2, 1)
```

最後の例は `--operators operator_function` で候補0件となる。注釈がなければ `add → sub` が生成される。`from operator import add as fn; fn: object` でも同様。

## 関数scopeとの区別

[Python公式仕様](https://docs.python.org/3.14/reference/simple_stmts.html#annotated-assignment-statements)は、値を伴うときの代入と、関数scopeでのローカル名宣言を区別する。次の関数はUnboundLocalErrorとなるため、単にAnnAssign全体を無視する変更は適切でない。

```python
def f():
    any: int
    return any([])
```

実際の値が付く `any: object = custom`、先にcustomへ束縛した名前への値なし注釈、classの外側でcustomへ束縛した名前も候補を抑制する。関数内のlocal importは既存operator設計の対象外であり、今回の拡張範囲には含めない。

## 実装箇所

- [NameResolutionBuilder::visit_stmt](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L1709) は、AnnAssignのvalue有無にかかわらず `record_target` を呼び、実行時の履歴・possible_bindingsに反映する。
- [operator_functions::ImportScan::visit_expr](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust/operator_functions.rs#L491) は、注釈targetのStoreを通常の再束縛として数える。この経路はbuiltin resolverと別である。
- 同じファイルの型注釈用 `KnownImports::transfer_statement` は、既に `assign.value` がSomeのときだけ束縛を更新する。この区別を参考にできる。

## 検証と受け入れ条件

Leanでscope、局所値、外側へのfallback、RHS有無を分離した。値なし注釈が局所値・名前解決を保持すること、任意回数の反復でも値を保持すること、関数ローカルの未束縛名が外側へfallbackしないことをモデル内で証明した。

3 scope × 3 value × 2 fallback × 2 RHSの36ケースで、常に再代入する・RHSを無視する・関数で外側へfallbackする壊した規則を検出した。Lean生成14入力を公開planと照合し、debug/releaseとも7 match / 7 mismatch / 実行基盤エラー0。mismatchは改善案との不一致であり、現行設計の完全性保証への違反を意味しない。全元ソースと提案する10置換はCPythonで正常実行でき、置換後の観測値が変わった。既存の関連テスト27件も成功した。

- module/classの値なし名前注釈で、source/destinationの組込み参照を保持する。
- 無条件module importのaliasについて、値なし注釈だけで同一性を失わない。
- 値を伴う代入、関数のローカル宣言、既存のshadowingと動的namespaceの抑制を維持する。
- 属性・subscript targetは評価副作用があるため、AnnAssignの子式走査を一律に省略しない。
- 正例と負例を正式な回帰検証へ追加する。LeanモデルはRust全体の正しさの証明ではない。

再現コード・証拠は作業ツリー `docs/audits/2026-09-15-declaration-only/` に保存した（起票時点で未コミット）。`verify_lean.py` でコーパスを確認し、`replay.py --binary target/release/hoimin --output /tmp/declaration-release.json` で公開CLI照合を再実行できる。
