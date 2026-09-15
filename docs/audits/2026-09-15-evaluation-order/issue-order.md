## 問題

builtinの名前解決がソース中の文字位置を実行順序として使うため、代入済みの利用者関数を組込み関数として変異する。多重代入の後続target、RHSのwalrusが先に実行されるtarget、keywordより後ろに書かれたstarred引数で再現する。sourceとdestination両方に影響する。

対象HEAD `5e631ef`、macOS arm64、CPython 3.14.7。debug/release両方で再現した。#556〜#559の既報とは別であり、今回の入力に型注釈や例外処理はない。

## 最小再現

`subject.py`:

```python
def custom(values): return 'custom'
slots = {}
any, slots[any([])] = custom, 7
observed = list(slots)
```

```sh
hoimin plan --root /path/to/project --file subject.py \
  --allow-best-effort-memory --operators collection_any_all -- true
```

期待: 候補0。先にanyへcustomが代入され、後続target内の `any([])` は利用者関数を呼ぶ。実際: `any → all` の1候補。元ソースのobservedは `['custom']`、候補適用後は `[True]` となる。

`check.py`を次の内容にすると、元ソースは成功する。公開runはbaseline成功、killed=1、score=1.0、complete=true、exit=0となった。

```python
import subject
assert subject.observed == ['custom']
```

```sh
hoimin run --root /path/to/project --file subject.py \
  --allow-best-effort-memory --operators collection_any_all \
  --format json -- /absolute/path/to/python3.14 check.py
```

## 同じ原因の境界入力

次の各行も同じprefixで、any→allの候補を生成するが、本来抑制が必要となる。

```python
any = slots[any([])] = custom                 # 連鎖代入、左側targetから順に格納
slots[any([])] = (any := custom)               # RHSがtarget式より先に実行
all, slots[any([])] = custom, 7                # destinationのallが先にcustomになる
```

引数では、次のanyもcustomを呼ぶ。source中ではwalrusより前でも、starred引数がkeyword引数より先に実行される。

```python
def custom(values): return 'custom'
def sink(*args, **kwargs): return kwargs['flag']
observed = sink(flag=any([]), *[(any := custom)])
```

対照の `any, slots[0] = any([]), 7` は、RHSで組込みanyを参照してから格納するので、候補を保持する必要がある。単に同じ文の全束縛を事前に無効化すると、この正例を失う。

## 原因

[NameResolutionBuilder::visit_stmt](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L1646) はAssignの全targetについて、文末offsetに束縛を記録する。後続targetの式内にあるlookupはこのoffsetより前なので、既に実行された格納が反映されない。

`record_named_target`もwalrusの文字位置で効果を記録し、`resolve_ordered_at`はlookupの文字位置で履歴を検索する。したがってRHS→targetやstarred→keywordの実行順序を文字位置から復元できない。#482の索引自体の検索速度ではなく、索引キーが表す時点の問題である。

Pythonの規則では、RHSを評価した後、targetへ左から順に格納する。[代入文の公式仕様](https://docs.python.org/3/reference/simple_stmts.html#assignment-statements)。starred引数がkeyword引数より先に処理される例外も明記されている。[呼出しの公式仕様](https://docs.python.org/3/reference/expressions.html#calls)

## 検証と受け入れ条件

Leanでsource束縛、destination束縛、lookupのイベント順序を分け、いずれかをshadowした後の任意長の後続操作で候補を許可しないことを証明した。lookup→bindの正例も保持し、書込みを文末へ遅延する壊した規則を深さ2で検出した。

7つのLean生成入力を公開planで照合し、debug/releaseとも2 match / 5 mismatch。全元ソース・全候補のCPython実行は成功し、意味の違いを観測した。関連する既存scopeテスト27件は成功した。Rust実装全体の形式証明ではない。

- ソースspanと評価時点を区別し、target格納、RHS→target、starred→keywordの順序で束縛状態を照会する。
- source/destination双方のshadowingを抑制し、RHSでの先行lookupなど正しい候補を保持する。
- 同じresolverを使う他のbuiltin pairでも検証する。
- 例外による途中停止、nested target、augmented assignmentの違いを考慮し、既存comprehension/loop/type-parameterの評価順序を維持する。

証拠・再現コードは作業ツリー `docs/audits/2026-09-15-evaluation-order/` に保存した（起票時点では未コミット）。
