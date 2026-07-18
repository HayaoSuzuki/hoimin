# hoimin設計書

## 目的

hoiminは、AIエージェントがPythonコードの変更箇所を短時間で検証するためのmutation testing CLIである。
初版はプロジェクト全体の網羅的な評価よりも、ファイル、行範囲、symbol、Git差分で絞った小さな反復を優先する。
Windowsネイティブで動作し、テストプロセスの時間、メモリ、出力、プロセス数を制限する。
Linuxでも同じCLIと結果スキーマを提供する。

既存ツールには、Windowsでforkを利用できないことや、大きなmutation実行がメモリを消費し続けることなど、今回の用途に合わない制約がある。
たとえばmutmut 3以降はforkを必要とするため、WindowsではWSLが必要になる。
Cosmic Rayは設定ファイルとセッションDBを中心に据えており、明示的な初期化と実行の段階を持つ。
hoiminは対象と安全制限を一つのCLI呼び出しに明示し、エージェントが実行条件と結果を機械的に扱える形にする。

## 初版の範囲

初版は次の機能を含む。

- Windows x86-64とLinux x86-64を正式にサポートする。
- Python 3.12、3.13、3.14のソース構文を対象にする。
- 明示的なファイル、行範囲、symbol指定とGit差分指定を提供する。
- テストランナーに依存しない任意のテストコマンドを実行する。
- pytestを主要な検証対象とするが、pytestのプラグインAPIには依存しない。
- 既定では直列に実行し、並列化は明示的な指定がある場合だけ行う。
- JSONとJSON Linesの安定した出力スキーマを提供する。
- SQLiteへの保存と中断再開を明示的なオプションとして提供する。
- 保守的な一次mutationだけを生成する。
- PyPIのネイティブwheelとして配布し、`uvx hoimin`と`pipx run hoimin`から起動できるようにする。

初版は次の機能を含まない。

- 対象指定のない全プロジェクト実行
- coverageに基づくテスト選択
- pytestのnodeid収集やテスト対応付け
- mutation演算子の外部プラグインAPI
- 文字列や数値リテラルの変形
- 文、関数呼び出し、例外、戻り値の削除や置換
- 複数箇所を同時に変える高次mutation
- TUI
- プロジェクト設定ファイル

## 検討した実行方式

### 制限付きサンドボックス複製

採用する方式は、プロジェクトを一時領域へ複製し、複製先だけにmutationを適用する方式である。
元の作業ツリーを変更しないため、強制終了や電源断が起きてもmutantがソースへ残らない。
既定の直列実行ではworkerコピーを一つだけ作り、並列実行ではworkerごとにコピーを一つ作る。

### 作業ツリーへのトランザクション適用

元ファイルを退避してmutantを原位置へ書き込み、テスト後に復元する方式も検討した。
この方式はコピーが不要だが、プロセスの強制終了や電源断で復元処理が走らない場合にmutantが残る。
hoiminが避けたい事故を構造上排除できないため、採用しない。

### 実行時選択コードの埋め込み

一つの変換済みソースへ複数のmutantを埋め込み、環境変数で一つを選ぶ方式も検討した。
この方式はファイル生成を減らせるが、traceback、モジュールレベルのコード、デコレータなどで元コードとの意味の差を抑える設計が難しい。
初版の安全性と実装範囲を優先し、採用しない。

## コンポーネント

hoiminはfunctional coreと、I/Oの種類ごとに分けたshell handlerで構成する。

- **core**：対象集合、run状態、Effect、Event、結果分類、集計、終了判断を扱う。
- **target handler**：filesystemとGitから対象候補を読み取り、相対パスと変更行を返す。
- **analyzer handler**：LibCSTヘルパーを起動し、mutation候補をJSON Linesのspoolへ書く。
- **workspace handler**：制限付きのプロジェクト複製、worker、mutation適用、復元、掃除を行う。
- **process handler**：baselineとmutantのコマンドをOS別のresource制御下で起動し、生の終了理由と出力spoolを返す。
- **report handler**：coreが生成した出力eventをJSONまたはJSON Linesへ逐次書き込む。
- **session handler**：SQLite transaction、互換runの検索、commit済み結果の読み出しを行う。

