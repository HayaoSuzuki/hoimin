# 対象選択・plan/verify・入力所有権の監査

HEAD `623dd808612dbc34775e16814845eec0bc52dff9`。macOS arm64、release CLI。productionコードは変更していない。

## 検証表

| 境界・組み合わせ | 既存の確認箇所 | 今回の判定・残る範囲 |
| --- | --- | --- |
| source/fileの和集合とline/symbolの絞り込み | core `target_policy.rs:295,391,425,470`、CLI `target_handler.rs:172` | 既存の明示仕様あり。追加の72 plan条件は独立期待位置と一致 |
| lineの0、逆転、隣接、重複、未該当 | core `target_policy.rs:224,252,586`、`line_selection_index.rs` | malformed拒否とrange統合を検証。今回line2/4/2+4/999、別fileをselector合成と交差 |
| changed×line×symbol、staged/unstaged/untracked、空集合 | core `target_policy.rs:639`、CLI `target_handler.rs:202,324,360,624`、`lean_changed_target_oracle.rs` | 72条件の半数でchangedを交差。単体のGit解釈だけではcandidate側scopeを保証しないため実planへ接続 |
| Git rename、binary、quoted path、hostile diff config | CLI `target_handler.rs:649,683,699,725,752,769,789` | 既存試験あり。対象handler suite 36件成功。任意のGit履歴全てを生成したわけではない |
| include/exclude、ignored、symlink、コピー対象との一致 | CLI `target_handler.rs:135,155,432,459`、workspace tests | pathの復元/除外は検証あり。default copy exclusionとの#452、session自己生成との#472は未解決 |
| pathのabsolute/relative、parent escape、Unicode、case | core `target_policy.rs:554,811–877`、CLI `target_handler.rs:102,118,280` | core/CLI通常経路は今回成功。Windows限定caseのnative動作は未実施 |
| selector数、file数、range数の増加 | `resolve_explicit`、`require_python`、`normalize_ranges` | 機能テストから計算量は保証できない。#453/#474/#475、共通ゲート#491 |
| symbol存在、クラス配下scope、rank加点 | analyzer `selected`、`plan/ranking.rs` | #476存在診断、#473子symbol rank。今回のone/two fixtureだけでクラスrankを解決済みとしない |
| operator/profileの正規化と空集合 | core `operator_selection.rs`、CLI `cli_config.rs:818,837` | 関連suite成功。今回独立表はboolean_literal/fullへ固定。他familyは解析器表と#489へ接続 |
| 候補上限の未満/一致/超過、複数file | CLI `plan.rs:384,1069,1088`、`lean_bounded_candidate_discovery_oracle.rs` | 72条件の上限1/2/4でretained/truncated/exit全一致。保持数と一時メモリは別問題 |
| 普通のrunとverifyのtruncation差 | core `machine.rs:590`、core test `candidate_overflow_stops_before_any_mutant_execution:3263` | 普通runは0実行、部分planのverifyは選択候補を実行する。今回2条件で確認し、誤って不一致バグに数えない |
| plan→verifyのcandidate ID/rank/top選択 | CLI `plan.rs:915,1007,1183,1215,1233,1302,1370,1422` | ID/descriptor改変、sequence permutation、diverse tiers等の既存試験。今回top1の26条件は一致 |
| changed source/fingerprintとbaseline前拒否 | CLI `plan.rs:576,595,783,799,852,891` | 既存suite成功。元bytesの変更だけでなく設定・fingerprintの再解決も確認している |
| raw CLI設定と保存済みnormalized設定の検証 | core `plan_config.rs:131,233,258,305`、CLI `plan.rs:718,753` | 上限/MAX+1ns、cross-field不整合、pre-dispatchの優先順位が検証されている |
| planの候補record長とspool | CLI plan出力と候補spoolの別制約 | #459。planが保存できるサイズと実行spoolに載るサイズの同一性が未保証 |
| 入力全量読み取り、verify再hash | `plan.rs:317,491,693` | whole manifest read/Value decode、source/fingerprint/copy準備、候補ごとの検証が別段階。#456および#490/#491 |
| import rootと選択fileの一致 | `workspace/mod.rs:585` | #477 src-layout/editable環境。候補IDの一致だけでは、実際にworkerの対象をimportしたか保証しない |
| metrics出力先×selected source×正常終了 | `metrics.rs:242`、`shell.rs:4497`、CLI `cli_config.rs:242` | **#484新規再現**。パス解決と保存の既存試験を通っても、保護対象の元ソースを上書きする |

## 独立期待位置を使った72条件

一時Git projectに`src/a.py`と`src/b.py`を置いた。両方ともone/two関数にboolean literalがあり、候補位置は各fileの2行目と4行目。commit後、aの2行目とbの4行目だけを変更した。

```python
# src/a.py（現在）
def one():
    return True
def two():
    return False
```

bはone=False、two=True。元commitは両方False。テストargvは絶対パスのPythonによる `import src.a, src.b` であり、正常な変異はsurvivedになる。

組み合わせ:

- changed: なし / あり
- line: なし / a:2 / a:4 / a:2とa:4 / a:999 / b:4
- symbol: なし / a:one
- max_candidates: 1 / 2 / 4

独立のset操作で、まず4位置の集合を用意し、指定されたfileのline/symbolで絞り、changedなら変更2位置と交差した。path/line順の先頭Kと、全期待件数>KのtruncatedをCLI結果と比較した。productionのresolve/discovery helperを期待値の作成に使っていない。

結果はplan 72/72一致。候補上限4の24条件はrunのID集合も一致。24条件に上限1のtruncated 2条件を加えたverify 26/26でtop1のID・complete・exitが一致。

普通のrunを上限1で実行した2条件ではmutantが0件になった。これは意図したabort-on-overflowであり、既存のcore回帰テストで裏付けられた。共通契約テストはこの差を明示しなければ誤警報になる。

この実験はsource/line/symbol/changed/候補上限の有限の組み合わせを検証した。include/exclude、full/focused、クラス、import layout、OS path差、各演算子を全て交差した証拠ではない。継続的な表は[#490](https://github.com/tokyogas-tech/hoimin/issues/490)、演算子側は[#489](https://github.com/tokyogas-tech/hoimin/issues/489)で追跡する。

監査時の一時スクリプト: `/tmp/hoimin-selection-contract-probe.py`。一時結果: `/tmp/hoimin-selection-contract-results.json`。永続的な要約はこのディレクトリの`verification-results.json`、最小再現と改善条件はリンク先Issueに記録した。
