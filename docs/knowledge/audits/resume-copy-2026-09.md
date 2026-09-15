---
type: Audit
title: コピー方針変更時のsession再開の追加監査
description: include/excludeの変更をfingerprintが区別せず旧判定を再利用する不具合とLean・公開CLIによる検証。
status: draft
catalog_revision: 5e631ef
sources:
  - id: report
    resource: ../../audits/2026-09-15-resume-copy/README.md
    working_tree: untracked
    sha256: 85e48710c73efb62a5ce970061dc6af55720579ee837a7c4e6fe762acfb61048
  - id: model
    resource: ../../audits/2026-09-15-resume-copy/ResumeModel.lean
    working_tree: untracked
    sha256: 624f2860dcec4b5087461a06c82857f133a4a7144b2a581df0a27dd179735983
---

# コピー方針を区別しない再利用

`5e631ef`でinclude/excludeを変更しても、旧条件のkilled/survivedを再利用する不具合を[#563](https://github.com/tokyogas-tech/hoimin/issues/563)に起票した。補助ファイルのコピー有無が変わるとテスト結果も変わるが、fingerprintにはその設定が含まれない。補助ファイルを明示fingerprintへ追加しても、root上のpath/hashが同じなら防げない。[^report]

# 証拠と限界

Leanは保存済み判定が正しいという前提の下で、コピー条件を比較するresumeが現条件の新規実行と一致することをモデル内で証明した。コピー条件変更時の再利用禁止と、任意の出力上限の違いだけでは互換性を失わないことも証明した。16条件の有限検査で、コピー条件を落とす規則と出力上限まで比較する規則を検出した。[^model][^report]

Lean生成7ケースを初回run・SQLite再開・新規runで照合し、debug/releaseとも3 match / 4 mismatchだった。4件はinclude/excludeの追加・削除であり、再開時の判定が新規実行と逆になる。設定据置きと出力上限のみの変更は一致した。実行基盤エラーは0、既存テスト64件は成功した。[^report]

# 再確認の契機

FingerprintInput、コピー選択、session load、保存済み結果の再利用を変更するときに再実行する。7fixtureはstrictで、一般定理と抽象状態の全列挙はmodel-onlyである。実装修正、fingerprint schema更新方針、正式CIへのケース移行、Windows/Linuxでの公開runは未実施である。[^report]

[^report]: [監査報告と再現手順](../../audits/2026-09-15-resume-copy/README.md)。
[^model]: [ResumeModel.lean](../../audits/2026-09-15-resume-copy/ResumeModel.lean)。