各handlerは判断済みのEffectを受け取り、内部表現ではなく完了Eventを返す。
この境界により、対象選択と結果分類をI/Oから独立して検証し、LibCST、OS別プロセス管理、保存形式をhandler単位で検証できる。

## Sans-I/Oによる制御と実行の分離

hoiminの制御ロジックは、Sans-I/Oの考え方を使って**functional core**と**imperative shell**に分ける。
Sans-I/Oはネットワークプロトコルのために整理された手法だが、同期的な入力から同期的な出力を返し、I/Oと非同期フロー制御を外側へ出す原則をrun制御へ適用できる。

functional coreの中心は、`RunState`と`RunEvent`を受け取り、新しい状態と実行要求を返す同期的な状態機械である。

実装はCargo workspace内の二つのcrateに分ける。
`hoimin-core`はEvent、Effect、状態機械、分類、集計、契約を実装し、Tokio、filesystem、SQLite、OS APIへ依存しない。
`hoimin-cli`は`hoimin-core`へ依存し、shellのhandlerとMaturinで配布するバイナリを実装する。
依存方向をcrate境界で固定し、coreへI/O依存を追加しなければ実装できない変更をレビュー時に識別できるようにする。

```rust
fn transition(
    state: RunState,
    event: RunEvent,
) -> Result<(RunState, Vec<RunEffect>), MachineError>;
```

状態機械はfilesystem、Git、子プロセス、時計、環境変数、SQLite、Tokioへアクセスしない。
runの状態遷移、空きworkerへの割り当て、mutation結果の分類、timeoutと終了コードの判断、resume互換性、score、target集合演算、run全体の予算予約を値の変換として処理する。

imperative shellは`RunEffect`を実行し、その結果を`RunEvent`として状態機械へ返す。
Git差分の取得、LibCSTヘルパーの起動、worker操作、OS別プロセス制御、時刻取得、JSONとJSON Linesの書き込み、SQLite transactionはshell側のhandlerが担当する。
巨大な`FileSystem` traitへI/Oをまとめず、Git、workspace、process、report、sessionごとの小さな具体的handlerに分ける。

主なeffectは対象解決、複製前検査、worker作成、baseline起動、候補解析、次候補の読み出し、mutation適用、テスト起動、worker復元、結果保存、イベント出力、cleanupである。
各effectにはrun内で一意な`EffectId`を付ける。
shellが受理したeffectは、成功、失敗、timeout、キャンセルのいずれか一つの完了eventへ到達させる。
同じ`EffectId`について二つ目の完了eventを生成しない。
プロセス自体が強制終了した場合はこの規則を適用できないため、SQLiteにcommit済みの結果だけをresumeで再利用する。

候補とプロセス出力の本体は状態機械へ渡さない。
eventは`CandidateSpoolRef`と`OutputSpoolRef`、件数、offset、hashなどの小さな値だけを保持する。
候補spoolから次のレコードを読む処理もshell側のeffectとし、functional coreのメモリ使用量をmutant数から独立させる。

並列実行の判断はfunctional coreが行う。
状態機械は`--jobs`、空きworker、実行中のeffect、run全体の予算から開始可能なeffectだけを返す。
shellは複数のeffectを非同期に実行できるが、完了順を判断に使わず、完了eventを状態機械へ戻すだけとする。

## 開発時の契約検査

hoiminは、コンポーネント境界の前提と結果を**契約**として記述する。
契約は、呼び出し前の事前条件、処理後の事後条件、状態を持つ値の不変条件に分ける。
`dbc` crateには依存せず、Cargo feature `contracts`で有効になる軽量なマクロとtraitを`crates/hoimin-core/src/contracts.rs`に実装する。

契約検査はCIと開発用テストでだけ使う。
`contracts` featureが無効なビルドでは条件式を評価せず、検査用のスナップショット、ハッシュ計算、診断文字列も生成しない。
PyPIへ配布するwheelは、このfeatureを無効にしてビルドする。

契約違反は、プログラム内部の前提が破られたことを示すため、契約検査が有効なときはpanicさせる。
panicメッセージには、`workspace.reset.post`のような安定した識別子、契約種別、条件、診断に必要な最小限の値を含める。
これにより、CIログから違反した境界を特定できる。

