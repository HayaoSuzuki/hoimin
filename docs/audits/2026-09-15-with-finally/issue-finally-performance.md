## 改善余地

型注釈解析の `apply_finally` が、入れ子のfinalbodyを記録用と転送用に繰り返し走査する。2.9KiB・候補1件の有効Pythonでも、finally深さ20でrelease planに約1.15秒を要する。深さ16〜20では、1段追加するたびに所要時間がほぼ倍増する。

対象HEAD `5e631ef`、macOS arm64。`cargo build --offline --release -p hoimin-cli` でビルドしたbinaryを使用。#546の入れ子ループ修正後にも残る、finally固有の経路である。

## 再現

次のスクリプトをrepository rootから実行する。各入力はCPythonのcompileも通し、各planを10秒で打ち切る。入力は一時ディレクトリに限定する。

```python
import json, pathlib, statistics, subprocess, tempfile, time
binary = str(pathlib.Path('target/release/hoimin').resolve())
with tempfile.TemporaryDirectory() as root:
    path = pathlib.Path(root) / 'subject.py'
    for depth in range(16, 21):
        source = 'value: Sequence[int]\n'
        for _ in range(depth):
            source = 'try:\n    pass\nfinally:\n' + ''.join(
                '    ' + line + '\n' for line in source.splitlines())
        source = 'from typing import Sequence\n' + source
        compile(source, '<benchmark>', 'exec')
        path.write_text(source)
        samples = []
        for _ in range(3):
            start = time.monotonic()
            result = subprocess.run([
                binary, 'plan', '--root', root, '--file', 'subject.py',
                '--allow-best-effort-memory', '--operators', 'type_list_sequence',
                '--', 'true'], capture_output=True, text=True, timeout=10, check=True)
            samples.append(time.monotonic() - start)
            assert len(json.loads(result.stdout)['candidates']) == 1
        print(depth, len(source.encode()), statistics.median(samples))
```

| finally深さ | source bytes | release plan中央値（秒、3回） |
| ---: | ---: | ---: |
| 16 | 1921 | 0.0809 |
| 17 | 2140 | 0.1549 |
| 18 | 2371 | 0.2971 |
| 19 | 2614 | 0.5900 |
| 20 | 2869 | 1.1510 |

10〜20を1段ずつ測定した。いずれもexit=0、候補1件。入れ子の生成コードは人工的な性能fixtureであり、通常プロジェクトの平均性能の主張ではない。

## 原因

[apply_finally](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L5428) は、入口を合流した状態で `visit_suite_from(annotation_entry, finalbody)` を実行し、その後各exitごとに `route_finally_entry` で同じfinalbodyを走査する。

`route_finally_entry` は `record_annotations=false` にするが、内側の `apply_finally` は既に記録無効でも最初の走査を省略しない。このfixtureにはnormal exitが1種類しかなくても、各入れ子で二重走査が起きる。#546で追加したloop transferの再利用はこの経路には適用されない。

## Leanの結果と限界

「各finalbodyが二度子のfinalbodyへ降りる」というコストモデルで、最深部の訪問数 `visits 0 = 1; visits (n+1) = 2 * visits n` に対して `visits n = 2^n` を全自然数で証明した。転送を1回にする比較モデルではleaf訪問数が常に1となる。二重走査を再導入したモデルは深さ1の感度検査で検出する。

この式は明示した走査モデルの定理。Rust内部の訪問カウンタとの照合は未実施であり、releaseの所要時間を厳密な訪問数の観測とは扱わない。今回のmemory使用量の改善は主張しない。

モデル・全測定値・コーパスは作業ツリーの `docs/audits/2026-09-15-with-finally/` に保存した（起票時点では未コミット）。

## 受け入れ条件

- 記録無効の転送処理から、不要な注釈記録用の再走査を発生させない。必要に応じて記録と転送、転送結果の再利用を分離する。
- nested finallyを性能fixtureに加え、実際のstatement/annotation訪問カウンタで指数的な増加の再発を検出する。
- releaseで深さ16〜20の改善を確認し、候補descriptorと順序を保持する。
- 複数exit、暗黙例外、finallyによるreturn/break/continue/raiseの上書きについて既存Lean oracleと回帰テストを維持する。
