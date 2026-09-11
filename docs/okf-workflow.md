# OKFを使った開発手順

OKF（Open Knowledge Format）は、概念ごとのMarkdownと索引で知識を整理する形式である。hoiminでは [docs/knowledge/](knowledge/index.md) に設計上の契約、判断、監査結果をまとめる。開発者は作業開始時に関連ページと出典を読み、変更によって説明が変わるページを同じ変更に含める。

## 作業開始時の参照

1. [カタログの範囲と読み方](knowledge/overview.md)で、資料の版と証拠の区別を確認する。
2. [入口の索引](knowledge/index.md)から関連する設計・監査ページを選ぶ。対象選択ならplan/verify、終了処理なら資源と終了処理、保存や再開ならsessionとレポートを読む。
3. ページの出典から元の設計書・報告と該当コードを確認する。監査対象のコミット、OS、入力、モデルの仮定を作業対象と照合する。
4. 変更する契約、未解決事項、必要な検証を整理する。関連ページがなければ原文とコードから調査し、作業結果を新規概念として残す必要があるか判断する。

設計書と現行コードが食い違う場合は、その違いを記録してから変更方針を決める。過去の監査で見つかった問題を現在も未修正と扱ったり、過去のテスト成功を今回の結果として記載したりしない。

## 更新対象の判断

| 変更・発見 | OKFで行うこと |
| --- | --- |
| CLI、設定、保存形式、互換性、終了コードなどの契約変更 | 対応する設計ページの条件と根拠を更新する |
| 解析、ランキング、資源制御、状態遷移などの設計判断 | 採用理由と適用範囲を既存ページに追記する。独立した話題なら概念を追加する |
| 不具合修正やテスト・Leanモデルの変更で保証範囲が変化 | 関連する契約・監査ページに修正と検証範囲を記録する |
| 新しい監査結果、未検証条件、従来の説明と矛盾する証拠 | 対象版を明示した監査概念を追加または更新し、関連設計ページから案内する |
| 再利用する開発・運用手順の追加や変更 | 手順の正本を更新し、必要なら `Playbook` から案内する |
| 設計書・監査報告の追加、移動、削除 | 原文一覧と、参照している概念・索引を更新する |
| 誤字修正、整形、契約も証拠も変わらない内部整理 | OKF更新は不要。PRまたは完了報告に理由を一文残す |

同じ話題の説明は既存ページを更新する。新規ページは後の作業で独立して参照する概念に限り、一時的な進捗記録をすべてOKF化する必要はない。元の設計書、実装、実行結果は既存の場所に保存し、OKFから根拠として参照する。

## 作成と出典の記録

`create-okf` スキルが使える環境では、たとえば `$create-okf 今回変更したsessionの互換性条件をdocs/knowledgeへ反映し、関連する監査と索引を点検してください` と依頼する。スキルがない環境でも、以下の規則で作成・更新できる。

このバンドルはOKF v0.2を使う。通常の概念ファイルには、先頭のYAML frontmatterに空でない文字列の `type` を置く。ここでは `title`、`description`、`status`、根拠を示す `sources` も記載する。`Contract`、`Decision`、`Audit`、`Playbook`、`Reference` はこのリポジトリの分類例であり、OKF標準の固定列挙ではない。

新規概念には `status: draft` を明記する。本文は、対象と用語、契約または判断、根拠、限界・未確認事項、再確認が必要な変更の順に、その話題に必要な項目を記載する。日本語は一段落一話題とし、設計、静的なコード確認、過去の実行結果を区別する。推量や未検証条件を推敲で断定へ変えない。

出典には `resource` とページ内で一意な `id` を記載し、対応する `[^id]` の脚注を主張に付ける。相対パスは文書の位置を基準にする。実装、スキーマ、結果ファイルへのリンクも根拠にできる。既存の独自メタデータは意味を確認して保持する。

版の記録には次のローカル規則を使う。

- コミット済みの出典は、実際に読んだコミットを `sources[].revision` に記録する。読んだファイルがその版と同一なら `working_tree: clean` とする。
- 未コミットの変更を読んだ場合は `working_tree: modified`、未追跡なら `working_tree: untracked` とし、読んだ内容の `sha256` を記録する。変更済み出典の `revision` は比較元のコミットを示す。将来のコミットIDを仮記入しない。
- `catalog_revision` はそのページの整理に使った基準コミット、`audit_revision` は監査対象のコミットを示す。過去の監査対象を現在のHEADへ機械的に置き換えない。
- 出典を読み直して本文を更新したときに、その出典の版・状態・ハッシュを更新する。無関係な過去の出典まで一括で更新しない。内容を確認していない出典は、メタデータだけを新しくしない。
- `working_tree` とハッシュは参照時点の記録である。後でコミットされたことだけを理由に記録を変更する必要はない。現在の出典と違う場合は、記録した版を確認し、現行の説明を更新すべきか判断する。