契約は、ユーザー入力と実行時障害の検証を置き換えない。
CLI引数の誤り、対象spanと元バイト列の不一致、元の作業ツリーの変更、リソース上限超過、worker復元失敗は、契約検査の有無にかかわらず型付き`Result`として処理する。
ユーザーは契約を意識せず、同じ終了コードと機械可読結果を受け取る。

初版では次の境界に契約を置く。

- `target`が返すパスはプロジェクトルートからの正規化済み相対パスであり、行範囲は昇順で重複しない。
- `analyzer`が受け入れた候補数は、候補spool内のJSON Linesレコード数と一致する。
- mutation適用後は指定spanだけが変わり、復元後のworkerはmanifestと一致する。
- 予約済みメモリ、コピー量、プロセス数はrun全体の上限を超えず、解放後に二重減算しない。
- runの状態遷移は定義済みの辺だけを通り、fatal errorまたはキャンセル後に新しいmutantを開始しない。
- 一つの`EffectId`に対する完了eventは一つだけであり、未知のID、重複完了、現在の状態で受理できないeventを拒否する。
- 出力イベントの`sequence`は単調増加し、一つのmutantについて開始イベントの後に完了イベントが現れる。
- SQLite transactionのcommit後は、同じrunとmutant IDで結果を読み戻せる。

通常のエラー経路を契約違反として扱わない。
たとえば候補数が`--max-candidates`へ達することは想定済みの不完全終了であり、候補数カウンターとspool件数が食い違うことが契約違反である。

## 実行の流れ

一回の実行は、状態機械とshellのあいだでeventとeffectを交換して進む。

1. CLI adapterが引数配列を`StartRequested` eventへ変換する。
2. 状態機械が引数間の制約を検証し、対象解決、Python確認、複製量の事前計算をeffectとして返す。
3. shellがeffectを実行し、対象ファイル、複製manifest、PythonとLibCSTの版を完了eventとして返す。
4. 状態機械がworker作成とbaseline実行を要求する。
5. baseline成功後、状態機械がファイル単位のLibCST解析と候補spool作成を要求する。
6. 状態機械が空きworkerとrun全体の予算に応じ、次候補の読み出し、mutation適用、テスト実行を要求する。
7. shellが完了eventを返すたびに、状態機械が結果分類、保存、出力、worker復元のeffectを返す。
8. 状態機械が次の候補を要求し、候補終了、制限到達、fatal error、deadline、キャンセルまで繰り返す。
9. 状態機械が未実行候補の分類、最終summary、cleanupを要求する。
10. shellが書き込みをflushし、子孫プロセス、worker、一時領域を掃除して完了eventを返す。

baselineが失敗した場合はmutationを一つも実行しない。
元の作業ツリーが実行中に変わった場合は、異なるソースに対する結果が同じrunへ混ざるため実行を中断する。

## CLI

基本形は次のとおりである。

```text
hoimin run [対象指定] [安全制限] [出力設定] -- <テストコマンドと引数>
```

実行例を次に示す。

```text
uvx hoimin run \
  --source src \
  --changed \
  --diff-base main \
  --file src/payment.py \
  --line src/payment.py:40-80 \
  --format jsonl \
  -- python -m pytest tests/test_payment.py -x -q
```

PowerShellでは行継続文字が異なるため、ドキュメントには一行の例も併記する。
`--`以降はシェル用の一つの文字列に戻さず、引数配列のまま子プロセスへ渡す。
この契約により、シェル展開と引用規則への依存を避ける。

### Python実行ファイル

LibCSTヘルパーを起動するPythonは`--python <PATH>`で指定できる。
指定がない場合は、実行中のhoiminバイナリと同じ仮想環境にあるPythonを使い、見つからない場合だけ`PATH`上の`python`を使う。
この順序により、PyPIパッケージの依存として同じ環境へ導入されたLibCSTを確実にimportする。
明示したPythonを含め、解決したPythonが3.12、3.13、3.14のいずれでもない場合、または指定範囲のLibCSTをimportできない場合は設定エラーにする。
テストコマンドが使うPythonは`--`以降のargvで独立して指定する。

### 対象指定

対象指定は次のオプションで構成する。

