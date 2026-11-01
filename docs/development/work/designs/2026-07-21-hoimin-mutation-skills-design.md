# hoimin ミューテーションテストスキル

## 目的

Python の機能開発中に、変更した実装とテストを `hoimin` で検査し、
survived mutant が示す振る舞い上の不足をテストへ反映できるようにする。
Claude Code と Codex CLI のどちらからも、リポジトリを開くだけで利用できる
project skill として同梱する。

通常の変更単位で実施する mutation test と、mutation test・テスト改善を反復して
改善が飽和するまで続ける作業は、起動条件と停止条件が異なる。そのため単一の大きな
スキルにはせず、二つのスキルとして提供する。

## 対象外

- `hoimin` のCLI、mutation operator、progress 判定、出力schemaの変更
- テストを自動生成して無条件に書き換えること
- production code をsurvivorを消す目的だけで変更すること
- CI、GitHub Actions、Claude/Codex plugin の導入
- mutation report を既定でリポジトリへ永続保存すること
- `saturated` を完全なテスト網羅や同値mutantの証明として扱うこと

## 配置と互換性

各スキルは、対応するCLIがproject scopeで検出するディレクトリに同一内容で置く。
シンボリックリンクを使わず、Windowsを含む通常のGit checkoutでそのまま利用できる
ようにする。

| 用途 | Codex CLI | Claude Code |
| --- | --- | --- |
| 変更単位のmutation test | `.agents/skills/hoimin-mutation-testing/SKILL.md` | `.claude/skills/hoimin-mutation-testing/SKILL.md` |
| 改善反復 | `.agents/skills/hoimin-mutation-improvement/SKILL.md` | `.claude/skills/hoimin-mutation-improvement/SKILL.md` |

同じ名前の二つの配置先にあるファイルはバイト単位で同一にする。frontmatterは両CLIで
共通の `name` と `description` だけに限定し、製品固有の `allowed-tools`、動的文脈挿入、
plugin metadata は含めない。`description` は、Python実装またはテストを変更して
hoiminで不足する振る舞いテストを見つけたい場面を明示し、暗黙呼び出しと明示呼び出しの
両方で発見できるようにする。

## スキル1: `hoimin-mutation-testing`

このスキルは、一つの実装変更を検査して、次に改善すべきテストを特定する。実行前に
次を確認する。

1. 実装のPythonファイル、対応するテスト、既存のテストコマンドを確認する。
2. 通常のテストコマンドを先に成功させる。失敗していればmutation testを開始せず、
   baseline failure とテスト失敗を修正する。
3. テストファイルではなく、変更したproduction sourceをmutation targetにする。
   source rootを特定できる変更には `--source <dir> --changed`、特定ファイルには
   `--file <path>`、さらに狭める必要がある場合には `--line` または `--symbol` を選ぶ。

標準の実行は `--profile focused --format json` とする。既に `hoimin` が利用可能なら
それを使い、利用できない場合は一回限りの実行に `uvx hoimin` または `pipx run hoimin`
を使う。`--` より後ろはテストコマンドのネイティブargvであり、shell command stringに
してはならない。JSON report はagentが作成したリポジトリ外の一時ディレクトリへ保存する。

終了コードは必ず分類する。`0` はsurvivorなし、`1` は解析対象として扱うsurvivorあり、
`2` は設定またはインフラエラー、`3` はbaseline failure、`4` は不完全実行、`130` は
cancelledである。`1` は期待される分析結果であり、報告を捨てたり実行失敗として停止したり
しない。`2`、`3`、`4`、`130` はテストを追加して先へ進まず、先に原因を解決または利用者へ
報告する。

survivorごとにmutated expression、元の振る舞い、変更後に失われるべき契約を確認する。
外部から観測できる結果を検証する最小のテストを追加または強化し、通常テストを再実行する。
mutantを殺すだけのproduction code変更、内部実装を固定するmock、無関係なテストの変更は
行わない。改善後は必要に応じて同じmutation targetを再実行するか、反復が必要なら
`hoimin-mutation-improvement` を使う。

## スキル2: `hoimin-mutation-improvement`

このスキルは、survivorの改善を反復するための状態機械である。開始時に対象selector、
`--profile`、operator選択、limits、テストargvを決め、その反復中は同じ設定を維持する。
テストだけでなくsourceも変更した結果は比較できるが、selectorやprofileを切り替えた
reportを同じ履歴へ混在させない。

各反復で次を行う。

1. 通常テストを成功させる。
2. `hoimin run --format json` の完全なreportを、古い順序が分かる一時ディレクトリへ
   一つずつ保存する。baseline failureまたは不完全なreportはprogress履歴に使用しない。
3. survivorがあれば、最も有用で説明可能な一つを選び、その振る舞いを検証するテストを
   追加または強化する。通常テストを再び成功させる。
4. 次の完全reportを保存した後、二件以上のreportがあれば、古い順に
   `hoimin progress --format json` へ渡す。終了コードではなく
   `latest.state` と `latest.consecutive_stalls` を判定に使う。

`latest.state` の扱いは固定する。

| 状態 | 行動 |
| --- | --- |
| `improving` | 停滞数がresetされた。残るsurvivorを一つ選び、次の反復へ進む。 |
| `stalled` | まだ既定のpatienceに達していない。別の説明可能なsurvivorを対象に一回だけ次の改善を試みる。 |
| `saturated` | 既定では三回連続の比較可能な停滞。反復を終了し、残るsurvivor、試した契約、停止理由を報告する。 |
| `regressing` | 反復を停止し、直前のテスト変更または対象の変化を診断する。悪化を隠すために次のテストを追加しない。 |
| `indeterminate` | 比較可能なcomplete reportが不足している。baseline failure、不完全実行、selector/profile変更を解決し、比較可能な履歴を作り直す。停滞として数えない。 |

survivorが0件のcomplete reportを得たときは、`saturated`を待たずに成功として終了する。
反復の終了時には、対象、実行したテストargv、保存したreport数、最後のprogress state、
追加・強化したテスト、残存survivorとその扱いを短く報告する。一時reportの永続保存や
`--session`/`--resume`の使用は、利用者が明示した場合だけ行う。

## 検証

実装時には、各skillのfrontmatterが `name` と `description` を持つこと、名前がディレクトリ名と
一致すること、対応する`.agents`と`.claude`のファイルが完全一致することを検証する。
また次の代表的な利用依頼に対して、skillの説明が適切に選択され、本文が必要な行動を導くことを
確認する。

- 「変更したPython機能に対してhoiminを実行し、テストの穴を見つけてほしい」
- 「survivorが残っている。改善が止まるまでテストを強化してほしい」
- 「baselineが失敗したが、mutation testを続けてほしい」

最後の依頼では、baselineを修復せずにmutation loopを進めないことを必須条件とする。