確認に使うコマンド例は `git rev-parse HEAD`、`git status --short -- <出典パス>`、`git show <参照コミット>:<リポジトリ内パス>`、`shasum -a 256 <出典パス>` である。出典の編集を終えてからハッシュを採る。未追跡資料を参照する変更を共有するときは、その資料も同じPRへ含めるか共有済みの出典を使う。

`verified` は、実際に内容を根拠と照合し、確認者と日時を記録できる場合だけ付ける。形式検査、文章の推敲、過去の成功結果の転記だけでは付けない。確認範囲を本文に書き、確認していない実装・OSへ保証を広げない。

## 索引の更新

新規概念は関連する索引からリンクし、[入口](knowledge/index.md)から到達できるようにする。ルート `index.md` のfrontmatterは `okf_version: "0.2"` だけとし、子ディレクトリの `index.md` と任意の `log.md` にはfrontmatterを付けない。索引は見出しと説明付きリンク一覧、ログは新しい日付を先にした記録にする。

[設計書一覧](knowledge/references/design-documents.md)は `docs/superpowers/specs/` のMarkdown、[監査・報告一覧](knowledge/references/audit-documents.md)は `docs/superpowers/reports/` と `docs/audits/` のMarkdownを対象にする。これらに原文を追加したら、一覧表・出典・脚注も更新する。原文の引用見出しは元の表記を保つ。実施計画を置く `docs/superpowers/plans/` はこの一覧の対象外である。

## 完了時の確認

OKFを変更したら、形式、参照、内容を別々に確認する。これは開発時とレビュー時の手順であり、現時点ではOKF専用のCIゲートはない。

1. **形式**: YAMLパーサーで先頭のfrontmatterを読み、マッピングと `type`、予約ファイルの構造を検査する。対象は `docs/knowledge/` 内のMarkdownに限定する。
2. **参照**: 出典IDと脚注の対応、リンク先、参照コミットまたはハッシュを確認する。新規・移動・削除された原文が一覧に反映され、入口から全概念へ到達できることを確認する。
3. **内容**: 変更後の主張と根拠、対象版、OS・入力・モデルの条件、日本語の用語と段落を読む。実装のテストは[開発ガイド](development.md)から変更に必要なものを実行する。

以下はリポジトリのルートで実行する最小のYAML・予約ファイル検査である。PyYAMLが利用可能な `python3` を使う。必要なら隔離環境へ `PyYAML==6.0.3` を導入する。この例はリンク、出典、内容の検証を含まず、OKF仕様全体の適合判定でもない。

```sh
python3 - <<'PY'
from pathlib import Path
import yaml

root = Path('docs/knowledge')
pages = sorted(root.rglob('*.md'))
assert pages, 'OKF bundle is missing or empty'
for path in pages:
    text = path.read_text(encoding='utf-8')
    header = text.startswith('---\n')
    if path.name in ('index.md', 'log.md'):
        if path == root / 'index.md':
            assert header, f'{path}: repository version declaration required'
            front, body = text[4:].split('\n---\n', 1)
            meta = yaml.safe_load(front)
            assert meta == {'okf_version': '0.2'}, path
        else:
            assert not header, f'{path}: reserved file must not have frontmatter'
        continue
    assert header, f'{path}: missing frontmatter'
    front, body = text[4:].split('\n---\n', 1)
    meta = yaml.safe_load(front)
    assert isinstance(meta, dict), f'{path}: mapping required'
    assert isinstance(meta.get('type'), str) and meta['type'].strip(), path
print(f'YAML and reserved-file checks passed: {len(pages)} Markdown files')
PY
```

PRまたは完了報告には、参照したOKFページ、更新したページまたは更新不要の理由、実行した検査と未確認事項を記載する。[PRテンプレート](../.github/pull_request_template.md)に記入欄がある。検査時点の件数を記録する場合は、その対象と日付を併記し、過去の検査結果と今回の結果を区別する。