- `--root <DIR>`：すべての相対パスを解決するプロジェクトルートであり、既定値は現在のディレクトリとする。
- `--source <DIR>`：mutation可能なソースルートであり、繰り返し指定できる。
- `--file <PATH>`：ファイル全体を指定し、繰り返した指定は和集合にする。
- `--line <PATH>:<START>-<END>`：一行または閉区間の行範囲を指定する。
- `--symbol <MODULE>:<QUALNAME>`：モジュール内の関数、メソッド、クラスを指定する。
- `--changed`：Gitの差分に含まれる行を候補にする。
- `--diff-base <REV>`：指定したrevisionと`HEAD`のmerge-baseを差分の起点にする。

対象パスは`--root`からの相対パスに正規化する。
この制約により、Windowsのドライブ文字に含まれるコロンと`--line`の区切りを混同しない。
複数の明示指定は和集合にする。
明示指定と`--changed`を併用した場合は、明示範囲と差分行の積集合にする。

`--changed`は、merge-baseまたは`HEAD`から現在の作業ツリーまでの差分を対象にする。
この差分にはstaged、unstaged、未追跡のPythonファイルを含める。
Gitに無視されているファイルは含めない。
`--changed`だけを指定する場合は、テストコードを誤ってmutateしないように`--source`を必須にする。
`--diff-base`は`--changed`と併用し、単独指定は設定エラーにする。
`--source`がある場合、`--file`と`--line`のパスは少なくとも一つのsource rootに含まれなければならない。
`--symbol`は`--source`から解決するdotted module名と、構文上の入れ子を表すqualified nameを使う。

対象指定が一つもない場合は実行しない。
初版は暗黙の全プロジェクト実行と`--all`を提供しない。

### 複製対象

`--include <GLOB>`は`.gitignore`や既定除外を上書きして必要なfixtureや設定ファイルを複製対象へ戻す。
`--exclude <GLOB>`は追加の除外を指定する。
両方を指定した場合は、明示的な`--exclude`を優先する。

## LibCSTとの境界

hoiminは`libcst>=1.8.6,<2`を必須依存とする。
PyPIのhoiminパッケージはRustバイナリとLibCST依存を同時に導入する。
PyPIパッケージの`requires-python`は`>=3.12,<3.15`とする。
LibCSTはPython 3.12から3.14の構文をCSTとして解析し、コメント、空白、括弧、改行を保持する。

LibCSTの公開APIはPython APIであり、native parserは現時点で独立した安定Rust crateではない。
hoiminはnative parserをforkせず、短命なPythonヘルパーから公開APIを使う。
ヘルパーコードはRustバイナリへresourceとして埋め込み、runの一時領域へ書き出して起動する。
この方式では独自のPythonモジュールやpytestプラグインを登録しない。

ヘルパーは一回につき一つのファイルを解析する。
位置情報が必要な場合は`MetadataWrapper`へ`unsafe_skip_copy=True`を指定し、parserが生成したCSTの深いコピーを避ける。
ヘルパーはCSTそのものやmutantごとの全ソースを返さず、一行に一つのmutation記述子を返す。
Rust本体はmutation記述子をrunの一時ファイルへ逐次書き込み、実行待ち候補をすべてメモリへ保持しない。

mutation記述子は次の形を持つ。

```json
{
  "path": "src/payment.py",
  "span": {"start": 418, "length": 2},
  "original": "==",
  "replacement": "!=",
  "operator": "comparison.eq_to_ne",
  "line": 52,
  "column": 11,
  "symbol": "Payment.is_valid"
}
```

Rust本体は`span`がファイル内に収まり、対象バイト列が`original`と一致することを検証する。
検証に失敗した候補は実行せず、基盤エラーとしてrunを終了する。
局所置換以外のバイト列は変更しない。

## mutation演算子

初版は次の一次mutationを提供する。

