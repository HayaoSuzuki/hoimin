# 境界条件・機能間の整合性・入力規模の監査

対象HEAD: `623dd808612dbc34775e16814845eec0bc52dff9`。実施日: 2026-09-11。実測環境: macOS arm64、CPython 3.14.7、同HEADのdebug/release CLI。

今回、新規に **実行で再現した不具合4件、静的に確認したWindowsの契約不一致1件、横断的な検証基盤の改善3件** を起票した。既存Issueを含め、構文・対象選択・実行と復旧・保存と比較・入力規模の検証範囲を以下の資料に整理した。

これは列挙した範囲の監査結果であり、任意のPythonプログラム、全OS、全非同期実行順序についてバグがないという保証ではない。未検証の部分も追跡可能にすることを完了条件とした。productionコード・既存テストは変更していない。

## 結論

既存テストには、境界値、プロパティ、実プロセス、障害注入、Leanからの対応検証が相当数ある。「単体テストしかない」「例外系を全く確認していない」という評価は当たらない。

不足が集中しているのは、次の3点。

1. **同じ規則の適用範囲が検証ごとに異なる。** 型注釈・operator importで考慮したgeneric scopeがbuiltin解決へ伝わらず、producerの意味検証がprogress readerへ伝わらない。resource modeもbackendからheader・再利用結果へ正しく伝わらない。
2. **検証の照合先が足りない場合がある。** ASTを再parseできてもPythonをcompileできるとは限らない。shared discoveryとの一致だけでは、両方が同じ誤った候補を返すことを検出できない。
3. **測っている入力次元が限られる。** 候補保持数や作成後のheapが適切でも、1行長、AST深さ、import状態幅、束縛履歴、selector数による再走査や一時メモリの増大は別に検証する必要がある。

## 新規Issue

