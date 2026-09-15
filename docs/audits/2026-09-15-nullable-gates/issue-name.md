## 問題

`type_nullable_add` がint/strなどの綴りだけで対象を判定し、先行する再代入や同名の利用者定義クラスを考慮せず `T | None` を生成する。listのsubscriptにも同じ抜けがある。

[型アノテーション設計](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/docs/superpowers/specs/2026-07-20-type-annotation-mutation-design.md)では、既知の型構成子だけを対象とし、任意の利用者定義型・型エイリアスは除外する。#548のcollection変異にはbuiltin解決の確認が入ったが、nullable追加の経路には入っていない。

対象HEAD `5e631ef`、macOS arm64、CPython 3.14.7。debug/releaseとも再現した。

## 再現

`subject.py`:

```python
class Meta(type):
    def __or__(cls, other):
        raise RuntimeError('custom union')

class int(metaclass=Meta):
    pass

x: int
```

```sh
hoimin plan --root PROJECT --file subject.py --operators type_nullable_add \
  --allow-best-effort-memory --jobs 1 --max-workspace-size 8GiB \
  --min-free-space 10GiB -- true
```

期待は候補0。実際は `int → int | None` を生成する。元の注釈xは利用者定義クラスとして評価できるが、候補適用後はcustom unionのRuntimeErrorとなる。Pythonの型オブジェクトの `|` はメタクラスの `__or__` の影響を受ける。[公式仕様](https://docs.python.org/3/library/stdtypes.html#types-union)。

より小さい境界として `int=7; x:int`、`str=7; x:str`、`list={int:7}; x:list[int]` でも候補が残る。これらはCPythonの注釈評価では元が7、変異後がTypeErrorとなる。数値の注釈を型チェッカーが受理することは主張しない。今回の根拠は候補の適用範囲と参照先であり、型チェッカーのbaselineやスコアは測定していない。

## 原因と検証

[nullable_add_allowed / is_supported_annotation](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L6314) はNameの綴りとKnownImportsの名前表現だけを見る。`facts.name_resolution.annotation_resolution`を使うcollection変異の条件がnullable追加にない。さらに既存のMUTABLE_BUILTINSにはstr/int/float/bool/bytesが含まれないため、resolverを呼ぶだけでなく追跡対象も確認する必要がある。

Leanで「候補許可なら参照先がtrusted」を証明し、名前解決の条件を落とす壊した規則を最小の原子型で検出した。公開planとの照合では名前再束縛4ケースがすべて不一致。通常のint、list[int]、dict[str,int]は候補を保持した。型引数内部の別問題も含む全15入力ではdebug/releaseとも4 match / 11 mismatch / 実行基盤エラー0だった。関連する既存テスト25件は成功した。

## 受け入れ条件

- nullable追加の対象名・型構成子を注釈scopeに従って解決し、再束縛した名前と利用者定義型を除外する。
- str/int/float/bool/bytesとlist/set/dictで確認し、正常な組込み型の候補は維持する。
- module/class/function/type-parameterの境界と遅延注釈の評価時期を、既存resolverの契約に合わせて扱う。
- #548のcollectionガードや#558のtyping aliasの後続再代入と区別し、このnullable経路の回帰を追加する。

Leanは適用条件の小モデル内の証明であり、Rust全体や型の意味の証明ではない。証拠と再現スクリプトは作業ツリー `docs/audits/2026-09-15-nullable-gates/` に保存した（起票時点では未コミット）。