- **比較**：`==`を`!=`へ、`!=`を`==`へ置換する。
- **順序比較**：`<`を`<=`へ、`<=`を`<`へ、`>`を`>=`へ、`>=`を`>`へ置換する。
- **所属**：`in`を`not in`へ、`not in`を`in`へ置換する。
- **同一性**：`is`を`is not`へ、`is not`を`is`へ置換する。
- **論理**：`and`を`or`へ、`or`を`and`へ置換する。
- **加減算**：二項と複合代入の`+`を`-`へ、`-`を`+`へ置換する。
- **乗除算**：`*`を`/`へ、`/`を`*`へ置換する。
- **整数演算**：`//`を`%`へ、`%`を`//`へ置換する。
- **単項符号**：単項`+`を`-`へ、単項`-`を`+`へ置換する。
- **否定**：`not x`を`x`へ置換する。
- **真偽値**：`True`を`False`へ、`False`を`True`へ置換する。
- **ループ制御**：ループ内の`break`を`continue`へ、`continue`を`break`へ置換する。

一つのmutantは一箇所だけを変更する。
連鎖比較に複数の比較演算子がある場合も、一つずつ別のmutantにする。
LibCSTヘルパーはmutation候補の置換後コードが対象Pythonで構文解析できることを検証する。
構文解析できない候補は生成段階のエラーとして報告し、テスト失敗による`killed`には数えない。

mutant IDは、スキーマ版、ファイル内容のBLAKE3ハッシュ、正規化した相対パス、開始バイト、長さ、演算子ID、置換内容から生成する。
ソースの内容が変わるとIDも変わるため、古い実行結果は再利用されない。

## サンドボックス

workspaceは`.git`、仮想環境、`__pycache__`、`.pytest_cache`、型検査やlintのキャッシュを既定で除外する。
そのほかのファイルは`.gitignore`、`--include`、`--exclude`を適用して選ぶ。
複製前にファイル数と総バイト数を計算し、上限を超えた場合は一つもコピーせず終了する。

シンボリックリンクは追跡しない。
初版ではリンクそのものもworkerへ作らず、除外したパスを診断へ記録する。
baselineがリンク先を必要として失敗した場合は、利用者が通常ファイルとして用意するか、複製対象を調整する必要がある。

workspaceは開始時に正規ファイルの相対パス、サイズ、更新時刻、BLAKE3ハッシュをmanifestへ記録する。
各mutantの終了後にworkerを走査し、変更または削除された既存ファイルを作業ツリーの開始時内容から復元する。
workerに新しく作成されたファイルは削除する。
復元元の作業ツリーが開始時ハッシュと一致しない場合は、異なるソースを混ぜないためrunを中断する。
復元に失敗したworkerは破棄し、上限を再確認してから作り直す。

テストコマンドのカレントディレクトリはworkerのプロジェクトルートにする。
親環境の`PYTHONPATH`に元のプロジェクト配下のパスがあれば、対応するworker内のパスへ置き換える。
そのうえでworkerのプロジェクトルートと各source rootを`PYTHONPATH`の先頭へ追加する。
この処理により、editable installが元の作業ツリーを指していても、通常のPython importではworker内のソースを優先する。
環境変数は親プロセスから継承するが、workerのルート、run ID、mutant IDを表すhoimin固有の変数を追加する。
hoiminはテストコマンドをシェル経由で起動しない。

このサンドボックスは作業ツリーをmutationから隔離するためのworkspaceであり、未信頼コードに対するセキュリティ境界ではない。
テストコマンドによるプロジェクト外のファイル変更、ネットワークアクセス、認証情報の利用は制限しない。

## 安全制限

既定値は次のとおりである。

```text
--jobs 1
--max-mutants 100
--max-candidates 10000
--analyzer-timeout 30s
--baseline-timeout 60s
--mutant-timeout auto
--total-timeout 5m
--max-memory 1GiB
--max-output 1MiB
--max-copy-size 1GiB
--max-processes 64
```

`--max-candidates`は対象全体から列挙する候補数に適用する。
候補が上限を超えた場合はmutationを開始せず、候補数上限による不完全なrunとして終了する。
`--analyzer-timeout`と`--max-memory`はLibCSTヘルパーにも適用する。
解析中のtimeout、OOM、候補数上限はテスト結果ではなくrunの不完全理由として記録する。
`--max-memory`と`--max-processes`は、すべてのworker、解析プロセス、テストプロセスを合わせたrun全体へ適用する。
`--max-copy-size`は全workerへ複製する正規ファイルの論理サイズを合算した値へ適用する。
`--jobs`を増やしても、これらのrun全体上限は増やさない。

