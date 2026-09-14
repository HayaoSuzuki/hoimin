---
type: Contract
title: fingerprint入力globの共有走査
description: 複数のinclude globを一度のディレクトリ走査で解決し、入力順のエラーを維持する契約。
status: draft
source_revision: 165a2d284a1af92eb02ffd214ba8c0070c2f3808
sources:
  - id: design
    resource: ../../superpowers/specs/2026-09-14-issue-457-fingerprint-shared-walk-design.md
    revision: 67b82d65bfa6f4c13b7476531161c1852be50381
    working_tree: clean
    sha256: 9f7b93abdbe62ab58d1ca1f4c6bb3f3bbbdd4326956cc1dde9b14d27b4cfc4b7
  - id: implementation
    resource: ../../../crates/hoimin-cli/src/fingerprint_inputs.rs
    revision: 67b82d65bfa6f4c13b7476531161c1852be50381
    working_tree: clean
    sha256: c77fd4122114a522116e130fa0024c339409e528a63ba857dfd444b2ed24e5d5
  - id: tests
    resource: ../../../crates/hoimin-cli/tests/fingerprint_inputs.rs
    revision: 67b82d65bfa6f4c13b7476531161c1852be50381
    working_tree: clean
    sha256: 95ff6b857b2a13f8c22087d66e8437ad8c5cd0e3ce5cb88e58686298cc09f9b2
---

# glob解決の契約

`--fingerprint-include` を複数指定した場合、実装は有効な正のglobの和集合についてrootを一度だけ走査する。否定globは個別の照合だけに使い、別の引数が必要とする一致を走査用の和集合から除外しない。各globの一致状態は個別に保持するため、重複と重なりは出力レコードでは統合されるが、未一致の判定には各入力が残る。[^design][^implementation]

エラーはglobの入力順で選ぶ。先のglobが未一致または未対応ファイルを選び、後のglobが不正である場合も、先のエラーを返す。exact fileの検査は全globの成功後に入力順で行う。[^design][^tests]

走査はhidden・ignore設定とsymlink非追跡を従来どおり維持する。和集合に一致しない非UTF-8名、symlink、特殊ファイルを新たなエラーにしない。実装または`ignore` crateの照合方法を変えた場合は、この境界を再確認する。

[^design]: [Issue 457 design](../../superpowers/specs/2026-09-14-issue-457-fingerprint-shared-walk-design.md)。
[^implementation]: [fingerprint_inputs.rs](../../../crates/hoimin-cli/src/fingerprint_inputs.rs)。
[^tests]: [fingerprint_inputs integration tests](../../../crates/hoimin-cli/tests/fingerprint_inputs.rs)。