| Issue | 確認内容 | 証拠レベル |
| --- | --- | --- |
| [#484 metrics出力が元ソースを上書き](https://github.com/tokyogas-tech/hoimin/issues/484) | selected sourceと同じ出力先でexit 0・complete=trueの後、元PythonがJSONになる | release実CLI再現 |
| [#485 mapping patternの重複キー変異](https://github.com/tokyogas-tech/hoimin/issues/485) | boolean/complexのキー編集でcompile失敗。importだけのテストで2 killed・score 1.0 | debug/release CLI、CPython compile、実run |
| [#486 generic型パラメータとbuiltinの誤認](https://github.com/tokyogas-tech/hoimin/issues/486) | 変異先tupleがTypeVarでもlist→tupleを生成 | debug/release CLI、Python実行 |
| [#487 resource modeの誤報告](https://github.com/tokyogas-tech/hoimin/issues/487) | best_effort実行のheaderとresume結果がhardになる | 自然なincomplete session→resumeを独立に再現 |
| [#488 Windowsの制限範囲の不一致](https://github.com/tokyogas-tech/hoimin/issues/488) | run-wideの説明に対して各rootへ個別制限。テストのconstructor制限値も無視される | 実装・fixture・公開説明の照合。Windows実機未検証 |
| [#489 有効Pythonを使う横断コーパス](https://github.com/tokyogas-tech/hoimin/issues/489) | syntax/scope/spanを各producerとCPythonで照合する共通基盤 | 検証改善 |
| [#490 コマンド・保存形式間の契約表](https://github.com/tokyogas-tech/hoimin/issues/490) | run/plan/verify/resume/reportで同じ前提を確認する共通fixture | 検証改善 |
| [#491 入力形状別の性能回帰ゲート](https://github.com/tokyogas-tech/hoimin/issues/491) | 操作数・peak memory・I/Oと規模増加を継続測定 | 検証改善 |

各Issueに再現条件、既存テストとの関係、既存Issueとの差、受け入れ条件を記載した。最低限の再現手順はGitHub側に保存している。

## 監査資料と判定の読み方

| 資料 | 内容 |
| --- | --- |
| [解析器の監査表](analyzer.md) | 構文位置×演算子、scope×binding×consumer、UTF-8/span、候補保持と入力形状 |
| [対象選択・plan/verifyの監査表](selection.md) | selector合成、境界値、変更検知、rank、72条件の独立照合 |
| [実行基盤の監査表](runtime.md) | 各制限の境界、parallel/cancel/cleanup、モデルと実OSの対応、CI実行条件 |
| [レポート・sessionの監査表](report-session.md) | 終了判定、永続化、再利用、v2/v3互換、readerの不整合検出、入力サイズ |
| [検証結果の要約JSON](verification-results.json) | 実行コマンド、結果、対象範囲、未検証条件 |
| [Leanの活用方針と再確認](lean-application.md) | 既存証明とreaderの間の前提、Issue別のモデル・adapter計画、資源上限付きの再検証 |

各表では以下を区別する。

- **実行確認**: 今回のfixtureで実際に観測した結果。異なる入力やOSまで一般化しない。
- **既存テストあり**: 指定したassertionをコードで確認した。今回そのsuiteを実行したかは実行証拠欄で別記する。
- **既知不具合**: 既存open Issueへ対応付け、新規に重複起票しない。
- **検証不足**: 指定した組み合わせを確認するassertionを調査範囲で見つけられなかった。これだけで製品の不具合と断定しない。
- **静的な契約不一致**: 実装と公開契約が異なる。native動作を実測したとは扱わない。
- **未検証**: 利用可能な環境や実験範囲の外。成功数に加えない。

## 今回実行した確認

主担当の既存9 suiteは **229 passed、0 failed、1 ignored**。

```sh
cargo test --offline -p hoimin-core \
  --test target_policy --test plan_config --test candidate_policy \
  --test line_selection_index --test operator_selection
cargo test --offline -p hoimin-cli \
  --test cli_config --test target_handler --test plan --test fingerprint_inputs
```

前者70件、後者159件。ignoredはplanの子プロセス用fixtureであり、通常の成功数へ加算していない。これはworkspace全suiteを今回実行したという意味ではない。

補助調査でも、report policy 37件、progress入力20件、runtime関連78件が成功した。runtimeには主担当と重複するtarget/config 44件が含まれるため、単純に足してユニークなテスト数とはしない。詳細は各監査資料を参照。

追加実験:

- **72 plan条件**: changed有無×lineの6形状×symbol有無×max_candidates 1/2/4。独立した位置表との一致、truncated、exitを確認。
- **24非truncated run**: 同じ入力のplanとcandidate ID集合が一致。
- **26 verify**: top1、選択ID、complete、exitが一致。truncated planの2条件を含む。
- **普通のtruncated runの2条件**: mutantを実行せずexit 4。planの保持候補と一致しないが、既存state machineと回帰テストで意図された差と確認した。不具合扱いしていない。
- **21安全なCLI設定境界**: jobs、process count、bytes、total timeoutの0/1/上限近傍。受理後の巨大負荷は実行しない構成。
- **10 Python構文fixture**: 生成候補の元bytesを照合し、CPythonでcompile。選んだ6 fixtureでは元コードと候補を実行。新規2不具合は主担当でもrelease CLIで再確認。
- **16 progress入力変種**: v2/v3のoptional fieldと不整合。既存 #460/#483へ集約。
- metrics/source衝突、fresh/incomplete/resumeのresource modeを実CLIで再現。

今回、性能の大規模再測定は行っていない。#491の根拠となる数値再現は、同HEADに対する既存Issueの計測結果を参照する。

## 既存Issueとの対応

| 分類 | 今回の監査と接続した既存open Issue |
| --- | --- |
| 構文・span・scope | #451 #455 #468 #469 #481 |
| selector・copy/importの整合性 | #452 #472 #473 #476 #477 |
| 保持上限と実行/保存 | #459 |
| readerの意味検証 | #460 #483。output_state/diagnosticsも#460の範囲に集約 |
| 入力規模・処理量 | #453 #456 #457 #461 #463 #470 #474 #475 #478 #479 #482 |
| 機能・診断不足 | #454 #458 #464 #467 #471 #480 |
| native実行検証 | #157 #162 #223 #228 #229 |

closed Issueにも有効な回帰テストが多数ある。例: #55 counts検証、#114 status/termination、#269 scope、#309候補保持、#330/#331/#332/#335 shutdown、#340 sibling帰属、#342 resource cleanup、#432 history memory、#441/#445括弧・型注釈。過去の不具合説明を読んだだけで再起票していない。

## 未検証・判断保留

1. **Windows Job ObjectとLinux delegated cgroupの実測**: 今回のmacOS環境では未実施。設定上、通常PRのCIはUbuntu中心、Windows/macOSは手動lane、delegated Linuxは条件付きmain push。実行履歴を取得した監査ではない。
2. **実OOM、実ENOSPC、最大256worker負荷、PID再利用**: 実行していない。既存native Issueと#488/#490に接続する。
3. **verify準備中のdeadline/cancel**: manifest decode、fingerprint、copy manifest準備は通常run shellの前にあり、rediscovery timeoutの既存試験だけでは全体を証明しない。まず適用範囲を明記して#490で検証する。
4. **dynamic class namespace**: `__prepare__`から供給されたbuiltin名への候補を観測。operator importとは保守性の扱いが異なるが、動的副作用をどこまで推定する契約か不明なため独立した確定バグには数えていない。
5. **annotation-onlyの精度**: moduleの `list: object` ではbuiltinが再束縛されないのに候補が抑制される。保守的な解決精度の改善余地として記録し、危険な候補を出す問題と同列には数えない。
6. **全構文・全組み合わせ・任意入力サイズ**: 有限のfixtureで完全性は主張しない。#489/#490/#491で明示的な検証表を継続し、新機能の未登録・未実施が分かるようにする。

## 対応の優先順

最初に、元データを変える#484、resource契約の#487/#488、スコアへ不正な候補が入る#485/#486を修正する。既存 #460/#483 も結果を信用するための優先項目。

各最小回帰テストを追加したうえで、#489/#490に同じfixtureを接続する。既存の個別性能修正を#491へ取り込み、修正後に処理量・メモリ・I/Oの増加傾向が保たれることを確認する。

この順序で、発見した不具合と、その種類の不具合が別の経路で再発する検証不足を一緒に追跡できる。