`--mutant-timeout auto`は、baseline所要時間の二倍に一秒を加えた値と五秒の大きい方を使う。
baseline自体が六十秒以内に終了しないプロジェクトでは、利用者が`--baseline-timeout`と必要な全体予算を明示する。
`--total-timeout`はbaseline、解析、コピー、すべてのmutant実行を含む壁時計時間に適用する。

`--max-output`は一つの解析またはテストプロセスについて、標準出力と標準エラーを合算した保持量に適用する。
上限後もpipeを読み続けて子プロセスの停止を防ぎ、保持する内容だけを切り詰める。
結果には切り詰めの有無と観測した総バイト数を記録する。

### Windows

Windowsでは解析プロセスとテストプロセスをrunごとのJob Objectへ所属させる。
Job Objectにはjob全体のメモリ上限、active process数、`KILL_ON_JOB_CLOSE`を設定する。
timeout、OOM、中断、親プロセスの終了時はJob Objectを閉じ、所属する子孫プロセスをまとめて停止する。

### Linux

Linuxでは利用可能な場合にcgroup v2をrun単位で作り、`memory.max`と`pids.max`を設定する。
解析プロセスとテストプロセスは新しいプロセスグループでも起動し、timeoutと中断ではグループへ終了シグナルを送る。

cgroup v2を作れない環境では、`RLIMIT_AS`、`RLIMIT_CPU`、プロセスグループ監視へ縮退する。
この縮退方式は子孫プロセス全体の合計メモリをカーネル側で制限できない。
そのため既定では実行を拒否し、`--allow-best-effort-memory`が指定された場合だけ縮退方式を使う。
結果には`hard`または`best_effort`の制御レベルと、使用したOS機構を記録する。

## テスト結果の分類

元コードに対するbaselineテストが終了コード0で終わった場合だけmutationを開始する。
各mutantの結果は次のいずれかに分類する。

- **killed**：テストコマンドが終了コード0以外で終了した。
- **survived**：テストコマンドが終了コード0で終了した。
- **timeout**：mutant単体の時間上限を超えた。
- **out_of_memory**：設定したメモリ上限を超えた。
- **error**：起動、パッチ、解析、保存、サンドボックス復元などの基盤処理が失敗した。
- **not_run**：mutant数または全体時間の上限により実行されなかった。

終了コード0以外を`killed`とみなすのは、テストランナーに依存しない契約を保つためである。
テストランナー自身の設定誤りはbaselineで検出する。
mutationによってテスト基盤だけが壊れた場合とテストが対象動作を検出した場合は、任意コマンドの終了コードだけでは区別しない。

mutation scoreは次の式で計算する。

```text
killed / (killed + survived)
```

`timeout`、`out_of_memory`、`error`、`not_run`は分母に含めない。
判定可能なmutantが一つもない場合はscoreを`null`にする。

## 出力

`--format json`はrun全体を一つのJSON文書として標準出力へ書く。
`--format jsonl`は進捗と結果を一行一イベントとして標準出力へ書く。
機械可読形式では診断ログを標準エラーへ書き、標準出力へ混ぜない。
対話端末向けの`human`形式は提供できるが、その表示文面は安定APIに含めない。

`json`形式でもmutant結果はrunの一時ファイルへ逐次書き込み、完了時に集計情報と合わせて一つの文書へ組み立てる。
`jsonl`形式はイベントを直接標準出力へ書く。
どちらの形式もmutant件数に比例する結果配列をRustのheapへ保持しない。

JSONには少なくとも次の情報を含める。

- `schema_version`
- run ID
- 正規化した実行条件
- OS、Python、hoimin、LibCSTのバージョン
- 実際に適用した資源制御方式
- baseline結果
- 各mutantのID、位置、差分、状態、所要時間、資源情報
- 出力切り詰め情報
- 状態別件数とmutation score
- 完全なrunかどうか

JSON Linesは次のイベントを提供する。

```text
run_started
baseline_finished
mutant_started
mutant_finished
diagnostic
run_finished
```

並列実行では完了順にイベントを出す。
各候補には列挙順の`sequence`を付けるため、利用側は安定順へ並べ直せる。

