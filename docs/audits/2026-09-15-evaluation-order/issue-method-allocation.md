## 改善余地

未選択のappend系method変異でも、呼出し全体のreplacement文字列を構築してから捨てている。`--operators binary_add_sub --max-candidates 1` と限定しても、約500KBの文字列を含むappend呼出し32段で、500KB以上の成功した確保要求の累積量が97,040,138bytesとなった。同じ長さ・AST形状の未対応method `ignore` は1,000,394bytesだった。

改善指標は累積確保要求量であり、peak RSSではない。今回のdebug時間差は小さく、高速化の割合は主張しない。

対象HEAD `5e631ef`、macOS arm64。#461の修正はlist/tuple literalのhelper guardとoriginal複製の遅延であり、今回のmethod呼出し用helperにはguardがない。同じ改善方針を別producerへ適用する後続Issue。

## 再現する入力

```python
from pathlib import Path
depth = 32
source = 'obj.append(' * depth + "('" + 'a' * 500000 + "', 1+2)" + ')' * depth + '\n'
compile(source, '<fixture>', 'exec')
Path('subject.py').write_text(source)
```

```sh
hoimin plan --root /path/to/project --file subject.py \
  --allow-best-effort-memory --operators binary_add_sub --max-candidates 1 -- true
```

`append`を同じ長さの`ignore`へ置き換えたものを対照とする。objは実行せず、公開analyzerで静的解析する。選択された加算は全呼出しの内側にあり、両入力とも加算→減算の1候補だけを返す。

## 実測

HEADのdebugライブラリの公開 `analyzer::discover_targets` を、System allocatorの成功したalloc/alloc_zeroed/realloc要求を数える独立Rustプログラムから呼んだ。thresholdは500000bytes。fixture作成とruntime構築は計測区間外。全8条件で候補1件、original=`+`、replacement=`-`をassertした。

| 深さ | source bytes（両method同一） | appendの大きな確保回数 | appendの累積bytes | ignoreの回数 | ignoreの累積bytes |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 500022 | 6 | 4000148 | 2 | 1000022 |
| 8 | 500106 | 34 | 25003130 | 2 | 1000106 |
| 16 | 500202 | 66 | 49010858 | 2 | 1000202 |
| 32 | 500394 | 130 | 97040138 | 2 | 1000394 |

確保回数・累積量は2回の独立したprobe実行で一致した。debug計測の経過時間は概ね37〜44msで、速度の改善率を推定する測定ではない。release rlibへ直接linkしたprobeはlinkに失敗したため、releaseの確保量は未検証。

allocatorの方式は#461に記載されたものと同じで、今回のfixtureへ置き換えた。作業ツリーの `docs/audits/2026-09-15-evaluation-order/alloc_probe.rs` と `measure_allocations.py` に全コードを保存した。再現コマンドは `python3 docs/audits/2026-09-15-evaluation-order/measure_allocations.py --output /tmp/method-allocations.json`（HEADのdebug rlibが必要、資料は起票時点で未コミット）。

## 原因と受け入れ条件

[collect_method_call / collect_structural_method_call](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L2507) が `append_to_insert_replacement` と `append_to_extend_replacement` をoperator選択の確認前に呼ぶ。`replace_within_call`が対象call全体を複製した後で、`make_candidate`のoperator gateが破棄する。innerの大きな文字列を各親callが含むため、深さに応じて累積要求が増える。

- methodのreplacement helperを呼ぶ前に、対応operatorが選択されていることを確認する。
- 未選択の親callでも、子にある選択済み演算子の探索は続ける。subtree全体をskipしない。
- append/extend/insert/get/sort等、全範囲を書き換えるhelperを点検する。
- 実際のhelper入口・確保境界のカウンタで回帰を検出し、候補descriptor・ID・順序・truncationを保持する。
- 確保の累積量、同時保持量、経過時間を区別して改善を評価する。
