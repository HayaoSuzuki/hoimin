# 追加監査: 値なし注釈と候補精度

値を伴わない名前の型注釈によってbuiltin・operator aliasの候補が欠落することを確認し、[#562](https://github.com/tokyogas-tech/hoimin/issues/562)に改善として起票した。対象は `5e631efc46a6e8c0b9fcf2d7a74536e57f79a369`、macOS arm64、CPython 3.14.7、Lean 4.32.2。実装は変更していない。

## 現行の契約と改善案

既存の[collection設計](../../superpowers/specs/2026-08-06-collection-and-structural-mutation-operators-design.md)と[operator設計](../../superpowers/specs/2026-09-08-python-operator-coverage.md)は、参照先が不確かな場合の保守的な候補欠落を許容する。今回の観測は候補精度の改善余地であり、安全性違反や、現行設計の完全性保証への違反とは扱わない。監査分類はspecification ambiguityで、提案する精度を採用するかがIssueの判断事項である。

改善案は、値なし注釈を実行時の再代入と区別すること。`any: int` の後でもmodule/classのanyは組込みを参照し、既存のmodule import aliasに値なし注釈を付けても参照先は変わらない。関数scopeでは名前がローカル宣言になるため、局所値がない場合はUnboundLocalErrorとなる。[Python公式仕様](https://docs.python.org/3.14/reference/simple_stmts.html#annotated-assignment-statements)。今回のfixtureでは注釈値を評価しない。

NameResolutionBuilderはAnnAssignのvalue有無にかかわらずtargetを束縛として記録する。operator用のImportScanも、注釈targetのStoreを追加の束縛として数える。型注釈用KnownImportsにはvalueがSomeの場合だけ更新する分岐が既にある。[最小再現・原因箇所・受け入れ条件](issue.md)に詳細を記録した。

## Leanモデルと証明範囲

[事前対応表](correspondence.md)に前提と観測を記した。[DeclarationModel.lean](DeclarationModel.lean)はscopeをmodule/class/function、局所値をmissing/known/otherとし、外側が既知かをBoolで表す。注釈対象名は関数ではローカル宣言済みとして解決する。RHSがあるモデルケースでは、利用者関数へ再代入する。

モデル内で、値なし注釈による局所値・解決結果の保持、任意回数の反復での値保持、値付き再代入後の候補拒否、関数ローカル未束縛名での外側へのfallback禁止を証明した。モデルはPythonの全名前解決やRust実装全体を証明していない。

[DeclarationMain.lean](DeclarationMain.lean)で有限な全組合せを列挙した。scopeをmodule、class、functionの順に1つずつ追加し、局所値3種類、fallback2種類、RHS有無2種類を固定順序で検査した。各ケースは1遷移であり、対称性削減や長いトレース探索は行っていない。

| scope数 | 状態数 | 遷移・ケース数 | 常に再代入する規則との不一致 | RHSを無視する規則との不一致 | 関数で外側へfallbackする規則との不一致 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 6 | 12 | 3 | 3 | 0 |
| 2 | 12 | 24 | 6 | 6 | 0 |
| 3 | 18 | 36 | 8 | 8 | 1 |

最小の証拠は `module / missing / fallback=true / RHSなし`。正しい遷移では局所値missingを保持し、組込みへfallbackする。常に再代入する壊した規則ではotherへ変わり、候補を失う。正例・負例を用いて、RHS有無の境界と関数ローカルの優先順位の誤りを検出した。重複宣言の値保持は任意回数の定理で確認した。並行処理の原子性、永続ID、配送の重複はこのモデルの対象外である。

証明モジュールと生成器は分離し、sorry/admit/native_decideは使用していない。各プロセスは単一スレッド、既定heartbeat上限、20秒・2GiB・50ms監視で実行した。最終のモデル検証は3,303ms / peak RSS 682,464KiB、有限検査・コーパス照合は694ms / 691,600KiB、両方exit=0。各scope段階の検査本体はミリ秒精度で0msだった。[検証ログ](verification/search.log)、[model計測](verification/model.json)、[search計測](verification/search.json)を保存した。初回生成は[generation](generation/search.log)に別途保存した。制限を上げた実行や中断した大規模探索はない。

## 公開CLIとの照合

Leanから[14入力](corpus.jsonl)と提案する候補組を生成し、[replay.py](replay.py)で公開planへ渡した。期待値は改善案のoriginal/replacement組であり、候補ID・順位・descriptor全体の証明は行っていない。各fixtureの既知とされた名前以外の条件は固定し、元ソース、実際の候補、提案する置換を別々にCPythonで実行した。

| 入力群 | 件数 | 提案契約との結果（debug/release共通） |
| --- | ---: | --- |
| module/classでsource・destinationを注釈 | 4 | 候補1を期待、実際0 |
| moduleの注釈後に関数から組込みを参照 | 1 | 候補1を期待、実際0 |
| module importとfrom-importのaliasを注釈 | 2 | 候補1を期待、実際0 |
| 利用者関数への既存束縛・値付き再代入・外側のcustom・関数ローカル | 4 | 候補0で一致 |
| 注釈なしのbuiltin・module alias・direct alias | 3 | 候補1で一致 |

debug/releaseとも7 match / 7 mismatch / infrastructure-error 0。14入力はstrict、抽象状態の全列挙と一般定理はmodel-onlyである。strictは同じ前提を公開入力で設定・観測できることを示し、改善案の採用済みを意味しない。

全元ソースは正常実行し、提案する10置換も正常実行できた。builtinはFalseからTrue、operator.addは3から1へ変わる。7件の候補欠落は意味のある変異を失う例である。[release結果](replay-release.json)と[debug結果](replay-debug.json)に、バイナリのSHA-256と全観測を記録した。公開runのスコアへの影響は今回測定していない。

## 再実行と未確認事項

```sh
python3 docs/audits/2026-09-15-declaration-only/verify_lean.py --output /tmp/declaration-proof-recheck
python3 docs/audits/2026-09-15-declaration-only/replay.py --binary target/release/hoimin --output /tmp/declaration-release.json
python3 docs/audits/2026-09-15-declaration-only/replay.py --binary target/debug/hoimin --output /tmp/declaration-debug.json
cargo test --offline -p hoimin-cli --test operator_function_contracts --test collection_annotation_builtins --test lean_annotation_scope_oracle
```

LeanのRSS監視にはpsの実行権限が必要となる。既存コーパスの再生成には `verify_lean.py --generate --output <新しいディレクトリ>` を使う。replayは一時ディレクトリで入力を隔離し、jobs=1、workspace上限8GiB、空き容量10GiB超を維持する。不一致はJSONへ記録するが、それだけでは非ゼロ終了しないため、そのままCIゲートとして使用しない。

関連テストは18+5+4の27件成功。属性・subscript targetの副作用、global/nonlocal、メタクラス、旧Pythonの注釈評価副作用は未検証である。operatorのlocal importは既存設計の対象外なので改善対象に含めていない。削除後の組込み参照、構造変異の括弧・generator条件、plan/verifyの入力照合も補助的に確認したが、今回それらの新規Issueは起票していない。