## SQLiteセッション

`--session <PATH>`を指定した場合だけSQLiteへ保存する。
指定がなければプロジェクト内へキャッシュやDBを作らない。
DB書き込みは単一writerへ集約し、一つのmutant結果ごとにtransactionをcommitする。

`--resume`は同じDB内の最新の未完了runを探す。
再開には、ソースハッシュ、対象指定、演算子セット、テストargv、安全制限、PythonとLibCSTのバージョンが一致する必要がある。
一つでも異なる場合は結果を流用せず、同じDB内に新しいrunを作る。
完了済みのmutantだけを再利用し、`timeout`、`out_of_memory`、`error`、`not_run`は再実行の対象にする。

## 終了コード

CLIの終了コードは次のとおりである。

```text
0   runが完了し、survivedがない
1   runが完了し、survivedがある
2   CLI、設定、解析、保存、サンドボックスなどの基盤エラー
3   baseline失敗
4   timeout、OOM、候補数上限、全体時間上限により結果が不完全
130 ユーザーによる中断
```

`survived`と未判定結果が混在する場合は、不完全であることを優先して4を返す。
候補が`--max-mutants`を超えた場合は安定順の先頭だけを実行し、残りを`not_run`として4を返す。
対象範囲にmutation候補がない場合は、完全なrunとして0を返し、scoreを`null`にする。

## エラー処理

CLI引数、対象範囲、Pythonの版、LibCSTのimport、コピー量は、サンドボックス作成とbaselineより前に検証する。
baseline後の基盤エラーは、その時点までのJSON LinesとSQLite結果を保持し、runを不完全として閉じる。
JSON形式では最終文書を作れないほどの内部障害だけを標準エラーへ報告し、2を返す。

timeoutとOOMはmutation testingで起こり得る判定不能状態であり、ツールのクラッシュとして扱わない。
パッチ対象のバイト列不一致、worker復元失敗、SQLite commit失敗は結果の信頼性を損なうため、後続mutantを実行しない。

shellのhandlerは、I/O失敗の処置を決めず、`EffectFailed` eventへ変換する。
eventには`EffectId`、spawn、filesystem、Git、serialization、SQLiteなどの分類、機械可読コード、診断詳細を含める。
状態機械が現在の状態と分類から再試行、中断、`not_run`、cleanup、終了コードを決める。

テストプロセスの非ゼロ終了、timeout、OOM、プロセス数上限はhandler自体の失敗ではない。
process handlerはこれらを`ProcessFinished` eventの終了理由として返し、functional coreが`killed`、`survived`、`timeout`、`out_of_memory`へ分類する。

deadlineとCtrl+Cは、それぞれ`DeadlineReached`と`CancellationRequested` eventとして状態機械へ渡す。
fatal errorまたはキャンセル後、状態機械は新しいmutantのeffectを返さず、実行中プロセスの停止、未実行候補の分類、flush、cleanupだけを要求する。
cleanup handlerは同じeffectを再実行しても結果が変わらない冪等操作とする。

未知の`EffectId`、同じeffectに対する二回目の完了、現在の状態で受理できないeventは`MachineError`とし、基盤エラーの終了コード2へ対応づける。

## テスト戦略

### Rustの単体テスト

Rust側では次を単体テストする。

- CLI対象指定の和集合、積集合、Git差分解決
- パスの正規化とプロジェクトルート外参照の拒否
- バイト範囲パッチと復元
- mutant IDの安定性とソース変更時の無効化
- manifestによるworker復元
- JSONとJSON Linesのスキーマ
- SQLiteの逐次保存と互換runの再開
- 状態集計、mutation score、終了コードの優先順位
- `RunEvent`ごとの状態遷移と返される`RunEffect`
- 空きworker、`--jobs`、run全体の予算から開始可能なeffectだけを返すこと
- effectの順序を入れ替えた完了event、重複完了、未知IDの拒否
- deadline、キャンセル、fatal error後にmutation開始effectを返さないこと
- spool参照だけを状態へ保持し、候補と出力の本体を保持しないこと

