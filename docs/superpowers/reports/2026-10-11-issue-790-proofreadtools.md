# proofreadtoolsの変異候補探索とテスト生成の評価

## 結論と範囲

proofreadtoolsの固定した1関数で、既存operatorとは別の単一AST編集から生存候補を見つけ、その候補を検出する具体例テストを生成できた。
対象は `409ed955c0695ec04976842ba9fd22c64c03c33a` の `_length_ratio_upper_bound`。
Windows、CPython 3.14.6での開発用実験であり、通常CLI・配布仕様・既定operatorは変更しない。
版は0.3.xのままとする。

| Issue | 判断 |
| --- | --- |
| #790 | 制約付き単一編集のPoCとして成立。小さい有限空間では誘導の優位を主張できず、汎用探索器・既定機能への採用は見送る。 |
| #776 | 信頼する固定純粋関数でのHypothesis入力探索と具体例テスト生成は有効。仕様確認を要する開発用手順に限定する。 |
| #777 | 同じ候補でCrossHairの差をCPythonでも再現できた。今回の単純な関数ではCLI費用が大きく、製品組込みは見送る。 |

本報告は製品機能を完成したという意味ではない。
生成テストの人による採用は未確認であり、sample repositoryへのPRや自動変更は行っていない。
#763・#764・#712、未実測OS、重いモデル、任意の関数・strategy受付は対象外。

## 候補と具体入力

元の式は `2 * min(len(x), len(y)) / (len(x) + len(y))`。
発見した変異は `min(len(x), len(y))` を `(len(x))` に置き換える1編集。
既存の関連6テストでは生存した。
Hypothesisは `x='aa', y='a'` を返し、元の値は `0.6666666666666666`、変異の値は `1.3333333333333333` だった。
CrossHairの文字列も同じ長さのASCIIへ投影すると同じ差を再現できた。

生成候補は次の通常pytestテストである。
期待値は元実装から得たcharacterizationであり、人が仕様を確認してから採用する。
数式上も `2 * min(2, 1) / (2 + 1) = 2/3` となるが、この1例だけで仕様全体を保証しない。

```python
from proofreadtools.checks._variant_candidates import _length_ratio_upper_bound


def test_generated_length_ratio_regression():
    assert _length_ratio_upper_bound('aa', 'a') == 0.6666666666666666
```

元・変異を各2回、別の通常CPythonプロセスで実行し、読み込んだsource pathもworker内の対象ファイルと照合する。
テスト追加後はtestsのfingerprintを含めてplanを再生成し、新しいworkerでoriginal-pass/mutant-assertion-failを確認する。
通常のmutation scoreへ研究候補の結果を混ぜない。

## 比較方法

固定方式は今回の作業ツリーからビルドしたhoiminのplanを使う。
PATHに残るhoimin 0.1.0は使わない。
生成方式はASTに現れる同じオペランド・引数と許可した演算子から18候補を作る。
独立した複数編集は合成しない。
構文と既存CandidateDescriptorのsource hash・span・originalを検証してからWorkerWorkspaceで適用する。

無作為と誘導は同じ集合からproptestでindexを生成する。
誘導は候補種別の初期重み1に、有限な差の発見と初出の種別それぞれ1を加える。
生存数だけを報酬にしない。
これは有限な編集集合の選択制御であり、新しい文法を学習するプログラム合成ではない。
候補の縮小は式のASTノード数・replacement長の順序を使うValueTreeで行い、生存と有効な差を失う縮小を棄却する。
Hypothesisは公開strategyと生成・縮小phaseを使い、非公開Conjecture APIには依存しない。

seedは790・776・777。
各方式の上限は60秒・32候補起動で、生成、既存テスト、差分入力探索、候補縮小を算入する。
有限集合を使い切った場合は実件数と時間を報告する。
baseline、plan、最終照合、レビューは探索と別費用で記録する。

## 最終測定

| 方式 | seed | 通常評価 | 縮小評価 | 生存・有限差 | wall秒 | 起動数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| fixed | 790 | 6 | 0 | 0 | 17.861 | 12 |
| random | 790 | 17 | 1 | 1 | 52.299 | 36 |
| guided | 790 | 18 | 0 | 1 | 53.133 | 36 |
| fixed | 776 | 6 | 0 | 0 | 18.896 | 12 |
| random | 776 | 18 | 0 | 1 | 52.089 | 36 |
| guided | 776 | 17 | 1 | 1 | 51.860 | 36 |
| fixed | 777 | 6 | 0 | 0 | 18.104 | 12 |
| random | 777 | 17 | 1 | 1 | 54.286 | 36 |
| guided | 777 | 17 | 1 | 1 | 57.341 | 36 |

探索9反復の合計は 375.870 秒。
全反復が候補空間を使い切り、各60秒以下だった。
縮小で別候補4件を評価したが、全件で既存テストの生存を失い、採用した縮小は0件。
通常評価で有限差80件、無効観測42件を記録した（同じ候補のseed間・方式間の反復を含む）。
新たな生存候補は6回見つかったが、IDで重複を除くと1件。
生成18候補のうち4件は括弧・spanを除いた正規化ASTで既存6候補と一致し、追加の構造は14件だった。
正規化AST一致は構文上の重複検査であり、意味上の等価性の判定ではない。

同候補の最終比較はHypothesis 0.947秒、CrossHair 45.449秒（各上限60秒）。
どちらも通常CPythonの元・変異各2回で差を再現した。
生成テストは元で1件成功し、変異で1件のassertion失敗を確認した。
生成テストを含む新しいplanのPythonテストfingerprintは165件。
最後の軽量suiteは `5053 passed, 6 skipped, 53 deselected in 49.05s`、`Exit(0)`、50.576秒だった。
optional PDF依存によるskipとslow除外を含み、全機能・全依存の検証とは扱わない。

