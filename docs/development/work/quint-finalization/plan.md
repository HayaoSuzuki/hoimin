# 終了と所有権の検証計画

設計は [design.md](design.md)。`software-design-philosophy` で境界・知識の重複・不要な設定をレビューし、`executing-plans` に従って継続実行する。

1. `formal/quint/disk.qnt` に状態遷移と3 owned-root設定、誤った delivery 完了の負の対照、正常・失敗・保留の入力 schedule を作る。公開API adapterと生成履歴、状態不変・削除許可・終端性のPBTを追加。
2. `formal/quint/finalization.qnt` と CLI test-only child module を追加。失敗計測・未joinの固定、試行の不可逆性、outer/inner drain の差、空き容量の一度だけの query を照合。未joinを許す負の対照を検出。
3. `formal/quint/ownership.qnt` と session ownership の test-only child module を追加。モデルの lease と実OS handle/probe を照合し、プローブのIO失敗を正常な busy と区別する。誤って複数 owner を許す負の対照を検出。
4. runner registry・CI・fuzz manifest・seed・文書を更新。モデルも実装も同じ入口を再利用し、期待結果の別実装を input schedule に作らない。
5. 各段階5回レビュー、実装mutation、通常/全featuresテスト、TLC正負・再生成一致、全suite、fuzz、lintを確認してcommit。探索上限と内部fixtureの限界を最終記録に残す。