契約無効の通常構成では`cargo test`を実行し、契約条件式に副作用を置いたテストで条件式が評価されないことを確認する。
契約有効の構成では`cargo test --features contracts`を実行する。
各契約には意図的に不正状態を作る`#[should_panic]`テストを設け、安定した契約識別子がpanicメッセージへ含まれることを確認する。
候補spool、worker復元、run全体の予算、状態遷移、イベント順序にはproperty testを追加する。
functional coreのテストはEvent列と期待するEffect列だけを使い、mock filesystem、mock process、async runtimeを使わない。

### Shell handlerのテスト

Git、workspace、process、report、sessionの各handlerは、状態判断を含まないことを前提に個別テストする。
一時ディレクトリ、実際のGit repository、短命な子プロセス、一時SQLite databaseを使い、成功とI/O失敗が対応する完了eventへ変換されることを確認する。
各handlerについて、同じ`EffectId`を維持すること、巨大な結果をspool参照として返すこと、cleanupを二回実行できることを検証する。

`hoimin-core`の依存グラフにはTokio、rusqlite、tempfile、OS API crateを含めない。
CIは`cargo tree -p hoimin-core`とfeature単位のビルドを実行し、この境界を固定する。

### LibCSTヘルパーのテスト

Python 3.12、3.13、3.14で同じfixture群を実行する。

- すべての組み込み演算子の候補列挙
- 行範囲とsymbolフィルタ
- Unicodeを含むファイルのバイト位置
- CRLFとLFの保持
- コメント、括弧、空白の保持
- 変更箇所以外がバイト単位で一致すること
- 生成したmutantが対象Pythonで構文解析できること

### OS別の結合テスト

WindowsとLinuxで次のE2Eテストを実行する。

- pytestと`unittest`の代表コマンド
- `killed`、`survived`、baseline失敗
- 無限ループのtimeout
- 大量メモリ確保のOOM
- 子と孫を含むプロセスツリーの停止
- 大量出力の切り詰め
- テストがファイルを作成、変更、削除した後のworker復元
- Ctrl+C後に作業ツリーと子プロセスが残らないこと
- SQLiteセッションからの再開
- PyPI wheelを`uvx`で起動できること

Linux CIではcgroup v2のhardモードと、明示的に許可したbest effortモードを別々に検証する。
Windows CIではJob Objectのメモリ上限、プロセス数上限、close時の一括停止を検証する。

## 完了条件

初版は次の条件をすべて満たした時点で完了とする。

- WindowsとLinuxで同じCLIと機械可読スキーマが成立する。
- 元の作業ツリーを一度も書き換えない。
- mutant数に比例してRust本体の保持メモリが増えない。
- LibCSTのCSTをファイル解析後まで保持しない。
- すべての候補に安定したIDとsequenceを付ける。
- 時間、メモリ、出力、コピー量、プロセス数の上限超過を区別して報告する。
- Python 3.12から3.14の対応構文で、変更箇所以外のソースを保持する。
- 中断後に子孫プロセスとworker内のmutationを残さない。
- JSON、JSON Lines、SQLiteの互換性を自動テストで固定する。
- CIの契約有効テストが事前条件、事後条件、不変条件の違反を検出する。
- 配布wheelでは契約条件式を評価せず、契約無効の通常テストと同じユーザー向け動作を保つ。
- `hoimin-core`がI/Oと非同期runtimeへ依存せず、公開APIへEventを渡すだけで全状態遷移をテストできる。
- 一つのeffectが一つの完了eventへ対応し、I/O失敗を含む処置を状態機械が一意に決める。

## 参考資料

- [mutmut](https://github.com/boxed/mutmut)
- [Cosmic Ray concepts](https://cosmic-ray.readthedocs.io/en/latest/concepts.html)
- [LibCST](https://github.com/Instagram/LibCST)
- [LibCST native parser](https://github.com/Instagram/LibCST/tree/main/native/libcst)
- [dbc crate](https://docs.rs/dbc/latest/dbc/)
- [Sans-I/O](https://sans-io.readthedocs.io/)
- [LibCST metadata](https://libcst.readthedocs.io/en/latest/metadata.html)
- [Maturin bin bindings](https://www.maturin.rs/bindings.html)
- [Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
- [Linux resource limits](https://man7.org/linux/man-pages/man2/getrlimit.2.html)
