# #759 段階的検証の実現性調査

2026-10-11。#711実装上の既存plan/verifyをWindows、Python 3.14で実行した。
製品コードや外部manifest、command DSLは追加していない。

## 判断

既存の二つのplanと独立したverifyで、screenのsurvivorだけconfirmする運用は可能だった。
同じ7候補でscreenは1 kill/6 survive、confirmは4 kill/2 survive、全体検証は5 kill/2 surviveとなり、最終survivorのID集合は一致した。
screenが見逃す変異をconfirmで検出でき、screenがkillした1候補のconfirmを省略できた。
ただし総時間はscreen 2.409秒+confirm 3.110秒=5.519秒で、単独全体検証3.919秒より約41%長い。
両baseline、各verifyのworker作成・cleanup、起動・レポート出力まで含む親processのwall timeである。
plan生成も含めると二段階5.706秒、単独全体4.007秒だった。
一回の管理fixture測定であり、実OSSの速度・費用低減を示す結果ではない。

実際の遅いプロジェクトでscreenのkill率とconfirmの費用が大きい需要は、今回まだ提示されていない。
したがって現時点では新しいproduct APIを作らず、この調査と既存機能の運用手順を残す。
#759の製品機能としての完了条件すべてを達成したとは扱わない。

## Fixtureと実行条件

元コードは次の通り。

```python
IGNORED = 10
def double(x) -> int:
    return x * 2
```

screenは `double(0) == 0` だけを検査し、worker内に `.screen_marker` を作る。
confirmはmarkerが存在しないこと、`double(0) == 0` と `double(2) == 4` を検査する。
confirmには人工的に0.15秒のsleepを加えた。
これは実プロジェクトの遅さを再現したモデルではなく、起動・baseline費用との関係を見る小規模fixtureである。
どちらも元コードのbaselineは成功した。

operatorは `integer_literal_neighbor,binary_mul_div,function_body_return_constant`、対象はsample.pyだけ。
helperのchecks.pyをfingerprintへ含めた。
jobs=1、max-mutants=20、top=8（実在7候補）、baseline/mutant timeout=3秒、total=30秒、memory=256 MiB、copy=16 MiB、workspace=64 MiB、output=16 KiB。
scratch rootがignore対象なのでsample.pyとchecks.pyを明示includeした。
空き容量のfixture閾値1 Bは調査用であり、通常運用では既存defaultを維持する。
各外部hoimin processにも45秒の親側timeoutを設定した。

候補IDだけでなく、sequence/rank/score/ranking_reasonsを除く全候補fieldが両planで一致することをassertした。
confirm baselineと全候補でmarker不在が成立し、screen生成物のstage間持越しはなかった。
各verifyは別processで、既存executorがfresh workerを作る。
外部DBやサービスの初期化までは検証していない。

## 失敗と中断

baselineをexit7にするconfirm用commandは終了3になり、confirm済みsurvivorを一つも返さなかった。
変異時に2秒sleepするcommandに800 ms mutant timeoutを設定すると、元コードbaseline成功、2 survive/5 timeout、complete=false、終了4だった。
survivedというラベル以外に、実termination、baseline、completeの条件を確認する必要がある。
最初の200 ms設定はPython起動時間にも足りず7件すべてtimeoutだったため、起動費用を超える800 msへ調整した。
この失敗を検出力の成功として数えていない。

confirmのtotalを1秒にすると1 kill/6 not_run、complete=false、終了4になった（wall time約1.152秒）。
部分結果を全候補確認済みと扱わないことを確認した。
Ctrl-Cの入力操作やOS crashを再現した試験ではなく、実行予算による中断の確認である。
今回の保守的な運用例は、run全体がcompleteでなければconfirmへの自動引継ぎを止める。

## 既存機能での運用手順

