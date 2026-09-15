## 問題

`contains_disallowed_annotation` が複数の型引数を表す `Expr::Tuple` の内部を検査しないため、対象外と定めたAny、object、TypeVar、文字列前方参照、Annotated、Callableを含む注釈へnullable追加を生成する。単一型引数では抑制されるので、同じ要素が型引数の個数によって許可・拒否される。

[型アノテーション設計](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/docs/superpowers/specs/2026-07-20-type-annotation-mutation-design.md)は、対象外構文を含まないTだけを `T | None` へ変えると定める。今回の問題は型引数の再帰検査であり、#548/#558の名前解決や#559の集合ABC名とは異なる。

対象HEAD `5e631ef`、macOS arm64、CPython 3.14.7。debug/releaseとも再現した。

## 最小再現

```python
from typing import Any
x: dict[str, Any]
```

```sh
hoimin plan --root PROJECT --file subject.py --operators type_nullable_add \
  --allow-best-effort-memory --jobs 1 --max-workspace-size 8GiB \
  --min-free-space 10GiB -- true
```

期待は候補0。実際は `dict[str, Any] → dict[str, Any] | None` を生成する。`x: list[Any]` では候補0となり、対象外の同じAnyを正しく除外する。

次の7種類で対象外構文の見落としを確認した。

- `dict[str, Any]`
- `dict[str, object]`
- `dict[str, T]`（TはTypeVar）
- `dict[str, "Foo"]`
- `dict[str, Annotated[int, 'tag']]`
- `dict[str, Callable[[int], str]]`
- `list[dict[str, Any]]`（外側の単一型引数を再帰しても内部tupleで止まる）

これらの元注釈と生成候補はCPythonで正常に評価できる。構文エラーを生むという指摘ではなく、設計上の対象外条件を無視して候補を増やす問題である。型チェッカーのスコアへの影響は未測定。

## 原因と検証

[contains_disallowed_annotation](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L6350) はSubscriptのsliceへ再帰するが、Tupleの分岐がない。複数引数のsliceは `_ => false` となり、子を検査せず「対象外要素なし」と返す。このhelperはannotation_replacementsの入口でも使用される。

Leanで注釈内容を原子・単一子・左右の子の木としてモデル化し、任意の深さの子孫に禁止要素があれば候補を許可しないことを帰納法で証明した。tupleの子を無視する壊した規則は深さ1のpair(good,bad)で検出した。両方goodのpairを保持する正例も確認した。

公開CLI照合では上記7ケースがすべて不一致で、`list[Any]` と通常のint/list/dictの対照は一致した。名前再束縛の別問題も含む全15入力ではdebug/releaseとも4 match / 11 mismatch / 実行基盤エラー0。既存の関連テスト25件は成功しており、現在の単一引数中心の検証ではこの穴を検出できていない。

## 受け入れ条件

- Tupleを含め、型引数内部の対象外要素を全て再帰的に検査する。
- dictのkey/valueの両側、複数段のネスト、別名importに正例・負例を設ける。
- `dict[str, int]` など対象内だけの注釈を保持し、tuple全体を一律に拒否しない。
- 同じhelperを使う他の型演算子への影響を点検し、モデルと公開CLIの回帰を正式な検証へ追加する。

証拠・Lean・全fixtureは作業ツリー `docs/audits/2026-09-15-nullable-gates/` に保存した（起票時点では未コミット）。Leanは小モデル内の構造的性質の証明であり、Python型システムやRust全体の証明ではない。
