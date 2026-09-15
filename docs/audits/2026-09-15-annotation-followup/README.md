# 追加監査: 遅延注釈とcollections.abc.Set

対象HEAD `5e631ef`、2026-09-15、macOS arm64、CPython 3.14.7、Lean 4.32.2。前回の#556/#557とは独立した2件を追加起票した。製品コードは変更していない。

| Issue | 内容 | 公開CLI照合 |
| --- | --- | --- |
| [#558](https://github.com/tokyogas-tech/hoimin/issues/558) | 遅延注釈の評価前にtyping aliasが再代入されても定義時の由来を使う | 6入力中3 mismatch |
| [#559](https://github.com/tokyogas-tech/hoimin/issues/559) | collections.abc.SetをAbstractSetと綴り、AttributeErrorと候補欠落を起こす | 4入力中3 mismatch |

## 契約と最小反例

型演算子は既知の標準型の組合せを変更し、安全なimportを使って置換先を綴る。#264の設計では型注釈を定義時の名前解決で扱っていたが、Python 3.14の既定はアクセス時の遅延評価である。[公式文書](https://docs.python.org/3.14/library/annotationlib.html#annotation-semantics)

```python
from typing import Sequence
def f(x: Sequence[int]): pass
Sequence = set
observed = f.__annotations__['x']
```

この元注釈はset[int]。しかし `type_list_sequence` はSequence[int]→list[int]を生成する。モデル状態は `(current=typing,cached=none)` → rebind `(custom,none)` → observe `(custom,some custom)`。最小反例はrebindの1イベントと必須の末尾observeであり、宣言時snapshotを固定する規則が誤る。期待候補0、実際1。classificationは **confirmed bug**。置換先のSequenceとクラス内の後続束縛でも同じ不一致を確認した。[Issue本文と再現](issue-deferred.md)

再代入前にf.__annotations__を参照すると、評価したtyping.Sequence[int]が保持される。この正例と、初回参照前にtyping importを復元する正例、不変importの正例は候補を保持した。#548で具体型名の遅延評価対策を追加しても、typing aliasの由来と置換先の綴りには別の確認が必要となる。

もう1件の最小入力は `import collections.abc as abc` と `def f(x: set[int]): pass`。期待置換はabc.Set[int]、実際は存在しないabc.AbstractSet[int]となる。正しいABCは[collections.abc.Set](https://docs.python.org/3.14/library/collections.abc.html#collections.abc.Set)であり、[typing.AbstractSet](https://docs.python.org/3.14/library/typing.html#typing.AbstractSet)とは綴りが違う。classificationは **confirmed bug**。直接importしたSetの両方向は候補欠落となる。[Issue本文と再現](issue-abc-set.md)

## 実装との対応

[対応表](correspondence.md)をモデル化前に作成した。[corpus.jsonl](corpus.jsonl)はLeanがソースと期待original/replacement組を生成したもので、手編集していない。

- debug/releaseとも **4 match / 6 mismatch / infrastructure error 0**。[debug観測](replay-debug.json)、[release観測](replay-release.json)
- 比較はoperatorを限定した公開planのoriginal/replacement集合。candidateのspanが元バイト列に一致することも確認した。candidate ID、rank等の全フィールドをLeanと照合したわけではない。
- 元ソース10件のCPython評価は成功。実際の候補は8件で、7件の評価が成功し、abc.AbstractSetの1件はAttributeErrorとなった。これらのruntime結果は補助観測であり、全annotation文字列のLean期待値を別途生成したとは扱わない。
- Leanの期待する7置換を適用すると全件評価成功。候補欠落のSet両方向も含む。[期待置換の実行観測](expected-evaluation.json)
- release公開runはlate_sourceとabc_moduleの2件を実施し、いずれもbaseline成功、killed=1、score=1.0、complete=true、exit=0。abcのテストは `import subject` だけであり、正常なabc.Setの注釈評価は失敗しない。

10fixtureの比較は `strict`。全イベント列と任意長の定理は `model-only` であり、新規の `internal-fixture` はない。CPythonプロセス異常や元fixtureの評価失敗は `infrastructure-error`、生成候補の既知のAttributeErrorは観測対象の意味的失敗として分ける。

## Leanで進めた証明

[AnnotationModel.lean](AnnotationModel.lean)は意味とカーネル検査可能な定理を持ち、[AnnotationMain.lean](AnnotationMain.lean)は独立した有限探索・感度・corpus生成の入口である。正式ライブラリやCIには追加していない。

- 任意長の後続操作について、初回評価で保持した値は再代入・復元・再参照でも変わらない。
- rebind→observeの後ではcustom値を、observe→rebindの後ではtyping値を保持する。
- provider別に選んだ集合抽象型名は、そのproviderのAPI表に存在する。
- abcにAbstractSetというメンバーは存在しない。typingにはAbstractSetとSetの両方が存在し、変異先としてはAbstractSetを選ぶ。

遅延評価のイベントはrebind、restore、observeの3種類。型はtyping/customの2値、キャッシュは未評価または保持値。探索は各列の末尾にobserveを付けて観測を確定し、最短優先・安定順序で深さ0から4まで1段ずつ実行した。対称性の削減はない。

| 深さ（末尾observeを除く） | トレース数 | イベント出現数合計 | stale snapshotとの相違 |
| ---: | ---: | ---: | ---: |
| 0 | 1 | 0 | 0 |
| 1 | 4 | 3 | 1 |
| 2 | 13 | 21 | 4 |
| 3 | 40 | 102 | 13 |
| 4 | 121 | 426 | 40 |

イベント出現数も末尾observeを除く。内部遷移回数は各行にトレース数を足した1/7/34/142/547である。ユニーク状態数は計測していない。各深さの計測はミリ秒分解能で0ms。provider表は2 provider×2 memberの4組を検査した。

感度検査は、定義時snapshot固定、初回キャッシュの喪失、全候補拒否、providerを無視したAbstractSet生成を固定witnessで検出した。評価と再代入の順序・名前の境界に対応する。transactionや永続IDの重複は対象がないため除外した。既存キャッシュを再評価で上書きする壊した規則は、observe→rebindのwitnessで検出する。

モデルのAPI表は前提であり、Leanが標準ライブラリの実装を証明したものではない。CPythonと公式文書で別途確認した。import失敗、動的hook、キャッシュの手動削除、future annotations、Python 3.13以前、async、並行評価は今回のモデル外。

## 再現と検証

repository rootで、出力先には新規パスを指定する。

```sh
python3 docs/audits/2026-09-15-annotation-followup/verify_lean.py --output /tmp/annotation-proof-recheck
python3 docs/audits/2026-09-15-annotation-followup/replay.py --binary target/release/hoimin --output /tmp/annotation-release.json --full-run
python3 docs/audits/2026-09-15-annotation-followup/replay.py --binary target/debug/hoimin --output /tmp/annotation-debug.json
python3 docs/audits/2026-09-15-annotation-followup/evaluate_expected.py --output /tmp/annotation-expected.json
cargo test --offline -p hoimin-cli --test collection_annotation_builtins --test operator_function_contracts
cargo test --offline -p hoimin-cli --test lean_progress_input_oracle --test lean_progress_decision_oracle --test plan
```

既存Rustテストは5+18+4+3+72=102件成功、子プロセスfixture1件ignored。plan/verifyの入力・候補・順位検証とprogressの入力・比較処理も読んだが、この追加調査から独立した再現付きIssueは得られなかった。全workspaceの検証ではない。

Leanは各プロセス20秒・2GiB・50msのRSS監視、1スレッド、既定heartbeat上限を保持して直列実行した。最終の[モデル統計](verification/model.json)、[探索統計](verification/search.json)、[探索・感度ログ](verification/search.log)に時間とpeak RSSを保存した。最終検証は全てexit=0で、timeout/OOMはなかった。再現スクリプトの初回配置では相対作業ディレクトリを誤ったが、正しいrootで配置し直し、以後の検証で成功を確認した。

`replay.py`は監査用で、mismatchを記録してもexit=0、infrastructure errorなら2となる。修正後に正式CIゲートへ移す際は、期待値の維持と比較失敗時の終了処理を追加する。

## 未解決事項

#558では評価時期を考慮したimport契約と、静的に保証できない場合の保守的な除外方針を決める。#559ではproviderごとの綴りと直接import対応を修正する。スカラー名のshadowingによるnullable候補も調べたが、今回の入力では元から通常の型でない値を注釈としていたため、独立したIssueにはしなかった。

新しい性能測定・性能Issueは追加していない。前回#557の測定を今回の結果として数えていない。新規モデルと再現コードは未コミットで、製品修正・全Leanモジュール・Windows/Linuxの検証は今回の範囲外である。
