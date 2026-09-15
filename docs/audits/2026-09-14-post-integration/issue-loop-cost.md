## 確認した問題

型注釈解析が有効なとき、入れ子の各ループで本体を固定点計算と候補収集のために再走査する。import状態が空で最初の反復で固定点に達する入力でも、最深部の訪問回数はモデル上 `2^depth` になる。

確認対象: `f11013542ccd735ab9741b5079c0b39a517df256`、macOS arm64、同HEADのrelease CLI。優先度案: P2。483 bytes・候補0件の有効なPythonで約0.65秒かかる。

## 再現

```python
from pathlib import Path
n = 20
source = ''.join(' ' * i + 'for _ in []:\n' for i in range(n)) + ' ' * n + 'x: list[int]\n'
compile(source, 'subject.py', 'exec')
Path('subject.py').write_text(source)
```

```sh
hoimin plan --root . --file subject.py --operators type_list_sequence \
  --allow-best-effort-memory --analyzer-timeout 5s -- true
```

対照は `--operators boolean_literal`。全件 exit 0、候補0、truncated=false。planはループを実行しない。

| ループ深さ | 入力bytes | 型解析あり、ms | 型解析なし、ms |
| ---: | ---: | ---: | ---: |
| 18 | 418 | 182 | 42 |
| 19 | 450 | 345 | 43 |
| 20 | 483 | 649 | 45 |

各条件3回の中央値。1実行10秒・1GiBの既存resource guardを使い、10ms間隔のRSSサンプリングを行った。時間は監視起動・終了確認を含む。サンプルRSSは約6MiBで大幅な増加なし。時間と操作数の問題であり、OOMを観測したという主張ではない。

## 根拠

- [visit_loop](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/crates/hoimin-cli/src/analyzer/rust.rs#L4850) は `loop_head_fixed_point` の後、同じ本体をもう一度 `visit_suite_from` する。
- [loop_head_fixed_point](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/crates/hoimin-cli/src/analyzer/rust.rs#L4879) は `record_annotations=false` でも入れ子のループを再帰的に走査する。注釈の記録を止めるだけでは、子ループの再走査は止まらない。
- 既存 #478 のAST深度128はスタック保護で、この処理量を抑えない。今回の20ループはその範囲内。
- #479 の逐次callbackと型演算子未選択の早期returnは有効。今回の問題は型演算子を選択した経路で残る。

Leanの小さなコストモデルで `visits(0)=1`, `visits(n+1)=2*visits(n)` から `visits(n)=2^n` をカーネル検査した。ただし内部訪問回数とのadapter照合は未実施のため、この部分はmodel-only。上表は独立した実CLI測定である。

追加確認: 同じ深さ20を `--analyzer-timeout 10ms/100ms` で実行すると、それぞれ26ms/106msでexit 2とtimeout診断を返した。timeout不動作は確認していない。深さを20より増やしていない。

## 対応案・受け入れ条件

- 入れ子の再解析を避ける転送要約・メモ化などを検討し、固定点計算の意味を維持する。
- 小さい深さから訪問回数を数える決定的なゲートを追加し、型解析なしの対照も残す。
- importが変化するループ、continue/back-edge、break、finallyの既存Lean対応試験を維持する。
- 性能台帳へ「幅・式深さ」と別に「入れ子の制御フロー再解析」を登録する。壁時計の閾値だけで合否を決めない。

今回は監査と起票のみ。
