# CIの選択とworkflow検査

CIは変更判定と`CI result`を常に起動する。
PRではbaseとheadのmerge baseから、pushとmerge queueではイベントのbaseから差分を取得する。
削除とrenameの旧pathも分類し、未知のpath、差分取得失敗、手動実行では全検証を選ぶ。
Runner上のPythonは変更判定・集約を含めて3.14を明示する。

| 変更 | 選択する検証 |
| --- | --- |
| `docs/**/*.md`（`docs/json-schema/`以外）、`CONTRIBUTING.md` | Python品質検査とworkflow検査 |
| `crates/`、`vendor/` | Rust、formal、配布検証も実行 |
| `formal/` | Rustとformal検証も実行 |
| `tests/**/*.py` | Python、Rust、formal検証も実行 |
| `tools/release.py`、`tools/sbom.py`、`tools/capture_sbom.py` | Pythonと配布検証も実行 |
| `README.md`、`LICENSE` | 配布検証も実行（wheelに含まれる入力） |
| schema、fixture、wheel smoke、workflow、lockfile、その他 | 全検証 |

Python変更時にもwheel smokeを実行する。
nightly shuffleとfuzzの制限は維持する。
cgroup実機検証は、委譲済みRunnerを有効にしたmainへのpushだけで選択する。
`CI result`は変更判定の成功と有効な出力、固定された全jobの結果を検査する。
必要なjobのfailure・cancelled・skipped、結果の欠落は失敗になる。
選択しなかったjobだけはskippedを許容する。
PRの古いCIとpreviewだけをキャンセルする。
previewではタグ予約jobを意図的にskipするため、後続のbuild・validate・publishにもstatus関数を明示し、祖先jobのskipが暗黙の`success()`で伝播することを防ぐ。
各jobの直接の依存先にはsuccessを要求し、失敗やキャンセルを許容しない。

## 必須チェックの設定

2026-10-10のGitHub API確認では、`HayaoSuzuki/hoimin`のrulesetは削除とforce pushの禁止だけで、mainの必須チェックは未設定だった。
保護を有効にする際は条件付きjobを必須にせず、集約する`CI result`を必須にする。
旧組織のPulumi設定は移管済みのため対象外とする。
この変更で外部rulesetを変更する操作は行っていない。

## workflow検査の再現

```console
uv sync --frozen --group workflow --no-install-project
uv run --frozen --no-sync python -m tools.workflow_lint
uv run --frozen --no-sync pytest tests/test_workflow_lint.py tests/test_ci_selection.py tests/test_ci_selection_workflow.py tests/test_ci_metrics.py
```

`.github/workflows/`直下の全`.yml`と`.yaml`を列挙する。
actionlint 1.7.12、ShellCheck 0.11.0、zizmor 1.30.1をdependency groupとlockfileで固定し、Renovateでまとめて更新する。
zizmorはoffline・auditorでinformationalを含む全重大度を検査し、ShellCheckのルールを一括除外しない。
秘密情報・書込権限・SARIF投稿は不要で、fork PRも同じ検査を実行する。

actionlint 1.7.12は子プロセス起動前にstdinを書き込み、Windowsで長いscriptがpipe容量を超えると停止する。
actionlint内のShellCheck起動を無効にし、同じCLIからShellCheckを各bash/sh stepへ別途実行する。
OS間の差を避けるため全OSで同じ経路を使う。
Actions式の構文と信頼性はactionlintとzizmorで検査し、ShellCheckには式をplaceholderへ置き換えて渡す。

SBOMツールのPATHは使用するstepだけで設定し、`GITHUB_PATH`への書込みとその除外は不要にした。
WindowsのGit Bashでは`cygpath`でRunnerのnative pathを変換する。

除外は該当行の`zizmor: ignore[...]`と理由コメントに限定する。

| rule | 箇所と理由 |
| --- | --- |
| `dangerous-triggers` | releaseのclosed/mergedイベント。書込jobはマージ済みbase commitだけをcheckoutする |
| `artipacked` | タグ予約job。タグのpushに認証情報が必要 |
| `self-hosted-runner` | 委譲cgroupの実機検証。main pushかつ明示的opt-inのみ |

zizmorはPython shell内の`GITHUB_ENV`評価をサポートせず警告する。
このツール上の限界を、検出できたと扱わない。
隔離fixtureでは不正な式、ShellCheck違反、template injection、過大な権限を実ツールで拒否することを検査する。

## 実行時間の測定

```console
uv run --frozen --no-sync python -m tools.ci_metrics --run RUN_ID --output docs/performance/ci/after-ci.json
```

認証済み`gh`でrunと最新attemptの全jobを取得する。
経過秒数、実行job数、job時間の合計、created_atからstarted_atまでの待ち時間を保存する。
依存jobの待機も含むrun開始からの遅延はqueue秒数とは別に記録する。
欠けた時刻を0秒に置き換えず、skipped jobを実行数に含めない。

導入前のCI run `38023706140`は694秒、10job、合計2803job秒、queue合計28秒。
対応するpreview `38023706209`は624秒、5job、合計1387job秒、queue合計19秒。
取得結果は[測定記録](performance/ci/)に保存した。
SBOM実装変更の1例であり、文書変更の平均や削減率ではない。
導入後は文書、Rust、formal、配布の各変更で同じ条件のrunを取得し、失敗率とskip結果も比較する。
hosted Runnerでの導入後実測はローカル検証から推定しない。

設計と各段階の5回のセルフレビューは[検証記録](reviews/2026-10-10-ci-selection.md)に記載する。
