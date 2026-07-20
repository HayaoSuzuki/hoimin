# Focused mutation profile

## 目的

`hoimin run` に、実行時間と survivor の認知負荷を抑える opt-in の
mutation profile を追加する。

Google の mutation testing の事例では、変更済み・カバレッジ済み・開発者に
とって有用な箇所だけを対象にし、低価値な *arid* node を抑制することで、
mutation testing をレビューの流れに組み込んでいる。hoimin はすでに
`--changed` と候補・実行上限を持つ。初版では外部カバレッジ形式や CI 固有の
連携を導入せず、Python AST から安全に識別できる arid な候補だけを除外する。

参照: [Petrović and Ivanković, *State of Mutation Testing at Google* (ICSE-SEIP 2018)](https://storage.googleapis.com/gweb-research2023-media/pubtools/4203.pdf)

## 対象外

- coverage.py、pytest-cov、または他の外部カバレッジデータの入力と、対象テストの選択
- 各行から一つだけを選ぶ確率的・決定的サンプリング
- GitHub、GitLab などのレビューコメント・annotation 連携
- 利用者による「有用」「不要」フィードバックの保存、学習、ルール自動生成
- 任意の logger、デコレータ、キャッシュ、ユーザー定義関数を識別する名前解決
- type annotation mutation の抑制
- `full` profile の候補、ID、並び順、mutant status、終了コードの変更

後続の機能はこの profile の利用状況を確認してから個別に設計する。特に
coverage ベースの対象選択は、入力の鮮度・ファイル対応・失敗時の動作を定義する
新しいデータ契約になるため、この変更には含めない。

## CLI と設定

`run` サブコマンドに次を追加する。

```console
hoimin run --profile focused --root . --source src -- python -m pytest -q
```

- `--profile full|focused` は `run` だけの option とする。
- 既定値は `full` とする。省略時の候補集合と終了状態は既存 release と同じである。
  正規化 config と human の開始行には profile 表示だけを追加する。
- `focused` は legacy runtime operator の候補生成時だけに arid 抑制を適用する。
- `--operators type_nullable,...` など、`type_` で始まる operator の候補は
  `focused` でも除外しない。型 checker が評価する注釈は runtime の arid 判断と
  同じ意味ではないためである。
- 正規化済み `RunConfig` に `profile` を常に含める。したがって JSON と JSONL の
  `run_started.normalized_config.profile` は `full` または `focused` になる。
- human 出力の `run started` 行にも profile を表示する。設定の違いを terminal 上で
  見失わないためであり、終了コードや mutant status は変更しない。

`profile` は `MutationProfile` enum とし、Serde では snake_case の `"full"` と
`"focused"` にする。Raw config と正規化 config の双方で既定値を `Full` にする。

## focused の arid ルール

解析器は構文解析成功後に AST を一巡し、抑制対象の半開バイト範囲 `[start, end)` を
収集する。範囲は重複・入れ子を統合して昇順に保存する。legacy candidate の mutation
span 全体がいずれかの範囲に含まれる場合、その candidate を生成しない。

初版のルールは次の四つに固定する。

1. `if __name__ == "__main__":`（文字列を左辺に置いた同値比較も含む）の条件と
   `body` を抑制する。`else` と `elif` は抑制しない。比較演算子が `==` でなく、
   比較対象が一つでない場合も抑制しない。
2. 組み込み名として構文上 `print(...)` と書かれた call expression 全体を抑制する。
   属性 call（`logger.print` など）や別名 import は対象外にする。`print` が
   ユーザー定義名を shadow していても、focused は opt-in の低ノイズ選択であるため
   初版では構文上の名前だけで判定する。
3. `assert` statement 全体を抑制する。対象ソースにテストコードが含まれる場合でも、
   assertion の内部を mutation target にして production contract を測る価値は低い。
4. 関数・async 関数の positional default と keyword-only default の各式だけを
   抑制する。関数本体、引数注釈、戻り値注釈は対象外である。

この規則は論文の Python 向け `__main__` guard、`print`、`assert`、default argument
values の抑制を、hoimin の parser で再現可能な狭い構文へ限定したものである。完全な
等価 mutant 判定を目指すものではない。focused が有用な mutant も除外し得ることを
README に明記する。

選択順は以下とする。

1. 既存の file/line/symbol 選択と operator 選択で候補の適格性を決める。
2. `focused` で legacy candidate が arid range 内なら除外する。
3. 残った候補を既存どおり span、operator で安定ソートし、重複を除去する。
4. 最後に `--max-candidates` を適用する。

この順序により、抑制された候補が candidate 上限を消費しない。候補が除外されるため
focused と full では sequence と `--max-mutants` で実行される集合が異なり得る。これは
意図した動作である。

## 実装境界とデータフロー

`crates/hoimin-core/src/config.rs` が profile 型、raw/normalized config、既定値を所有する。
`crates/hoimin-cli/src/cli.rs` は Clap の `ValueEnum` と CLI 値から raw config への変換だけを
担当する。

`crates/hoimin-cli/src/analyzer/rust.rs` は `AstFacts` に arid range の収集を追加する。
通常の token mutation 候補と type annotation 候補の両方を一度生成した後、profile と
operator 種別を見て legacy candidate だけを filter する。既存の line/symbol 選択、
candidate descriptor、stable mutant ID の構築は変更しない。

`crates/hoimin-cli/src/analyzer/mod.rs` と `crates/hoimin-cli/src/shell.rs` は、正規化済み
profile を `AnalyzeRequest` へ渡す。state machine、candidate store、workspace mutation
には新しい分岐を持たせない。

profile は candidate 集合を変える session 互換性条件である。`FingerprintInput` に
`profile: MutationProfile` を追加し、fingerprint encoding に新しい field を追加する。
`FINGERPRINT_SCHEMA` を 3 に上げ、session の `fingerprints.schema_version` にも 3 を保存する。
この release より前の incomplete session は、full であっても再利用せず、新しい run として
開始する。誤った mutant result の再利用を避けることを優先する。

公開 run-result/event schema は version 2 のままとする。`normalized_config` は schema 上
追加プロパティを許可しており、profile はその既存の拡張点に入る。summary counts に
suppressed 数を混ぜない。suppressed candidate は mutant ではないためである。

## エラー処理と互換性

- Clap が `full` と `focused` 以外を reject し、既存と同じ CLI error（exit 2）にする。
- Python 構文が不正なら、現在どおり analyzer の invalid-syntax 診断にする。arid 判定は
  その後に実行しない。
- AST 範囲が取得できない構文は suppression しない。解析器を失敗させたり候補を推測で
  除外したりしない。
- `full` では arid range を生成しても candidate filter に使わない。既存同一入力との
  candidate list 比較テストで後方互換性を守る。
- `--resume` は profile が異なる run、または fingerprint schema 2 の run を再利用しない。
- report schema、candidate ID schema、SQLite table schema の migration は不要である。

## 検証

core config tests で `Full` の既定値、`Focused` の serde 表現、fingerprint が profile の変更で
変わることを確認する。CLI tests で `--profile focused` の受理、既定値、無効値の拒否を
確認する。

analyzer tests は同一 source を full/focused で解析し、次を確認する。

- `__main__` guard の condition/body は除外され、else の候補は残る。
- 文字列左辺の guard も除外される一方、`!=`、複数比較、別文字列は残る。
- bare `print(...)` と `assert` の内部だけが除外され、属性 call と通常の expression は残る。
- positional/keyword-only defaults の候補は除外され、関数本体と注釈候補は残る。
- `type_` operator は focused でも残る。
- line/symbol selector と combined のとき、profile filter は上限適用前に働く。
- full の candidate descriptor 列が現在の fixture と一致し、focused の descriptor 列は
  span 順・operator 順で安定している。

session handler/integration tests で full と focused が異なる fingerprint になり、同一 profile
だけが incomplete session を resume することを確認する。JSON、JSONL、human の E2E tests
で profile 表示と `normalized_config.profile` を確認し、mutant status・exit code の既存契約を
確認する。
