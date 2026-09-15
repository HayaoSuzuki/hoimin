# 追加監査: nullable型変異の適用条件

nullable追加における名前解決と型引数の再帰検査の2件を起票した。対象は `5e631efc46a6e8c0b9fcf2d7a74536e57f79a369`、macOS arm64、CPython 3.14.7、Lean 4.32.2。実装は変更していない。

| Issue | 確認した問題 |
| --- | --- |
| [#564](https://github.com/tokyogas-tech/hoimin/issues/564) | 型名が利用者定義クラスや値へ再束縛されてもnullable追加を行う。4ケースで変異後の注釈評価が例外となる。 |
| [#565](https://github.com/tokyogas-tech/hoimin/issues/565) | 複数型引数のtuple内部を検査せず、Any・TypeVar等の対象外構文を見落とす。7ケースで契約外の候補を生成する。 |

## 適用契約と最小再現

[型アノテーション設計](../../superpowers/specs/2026-07-20-type-annotation-mutation-design.md)は、既知の型構成子だけを対象とし、対象外構文を含まないTにnullableを追加すると定める。利用者定義型、Any、object、TypeVar、文字列前方参照、Annotated、Callable等は除外する。この契約に対し、名前の綴りと複数型引数の扱いにそれぞれ抜けがある。両件ともconfirmed bugとして記録する。

名前解決の最小の構造例は、`class int` を利用者定義し、`x: int` と注釈する入力である。解析器は `int | None` を生成する。メタクラスの `__or__` を例外にすると、元の注釈はそのクラスとして評価できるが、変異後はRuntimeErrorとなる。型オブジェクトの `|` がメタクラスの影響を受けることは[Python公式仕様](https://docs.python.org/3/library/stdtypes.html#types-union)にも記載される。[再現コードと原因](issue-name.md)を保存した。

再帰検査の最小例は `from typing import Any; x: dict[str, Any]`。実際は `dict[str, Any] | None` を生成するが、単一引数の `list[Any]` は抑制される。Subscriptのsliceへ再帰した後、複数引数を表すExpr::Tupleが未処理の分岐へ落ちることが原因である。両注釈ともPythonで評価できるため、この件では設計上の適用範囲の違反を指摘する。[対象7種類と原因](issue-tuple.md)を保存した。

## Leanモデルと証明

[モデル化前の対応表](correspondence.md)で、名前の参照先と注釈内容を独立した前提として定めた。[GateModel.lean](GateModel.lean)ではtrustedを名前が既知かのBool、型引数を対象内・対象外の原子、単一子、左右の子を持つ木で表す。eligibleはtrustedと、木全体が対象内であることの論理積である。

モデル内で、候補を許可したならtrustedであること、任意の深さの子孫に禁止要素を含めば候補を拒否することを証明した。後者はContainsBlockedに対する帰納法であり、有限深さの検査とは別の一般性を持つ。左右とも対象内のpairを保持する正例も確認した。Pythonの全型システム、任意の名前解決、Rust全体の証明ではない。

[GateMain.lean](GateMain.lean)は深さ0から1ずつ増やして2まで全木を列挙した。原子2種類・単一子・左右の子の4構成を固定順序で生成し、対称性削減は行っていない。表のnode数は木の構造から数えた1巡分の合計で、Rustの性能計測ではない。

| 最大深さ | 木の数 | trustedを2値にした条件数 | 合計node数 | tuple内部を省く規則との不一致（trusted=true） | 名前条件を省く規則との不一致（trusted=false） |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 2 | 4 | 2 | 0 | 1 |
| 1 | 8 | 16 | 18 | 3 | 3 |
| 2 | 74 | 148 | 380 | 58 | 13 |

tupleの固定した最小証拠はpair(good,bad)で、正しい規則はfalse、子を見ない規則はtrueとなる。名前の最小証拠はtrusted=falseのgood原子で、正しい規則はfalse、名前条件を省く規則はtrueとなる。pair(good,good)を保持する正例で、一律に複数型引数を拒否する修正も検出した。適用条件と境界の感度を調べており、原子性・永続ID・重複配送は対象外である。

証明と生成器を分離し、sorry/admit/native_decideは使っていない。各プロセスは単一スレッド、既定heartbeat上限、20秒・2GiB・50ms監視で実行した。最終のモデル検証は3,334ms / peak RSS 677,488KiB、有限検査・コーパス照合は623ms / 686,272KiB、両方exit=0。各深さの検査本体はミリ秒精度で0msだった。[model計測](verification/model.json)、[search計測](verification/search.json)、[検査ログ](verification/search.log)、[初回生成ログ](generation/search.log)を保存した。上限引上げや中断した大規模探索はない。

## 公開CLIとCPythonの照合

Leanから生成した[15入力](corpus.jsonl)を[replay.py](replay.py)で公開planへ渡した。期待値はtype_nullable_addのoriginal/replacement組。実際の候補spanが元ソースに対応することも検査し、元と変異後の注釈をannotationlibで評価した。

| 入力群 | 件数 | debug/release共通の結果 |
| --- | ---: | --- |
| int/strの数値への再代入、intの利用者定義クラス、listのmappingへの再代入 | 4 | 候補0を期待、実際1。変異後はTypeError 3件、RuntimeError 1件。 |
| dict内のAny/object/TypeVar/前方参照/Annotated/Callable、list内部のdict[str,Any] | 7 | 候補0を期待、実際1。元と候補は正常評価。 |
| list[Any] | 1 | 候補0で一致。 |
| 通常のint/list[int]/dict[str,int] | 3 | 候補1で一致。元と候補は正常評価。 |

debug/releaseとも4 match / 11 mismatch / infrastructure-error 0。15fixtureはstrict、全木の列挙と一般定理はmodel-onlyである。scopeや除外対象をモデルの入力として明示しており、解析器の出力に合わせて期待値を変更していない。全元ソースの注釈評価は成功し、対照3ケースの期待置換も評価できた。

[release結果](replay-release.json)と[debug結果](replay-debug.json)に、バイナリSHA-256、全候補、元と候補の注釈評価結果を保存した。CPythonが数値注釈を評価できることは、型チェッカーのbaseline成功を意味しない。今回は型チェッカーや公開runのスコアを測定していない。ID・順位・descriptor全体の証明も対象外である。

## 再実行と未確認事項

```sh
python3 docs/audits/2026-09-15-nullable-gates/verify_lean.py --output /tmp/nullable-gates-proof
python3 docs/audits/2026-09-15-nullable-gates/replay.py --binary target/release/hoimin --output /tmp/nullable-gates-release.json
python3 docs/audits/2026-09-15-nullable-gates/replay.py --binary target/debug/hoimin --output /tmp/nullable-gates-debug.json
cargo test --offline -p hoimin-cli --lib nullable
cargo test --offline -p hoimin-cli --test operator_function_contracts --test collection_annotation_builtins
```

LeanのRSS監視にはpsの権限が必要。再生成は `verify_lean.py --generate --output <新規ディレクトリ>` を使う。replayはjobs=1、workspace上限8GiB、空き容量10GiB超で入力を一時ディレクトリに隔離する。不一致だけでは非ゼロ終了しない監査用adapterなので、そのままCIゲートとして使用しない。

既存のnullable単体テスト2件、collection注釈5件、operator契約18件の計25件が成功した。対象の絞り込み、mapping patternの衝突防止、例外handlerの変異もコードを確認したが、今回そこへの新規Issueはない。修正時には名前解決の追跡対象追加と、tupleの全要素走査を別々に検証する。遅延注釈の後続再代入は#558、collectionの型名解決は#548/#559の範囲として今回と分ける。実装修正と正式CIへのケース移行は未実施である。