初回の全体suiteはnox不足で収集失敗、追加後はOpenBLASの確保で1GiBに達した。
最後は同じメモリ・プロセス上限のままnative threadを各1に制限して検証した。
比較表の9反復は全方式が同じ従来のthread設定で測った値であり、後のthread制限後の性能値とは混ぜない。
最終検証で導入したnoxはサンプルのuv.lockに固定された2026.8.10。

測定実行はbaseline・plan・当時の照合と失敗した全体suiteを含めて455.023秒、271起動。
修正後の最終照合・suite・清掃は別実行で123.333秒、20起動。
初期接続確認382.90秒、比較接続確認72.61秒、fingerprint globを拒否された試行65.32秒、メモリ上限に達した検証125.114秒も別途要した。
環境導入・実装・セルフレビュー・独立レビューのwall timeは未計測で、これらを含む総費用の削減は主張しない。
独立コードレビュー1名、人によるsampleテスト採用0件（未レビュー）。

[機械可読の証拠](issue-790/evidence.json)に候補ID・span・replacement・source hash・入力・観測・生成テストhash・新planのfingerprint・依存版・終了処理を保存した。
[生成テスト候補](issue-790/generated_test.py.txt)は自動実行されない資料として保存した。


## 制約と誤成功の防止

ASCII文字列を各長さ1〜32に限定し、両空文字のゼロ除算を差に数えない。
有限float以外、例外、無効入力、import error、timeout、切詰め出力は有効な差またはkillと区別する。
Hypothesisで差が未発見でも等価とは判断しない。
探索用の抽出関数をPythonのセキュリティsandboxとは扱わない。
開発者が固定した信頼するローカルコードだけを対象にする。

CrossHairのdiffbehaviorはdocstring preconditionだけでは今回の入力領域を制限しなかったため、両関数へ同じguardを挿入する。
長さだけに依存するこの関数に限り、文字内容をASCIIへ投影する。
CrossHairの出力はAST literalとして読み、任意式を評価しない。
`Exit(1)` はdiffbehaviorの差分検出による終了であり、pytestの失敗とは別に記録する。
CLIが探索を続ける時間も含むwall timeを比較し、最初の差が見つかる時間を測ったとは主張しない。

全ての実験子プロセスは既存ProcessHandlerを通す。
WindowsではJob Objectによるhard limitを使い、メモリ1GiB、プロセス16、出力1MiB、timeoutは残予算以下。
workspaceコピーは8GiB、空き容量の最低値は10GiB、jobs=1。
候補ごとに空き容量を確認し、後処理・元checkoutの照合・子孫回収・出力drainが全て成功した後に新規ファイルだけへ証拠を保存する。

## 再実行

固定SHAをcheckoutしたproofreadtoolsと、Python 3.14の隔離環境を用意する。
sampleのuv.lockに従うコア依存に加え、numpy 2.4.6、Hypothesis 6.169.0、CrossHair 0.0.111、pytest 9.1.1、nox 2026.8.10を導入する。
サンプルregistryがnumpyを無条件importするため、numpyなしではbaselineが失敗する。
heavy extraやモデルは導入しない。

```powershell
$env:HOIMIN_PROOFREADTOOLS_ROOT = '<clean pinned sample checkout>'
$env:HOIMIN_RESEARCH_PYTHON = '<isolated Python 3.14 executable>'
$env:HOIMIN_RESEARCH_OUTPUT = '<new evidence.json path>'
cargo test --locked -p hoimin-cli --test proofreadtools_search proofreadtools_experiment -- --ignored --nocapture
```

通常のCIではignored実験を起動せず、小さな契約テストだけを実行する。
fuzz依存のない環境ではPythonの研究用テストを明示skipし、fuzzを導入するQuality jobで9件を実行する。
探索ライブラリをRust製品依存へ移したり、standaloneへPythonを必須化したりしない。

## 開発側の検証と清掃

Rust契約テスト7件成功、ignored実験1件。
Python探索契約9件とworkflow lint契約8件は計17件成功した。
全ターゲット・全featureのClippy `-D warnings`、cargo fmt、Ruff check/format、tyが成功した。
既存のworkflow lint経路でactionlint・ShellCheck・zizmorも成功した（既存のzizmor除外3件は追加していない）。
独立レビューでtimeout後の停止、縮小した候補の最終照合、負の結果の保存、git前処理の資源制限を修正した。
前二者は先行失敗テストを確認してから修正した。

実験終了後のcargo cleanは5,633ファイル・3.4GiBのbuild生成物を削除した。
Cドライブ空きは清掃前185,429,725,184 bytes、清掃後188,666,273,792 bytes。
workerは各実行で削除・元checkout不変を確認済みで、再現に必要な小さい証拠だけをリポジトリへ保存する。

## 根拠

- [承認済み設計](../specs/2026-10-11-issue-790-proofreadtools-design.md)
- [実装計画](../plans/2026-10-11-issue-790-proofreadtools-plan.md)
- [各段階の5回レビュー](../../reviews/2026-10-11-issue-790-proofreadtools.md)
- [Proptest Strategy Basics](https://proptest-rs.github.io/proptest/proptest/tutorial/strategy-basics.html)
- [Hypothesis公開API](https://hypothesis.readthedocs.io/en/latest/reference/api.html)
- [CrossHair diffbehavior](https://crosshair.readthedocs.io/en/latest/diff_behavior.html)