1. 同じcheckout・source・selector・operatorでscreen用とconfirm用planを作る。test argvだけをそれぞれ利用者管理の検証commandへ変える。verifyは保存commandをoverrideできない。
2. 両planがtruncatedでないこと、IDと完全descriptor、source/fingerprint対象の一致を確認する。任意の古い別runのIDを混ぜない。
3. screenを `hoimin verify screen-plan.json --top N` で実行する。exit1はsurvivorありでも返るため、`screen && confirm` のshell連結で判定しない。
4. v4 reportのbaseline terminationがExit0、summary.complete=trueを確認する。その上でstatus=survivedかつtermination=Exit0のIDだけを選ぶ。timeout/error/not_run/inconclusiveは対象外とする。
5. 対象が空ならconfirmする候補はない。対象があれば `hoimin verify confirm-plan.json --candidate ID1 --candidate ID2` を別processで実行する。このverifyも元コードbaselineを検査する。
6. confirmにも同じ完了・baseline・termination条件を適用する。未確認候補をconfirm済みsurvivorへ変換しない。
7. 両planと両reportを別々に保存し、stageのargv、run ID、選択ID、時間、試行回数を追跡する。各reportのscoreを平均したり、異なるcommandの試行を反復評価の同一試行として混ぜたりしない。

screenがconfirmと同じ検証規約の有効な部分集合であるかは利用者の責任である。
順序依存、flaky test、外部状態がある場合は全体単独実行との一致を保証しない。
設定変更時はplanを作り直し、古いsessionを無条件resumeしない。
この手順は利用者による管理下artifactの操作例であり、自動集約やstage metadata保存を製品へ実装したものではない。

## セルフレビュー

### 設計（5回）

1. 現行トリアージと照合し、具体的需要なしでmanifest/DSLを導入しない方針を維持した。
2. screen通過を全体surviveと混同せず、confirmの完了条件を独立させた。
3. IDのみでなく完全descriptorとsourceを照合する境界を確認した。
4. fresh processとworkspace、利用者管理の外部状態の責任を分けた。
5. 反復結果・stage scoreを混ぜない記録方法と、機能未実装の範囲を確認した。

### 調査計画（5回）

1. screen kill、見逃し、true survivorを少数候補に含めるfixtureを確認した。
2. 全体controlを同じ7候補で一度だけ実行する計画にした。
3. markerでstage間の生成物持越しを検出するassertを置いた。
4. baseline失敗、timeout、予算中断を独立に観測する計画にした。
5. baseline/reset込みwall timeとplan生成を記録し、大規模OSSの不要な比較を行わないと決めた。

### 実験実装（5回）

1. 既存CLIだけを使い、product parserや任意command取込み口を追加していないことを確認した。
2. 二つのplanの候補dictを照合し、IDの順番依存やrank差を評価同一性と混ぜていないことを確認した。
3. JSON完了・baseline・実terminationを検査してからsurvivorを抽出するassertを読み直した。
4. process単位45秒と既存資源上限を確認し、200 msの起動timeoutを実験失敗として修正した。
5. markerを元fixtureに置かずscreen workerだけで生成し、confirmが存在しないことを検査する実装を確認した。

### 検証（5回）

1. 1+4 kill/2 surviveが全体5 kill/2 surviveとID集合でも一致することを確認した。
2. timeoutを含むcomplete=false reportのpartial surviveを引継ぎ対象にしないことを確認した。
3. baseline失敗と予算中断をconfirmed survivorに変えていないことを確認した。Ctrl-C未検証も明記した。
4. 総時間5.519対3.919秒を再計算し、人工sleepの結果を実運用高速化と解釈していないことを確認した。
5. 製品機能の未達条件、自動化していない記録・集約、再検討条件を文書とIssueに照合した。

## 再検討条件

実際の遅い利用projectで、screenのkill率・confirm費用・baseline/reset費用を取得できたときに再評価する。
費用低減と運用上の需要が確認できた場合に、stage別結果・再開指紋・試行回数を製品が保持する設計を別途レビューする。
今回はproduct codeが変わらないため追加の形式証明は行っていない。
