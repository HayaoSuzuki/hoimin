## 確認した改善余地

#479でannotationごとのimport snapshotの保持は解消されたが、型解析が有効な経路では文の転送・exit受渡しで `KnownImports` 全体のdeep cloneが残っている。候補0件でも、import幅 I × 注釈数 A に比例するコピーが発生する。

確認対象: `f11013542ccd735ab9741b5079c0b39a517df256`、macOS arm64、同HEADのrelease CLI。優先度案: P2。

## 再現

```python
from pathlib import Path
n = 2048
source = ''.join(f'import typing as t{i}\n' for i in range(n)) + 'x: int\n' * n
Path('subject.py').write_text(source)
```

```sh
hoimin plan --root . --file subject.py --operators type_list_sequence \
  --allow-best-effort-memory --analyzer-timeout 5s -- true
```

`int`に対するtype_list_sequence候補はなく、全件exit 0・候補0・truncated=false。

| import数 = 注釈数 | 入力bytes | 型解析あり、ms | 型解析なし、ms |
| ---: | ---: | ---: | ---: |
| 512 | 14,738 | 43 | 44 |
| 1,024 | 29,610 | 129 | 44 |
| 2,048 | 60,330 | 409 | 43 |

各条件3回の中央値。型解析なしは `--operators boolean_literal`。既存resource guardで1実行10秒・1GiB、10ms間隔のRSS監視を行った。時間には監視の固定費があり、小入力の差は解像できない。2,048の型解析ありのサンプルRSSは約11〜13MiBで、以前のsnapshot蓄積による数百MiBの保持とは区別する。

## 原因・既存修正との差

- [visit_statement_flowのAnnAssign](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/crates/hoimin-cli/src/analyzer/rust.rs#L5286) は注釈をcallbackした後も `ControlFlowExits::fallthrough(self.imports.clone())` を実行する。
- [visit_suite_flow](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/crates/hoimin-cli/src/analyzer/rust.rs#L4685) は各statementの `fallthrough.clone_from`、末尾のcloneを行う。
- `KnownImports` のproduction版はHashMap/HashSetを持つderive(Clone)。取り扱うI個のimport名を文ごとにコピーする。import構築中にも状態幅に応じたコピーがある。
- #479はsnapshotの保持量と型演算子未選択経路を修正した。本件はその後に残った一時コピーのCPUコストを追跡する後続Issue。
- #491のcost oracleが計数するannotation callback内のcloneは0のままでも、本件の文転送のcloneは検出できない。ゲートを全collectorの計数へ広げる必要がある。

Leanで一回/annotationのコピーだけを数える下界 `I*A` と、両次元を2倍にすると4倍になる性質をカーネル検査した。内部Rust計数との比較は未実施なのでmodel-onlyであり、CLIの時間保証へ読み替えない。

## 対応案・受け入れ条件

- 単一路の文転送は所有権を移し、合流で必要になる状態だけを共有・コピーする設計を検討する。
- IとAを独立に増やし、collector全体のclone呼出し数・コピーentry数を測る決定的ゲートを追加する。
- 候補0の対照に加え、実候補と分岐・loop・finallyを含むfixtureで意味の同値性を確認する。
- #479の保持量改善を維持し、再び全snapshotを保持する方式へ戻さない。

今回は監査と起票のみ。
