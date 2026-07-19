# 型アノテーション mutation

## 目的

型検査が通ることだけでは、型アノテーションが意図した契約を十分に表しているかは分からない。
型アノテーションを一箇所だけ変え、利用者が指定した型チェッカーを実行することで、変異後も通過する注釈を `survived` として報告する。

この機能は実行時の振る舞いを測るものではない。
利用者は `--` の後ろに `uv run ty check`、`uv run mypy src`、または同等の型検査コマンドを渡す。
hoimin は既存の baseline と mutant 実行の仕組みを使い、型チェッカーの終了コードが 0 以外なら mutant を `killed`、0 なら `survived` とする。

## 型アノテーションの対象範囲

解析器は Ruff の AST から型アノテーションを識別する。
対象にする位置は、関数とメソッドの引数注釈、戻り値注釈、注釈付き代入、クラス属性の注釈である。
モジュール変数と関数本体のローカル変数も、注釈付き代入として含める。

`--line` は注釈の開始行で候補を選択する。
`--symbol` は既存のスコープ判定を使い、関数またはメソッドに属する引数、戻り値、ローカル変数の候補を選択する。
クラス属性はクラスのシンボルに属する。

型コメント、文字列リテラルとして書いた前方参照、`typing.Annotated` のメタデータ、利用者定義の型エイリアスは初版の対象外とする。
文字列化された前方参照は局所置換後の解釈が型チェッカーの設定に依存し、型コメントは構文上の注釈と別の扱いを要するためである。
利用者定義の型エイリアスは、名前だけから元の型の意味を復元できない。

## 初期の型演算子

型演算子は、候補数とノイズを抑えるため、標準ライブラリの既知の型構成子だけを一段階だけ変える。
すべての mutant は一箇所だけを変更する。

- **nullable**：`T | None` と `Optional[T]` を `T` に置き換える。
- **nullable**：対象外に列挙した構文を含まない `T` を `T | None` に置き換える。
- **collections**：`list[T]` と `Sequence[T]` を相互に置き換える。
- **collections**：`set[T]` と `AbstractSet[T]` を相互に置き換える。
- **collections**：`dict[K, V]` と `Mapping[K, V]` を相互に置き換える。
- **iterables**：`Iterable[T]` と `Iterator[T]` を相互に置き換える。
- **iterables**：`Sequence[T]` と `Iterable[T]` を相互に置き換える。

`typing`、`collections.abc` の修飾名と、そこから直接 import した名前を認識する。
たとえば `typing.Optional[T]` と `Optional[T]` は同じ nullable 演算子の対象にする。
直接 import は AST の import 文で標準ライブラリから導入されたことを確認し、同名の利用者定義名は対象にしない。

`Any`、`object`、任意の利用者定義型、`TypeVar`、`Protocol`、`Callable`、`Literal`、`Annotated`、共変性や反変性を含む型パラメータは初版では変異しない。
これらは一律の強弱関係を定められないか、候補の大半が型検査器固有の名前解決エラーになるためである。

## 演算子の選択

`--operators` を追加し、既存演算子と型演算子の実行対象を選べるようにする。
値は個別演算子名に加え、`type_nullable`、`type_collections`、`type_iterables` の系統名を受け付ける。
`--exclude-operators` は選択した集合から個別演算子または系統を除外する。
`--operators` を省略した既存の実行は、現在の全既存演算子を選び、型演算子は選ばない。

型演算子を指定しない既存の実行では、候補、fingerprint、結果を変更しない。
型アノテーションだけを測る利用者は、たとえば次のように実行する。

```console
hoimin run --root . --source src --operators type_nullable,type_collections -- uv run ty check
```

選択された演算子の正規化済み集合は、run の fingerprint、session 互換性、結果の `operator` フィールドに含める。
新しい演算子名は analyzer JSONL protocol の許可リストにも追加する。

## 解析と候補の生成

解析器は AST で注釈の式とその親の位置を取得する。
各演算子は、その式が既知の構文形式と import 形態に一致するかを判定し、置換する最小のバイト範囲を生成する。
候補は既存の byte span、元の文字列、置換文字列、行、列、シンボルを使う。

候補を作った後、既存と同じ Python 構文解析によって置換後のソースを検証する。
構文として不正な候補は実行せず、解析診断として報告する。
型チェッカーが変異後の型をエラーと判断した場合は通常の `killed` であり、解析診断ではない。

型チェッカーの設定不備、依存関係不足、またはクラッシュは baseline で検出する。
baseline が終了コード 0 以外なら、既存の契約どおり mutation を開始せず終了コード 3 を返す。

## 結果と再開

型演算子の候補は、既存の candidate ID と同じ構成要素で識別する。
演算子 ID と置換文字列が ID に含まれるため、同じ位置の nullable 変異と collection 変異は別の mutant になる。

operator 選択は実行条件の fingerprint に含める。
そのため、型演算子を追加または除外した run は、以前の session の結果を誤って再利用しない。
JSON、JSON Lines、SQLite は既存の `operator` 文字列を保持する形式を使い、追加の結果種別は設けない。

## テスト

Rust の解析器テストは、各型演算子について候補の span、元の文字列、置換文字列、行、列、演算子名、安定順を確認する。
直接 import と修飾名、ネストしたジェネリクス、Unicode、コメント、CRLF、非対象の型構文を fixture に含める。

対象選択のテストは、`--line`、`--symbol`、関数ローカル変数、クラス属性を確認する。
protocol、session、resume のテストは、新しい演算子を含む候補の受理と、operator 選択の変更で互換性が失われることを確認する。

E2E fixture は `ty` と `mypy` の両方を使う。
nullable または collection の注釈を変えた mutant が型エラーになり `killed` となる例と、設定上見逃されて `survived` となる例をそれぞれ固定する。
型チェッカーの実行環境を増やしすぎないため、各チェッカーの fixture は最小の独立プロジェクトにする。




