# 追加監査のLeanモデルを正式な検証へ組み込む

2026-09-15の6組の追加監査から、6モデル・33定理・58生成ケースを `formal/HoiminOracle/` へ組み込んだ。集約ライブラリがモデルをimportし、既存のLean CIが依存順の直列ビルド、コーパスの再生成との一致、誤モデルの検出を検査する。

## 移行先

以下のパスは `formal/HoiminOracle/` からの相対パス。モデルは `HoiminOracle/<名前>Model.lean`、生成器は `<名前>AuditMain.lean` にある。

| 監査 | モデル・生成器の名前 | Lake実行名 | コーパス | 入力数 |
| --- | --- | --- | --- | --- |
| [with/finally](../../audits/2026-09-15-with-finally/README.md) | WithSuppression | generate_with_suppression | corpus/with-suppression.jsonl | 5 |
| [遅延注釈・集合ABC](../../audits/2026-09-15-annotation-followup/README.md) | DeferredAnnotation | generate_deferred_annotation | corpus/deferred-annotation.jsonl | 10 |
| [評価順序](../../audits/2026-09-15-evaluation-order/README.md) | EvaluationOrder | generate_evaluation_order | corpus/evaluation-order.jsonl | 7 |
| [値なし注釈](../../audits/2026-09-15-declaration-only/README.md) | DeclarationOnly | generate_declaration_only | corpus/declaration-only.jsonl | 14 |
| [コピー方針と再開](../../audits/2026-09-15-resume-copy/README.md) | ResumeCopy | generate_resume_copy | corpus/resume-copy.jsonl | 7 |
| [nullable適用条件](../../audits/2026-09-15-nullable-gates/README.md) | NullableGate | generate_nullable_gate | corpus/nullable-gate.jsonl | 15 |

モデルと定理の内容は監査版と同一である。生成器は正式なモジュールをimportし、既存の実行規約に合わせて先頭の `--` を処理する。有限探索と誤モデルの検出を `--sensitivity` に分離し、`--check` と `--output` ではコーパスだけを扱う。58入力のJSONLは監査版とバイト単位で一致する。

今後のモデルとケースの変更先は正式なプロジェクトである。`docs/audits/` のモデル、生成器、コーパス、再現スクリプト、ログは、対象コミット `5e631ef` の監査を再実行するためのスナップショットとして保持する。

## 証明と実装との対応の境界

Leanが確立したのは、例外抑制後の到達状態、初回注釈評価のキャッシュ保持、束縛と参照の順序、値なし宣言の保存性、コピー条件が変わったときの再利用禁止、型引数の木における除外の伝播など、各モデル内の性質である。WithSuppressionには、二重走査の葉の訪問回数と一度だけ転送する場合の訪問回数の証明も含む。Python解析器やコピー処理そのものの正しさは証明していない。

監査時の公開CLIとの比較は、各debug/releaseで計24 match・34 mismatch・0 infrastructure errorだった。今回、製品コードを変更せず、実装との照合も再実行していない。未解決の対応は各監査のREADME・対応表とIssue #556〜#565に残る。#562は候補精度の改善案であり、既存仕様が完全性を保証しているという意味ではない。#561の確保要求量の測定は監査資料に保存し、Leanの実装性能保証とは扱わない。

コーパスの `mode: strict` は元の監査の前提を満たす入力を表す。今回のCIで製品との一致を保証する印ではない。各 `replay.py` は不一致を報告する用途のままである。Issueの修正後に対応する入力をRustの厳密な回帰テストへ移す。

## 実施した検証

環境はmacOS arm64、Lean 4.32.2、CPython 3.14.7。既存のビルドキャッシュを利用した。

- 追加12モジュールと集約ライブラリのビルド、6生成器のfreshnessとsensitivity、計25コマンドが終了コード0。
- 6生成器の `--output` は正式コーパスと完全一致。各出力へ余分な改行を加えると `--check` が `stale corpus` で失敗し、改変を検出した。
- 上記37コマンドは、1プロセスずつ、20秒・合計RSS 2 GiB・50 ms間隔の監視で実行。最大3,711 ms、最大809,456 KiB。資源制限による停止なし。[全計測とsensitivity出力](2026-09-15-followup-lean-promotion.json)を保存した。`-negative-check` の6件は意図した非ゼロ終了である。
- `.venv/bin/python -m unittest discover -s tests -p 'test_*.py'`: 92件成功。最初のsandbox内実行では監視テスト4件が `ps` の制限で失敗し、プロセス監視を許可した環境で再実行した。CI登録の28テストは個別実行でも成功。

CIの全モジュール・全生成器の一覧は [.github/workflows/ci.yml](../../../.github/workflows/ci.yml) にある。今回の変更では追加部分と集約ビルドを実行し、既存全生成器の再実行やCIサービス上での実行は行っていない。

## 再実行例

リポジトリのルートから実行する。ほかのモデルも上表の名前に置き換える。先にモデル、次に生成器をビルドし、資源監視のあるコマンドを直列で使う。

```sh
cd formal/HoiminOracle
stats_dir=$(mktemp -d)
guard() {
  name="$1"
  shift
  python3 tools/lean_resource_guard.py \
    --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 50 \
    --stats "$stats_dir/$name.json" -- "$@"
}
guard model lake build +HoiminOracle.NullableGateModel:o
guard generator lake build +NullableGateAuditMain:o
guard freshness lake exe generate_nullable_gate -- --check corpus/nullable-gate.jsonl
guard sensitivity lake exe generate_nullable_gate -- --sensitivity
```
