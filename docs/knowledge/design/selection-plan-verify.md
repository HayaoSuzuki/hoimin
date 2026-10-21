---
type: Contract
title: 対象選択・plan・verifyの契約
description: 候補発見と実行の分離、ランキング、部分集合、保存形式の版の不整合を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: six-perf-record
  resource: ../../maintenance/2026-10-21-issues-1039-1040-1041-1042-1044-1045/README.md
  revision: 4e2b4417acc4999676b47e6ee9e80b1b2fe8f607
  working_tree: modified
  sha256: 29bd14f404dab541e5b203dcd73c3323c2779795647ff6358da6656aa2cd3c8b
- id: six-perf-verify
  resource: ../../../crates/hoimin-cli/src/plan.rs
  revision: ab940366085cccfadcfe182bfbf8f447613e25bc
  working_tree: clean
  sha256: 4990f857e033b4006cb0270f6bd612bcf47cf16d0a53f04bfbe14773f29dee17
- id: six-perf-verify-heap
  resource: ../../../crates/hoimin-cli/tests/manifest_buffer_lifetime.rs
  revision: ab940366085cccfadcfe182bfbf8f447613e25bc
  working_tree: clean
  sha256: 0ff957ee52c84f0803faac5ec7555e508d1b84542f5e3df9e3287b8064f1fbf1
- id: issue-1033-config
  resource: ../../../crates/hoimin-core/src/config.rs
  revision: 91b345996357900ee3d7b7f956d80d13c5aacba3
  working_tree: modified
  sha256: 06299876caa5d1d33dca7c06806aa118091f314535dc4c87f2cd8d71a7a2e23d
- id: issue-1033-verify-test
  resource: ../../../crates/hoimin-cli/tests/plan.rs
  revision: 91b345996357900ee3d7b7f956d80d13c5aacba3
  working_tree: modified
  sha256: faf967c3713789320bff4be62407907dc0c752826a554219b006fe9e00524a67
- id: issue-1031-selection
  resource: ../../../crates/hoimin-core/src/machine/selection.rs
  revision: 3054427a1b1b9a7d2f5cdd1b6ce62053e6c417ac
  working_tree: modified
  sha256: 915dee30a763915c155e1ac223ee42e9b3517f3f97c56e3d93f4fbb3788691c2
- id: issue-1026-git
  resource: ../../../crates/hoimin-cli/src/target/git.rs
  revision: 05c909c54a3bdf3485db2c833456fabe83c7dfaf
  working_tree: modified
  sha256: 2a5c02ed960b8b4d539cb5aa1419b0e30f7f611f65054462ce31ddded6d835ba
- id: issue-1026-environment-tests
  resource: ../../../crates/hoimin-cli/tests/changed_git_environment.rs
  working_tree: untracked
  sha256: 7d2f0f73c5e4ab0a3f711848e88d5fd2817384917a03f2a0ec57a13da80a7dd8
- id: perf-three-selection
  resource: ../../../crates/hoimin-python/src/analyzer/rust/selection.rs
  revision: f72134d76d28d87cd728305ed224f298df41af4a
  working_tree: clean
  sha256: 55784a1d7923b782f84f95b5cf67e9e7ae9e16e86a56b8bb0569a51039bfd5c5
- id: issue-1004-config
  resource: ../../../crates/hoimin-core/src/config.rs
  revision: 6d78789eca05fad577b2a8dcce83077f9389ee06
  working_tree: modified
  sha256: 6c0aa32efc26a872a6f5c3aaf32c7b9cfad9cb5ae58d4aa83a9ce5c26461fba4
- id: issue-1004-preparation-tests
  resource: ../../../crates/hoimin-cli/tests/checked_config.rs
  revision: 6d78789eca05fad577b2a8dcce83077f9389ee06
  working_tree: untracked
  sha256: 5b5f8758c22e950dcbab0dd95a4058a81aee0f95ece51f58760436fbfb15339e
- id: issue-1002-human-text
  resource: ../../../crates/hoimin-cli/src/human_text.rs
  revision: b02ee7d51709de664ee88db68abca34025db3487
  working_tree: untracked
  sha256: 00322fd04c933a8ee575787e2f65bbb641059fd043a8493d76beca57a6ccf357
- id: issue-1002-preview
  resource: ../../../crates/hoimin-cli/src/plan/preview.rs
  revision: b02ee7d51709de664ee88db68abca34025db3487
  working_tree: modified
  sha256: 12c88134f0d9dc3ec5b184e0e656376efa44bc2aabafc7bbcd4d79e751ec3956
- id: issue-1002-preview-tests
  resource: ../../../crates/hoimin-cli/tests/plan.rs
  revision: 3d9e33d178a451e39e182495f4a5bb3a4c5ebfa0
  working_tree: modified
  sha256: 19dd709bc3460827f3884558d6924b39fe6d877404558de8a7fe7128002a9ffb
- id: issue-998-selection
  resource: ../../../crates/hoimin-core/src/machine/selection.rs
  revision: 50b9c089ca73aeca2205636a2a9e1823cd244d39
  working_tree: clean
  sha256: 0f8d6ecf192787fc8b759801c380b02c4db5fc0d46261b9b58998af08fa67f85
- id: issue-997-stop
  resource: ../../../crates/hoimin-core/src/machine.rs
  revision: 50b9c089ca73aeca2205636a2a9e1823cd244d39
  working_tree: clean
  sha256: 8516c95c43ed3442929466044a72b4d1941d697760353617e7c68a8b46c1650f
- id: task-10-current-preview
  resource: ../../../crates/hoimin-cli/src/plan/preview.rs
  revision: c74410f56c8389ef246567318ddcade724dbe3f7
  working_tree: clean
  sha256: a4abe92ead1ac8ee04c620d93b36e967a461c1cf836947c5f31ee697ba41d322
- id: issue-959-config-tests
  resource: ../../../crates/hoimin-cli/tests/cli_config.rs
  revision: 30d158c7ead3252c5c0aa398e8ab1819ffa9d75b
  working_tree: modified
  sha256: 4909fdcdbe7abdb60014a1e77739db718143b6694cd3a100d9dd4440b3f9491a
- id: issue-959-cli
  resource: ../../../crates/hoimin-cli/src/cli.rs
  revision: 30d158c7ead3252c5c0aa398e8ab1819ffa9d75b
  working_tree: modified
  sha256: 4e2fb37221f97b90dc0f1b4369859cf17f07c0b93d0525fafffc0ec1d296a43d
- id: issue-959-plan
  resource: ../../../crates/hoimin-cli/src/plan.rs
  revision: 30d158c7ead3252c5c0aa398e8ab1819ffa9d75b
  working_tree: modified
  sha256: ecee9e236c23c92c24820d2744501c6583c6201fe544caf295be932a0fd10942
- id: issue-959-preview
  resource: ../../../crates/hoimin-cli/src/plan/preview.rs
  revision: 30d158c7ead3252c5c0aa398e8ab1819ffa9d75b
  working_tree: modified
  sha256: a4abe92ead1ac8ee04c620d93b36e967a461c1cf836947c5f31ee697ba41d322
- id: issue-959-tests
  resource: ../../../crates/hoimin-cli/tests/plan.rs
  revision: 30d158c7ead3252c5c0aa398e8ab1819ffa9d75b
  working_tree: modified
  sha256: b485e12eaefc55bca6f1db689d76a5112e6a7ec03afd9e991e4a3c9519af2ce2
- id: issue-959-record
  resource: ../../maintenance/2026-10-16-issue-959.md
  revision: 30d158c7ead3252c5c0aa398e8ab1819ffa9d75b
  working_tree: untracked
  sha256: cd7db87afcbf271a519bcb4c5407da7bdbc7958a78a434437a88776aae5da44c
- id: issue-936-plan
  resource: ../../../crates/hoimin-cli/src/plan.rs
  revision: 5237d1e53a2c8b34fca80f6eeb7d644a01d732e2
  working_tree: modified
  sha256: e3319cca686aaaac9089d48ef2c7f546f1fdff626dc20859c4781359036a5427
- id: issue-936-review-input
  resource: ../../../crates/hoimin-cli/src/review/input.rs
  revision: 5237d1e53a2c8b34fca80f6eeb7d644a01d732e2
  working_tree: modified
  sha256: 97ec7d0c094f0702b576e1fe7bb741ec9f901e1c8608361dba401e7d6526cb6f
- id: issue-936-tests
  resource: ../../../crates/hoimin-cli/src/review/tests.rs
  revision: 5237d1e53a2c8b34fca80f6eeb7d644a01d732e2
  working_tree: modified
  sha256: 08b1b50da26fd0938d8fc8f8089a67f702220f5075fca847a42a0e440a760904
- id: issue-936-compatibility
  resource: ../../compatibility.md
  revision: 5237d1e53a2c8b34fca80f6eeb7d644a01d732e2
  working_tree: modified
  sha256: b1d7318075ad936964d4d1bba3c53be93ca3bdb8672fe662973ceda6d6fe0c9a
- id: issue-923-config
  resource: ../../../crates/hoimin-core/src/config.rs
  revision: 7c3a9105c432ec1e0ba2483ada42b10133d071a6
  working_tree: modified
  sha256: 1c0680d4872c468f4c1c3fb6433b589da1f7752bc6f10b70d71eba876bcefb08
- id: issue-923-compatibility
  resource: ../../compatibility.md
  revision: 7c3a9105c432ec1e0ba2483ada42b10133d071a6
  working_tree: modified
  sha256: b060c02db7f6e3adc035a61f2d954caa387674b64cbb273269587e170baf0c1c
- id: issue-915-design
  resource: ../../superpowers/plans/issue-915/design.md
  working_tree: untracked
  sha256: d1895363d3374a621fba3a6623b545127ad490aaafee46122fbaebd72c44ef9b
- id: issue-915-git
  resource: ../../../crates/hoimin-cli/src/target/git.rs
  revision: 54093222abe08c79e8e36e4656ee04b9c6d8dda0
  working_tree: modified
  sha256: a1955fa38410aa474fd36e41dd8a041b949d84eefa19c6c9e604473d15d47ff0
- id: issue-915-tracked
  resource: ../../../crates/hoimin-cli/src/target/git/tracked.rs
  revision: 54093222abe08c79e8e36e4656ee04b9c6d8dda0
  working_tree: modified
  sha256: 798eecf029794d88779e6e5f897d2a0ced356a44bd9dab4a8bf477a2ca7788d4
- id: issue-915-review
  resource: ../../superpowers/plans/issue-915/review.md
  working_tree: untracked
  sha256: a5d5101ec961409d0c5e51f636e0aee5d922cfef9e0f371b40fe7fd90b2078cc
- id: issue-911-design
  resource: ../../superpowers/reports/issue-911/design.md
  working_tree: untracked
  sha256: 9cafb2a20f48f581aa311e1bd6b0b8bd0676d75e2552363d5f33a61cc8256644
- id: issue-911-review
  resource: ../../superpowers/reports/issue-911/review.md
  working_tree: untracked
  sha256: 7f51e40d998ae77447b6cf6c02d229901c332c49685e946ee5da210a72231088
- id: issue-899-changed-selection
  resource: ../../superpowers/specs/2026-10-15-issue-899-design.md
  revision: 7b6cb19ba0181759f6b5726f985c5fd7700d313e
  working_tree: clean
  sha256: fa68b3e4aa56ab0ee3dd19f818a0b932a033dce550dbc869f956c261578b501f
- id: issue-899-changed-review
  resource: ../../superpowers/reports/2026-10-15-issue-899-review.md
  revision: ad65b8ce55b0dabc59712775e362d5d614b27559
  working_tree: modified
  sha256: 50a53fa428a46289ede33f60ae1bcc342f8cfa3b2a03e37ebc5ad127ff5444bc
- id: issue-886-duration
  resource: ../../superpowers/plans/2026-10-14-issue-886.md
  working_tree: untracked
  sha256: 788e07872b540e6db6952d7c8bdfd3c7f15ca55435f4996480793343a4198d93
- id: issue-850-execution
  resource: ../../maintenance/2026-10-13-verified-execution.md
- id: issue-888-canonical-enums
  resource: ../../superpowers/plans/2026-10-14-issue-888.md
  revision: 5c0215cb65e57d2758c4ec1708cd50d96b1bcdfd
  working_tree: modified
  sha256: ac7f3e70687df6d1beab5fe896a50efa15e6ffd62dfd5a0d0832d7e6dc48c5c0
- id: issue-843-config
  resource: ../../../crates/hoimin-core/src/config.rs
  revision: 16285fdd71b9db1da6570e7b3dbc4d234c3928fe
  working_tree: clean
  sha256: 16a4ceacacb685bf8d9440da50fa4219a001ec655a64a6d3ab8e94b5ac9dc46d
- id: issue-843-plan
  resource: ../../../crates/hoimin-cli/src/plan.rs
  revision: 16285fdd71b9db1da6570e7b3dbc4d234c3928fe
  working_tree: clean
  sha256: 02d0e43624c9f64ac2372b31ebc3a36495024fc0bdc4191b13a57a3144a040ac
- id: issue-759-staged-report
  resource: ../../superpowers/reports/2026-10-11-issue-759-staged-verification.md
  working_tree: untracked
  sha256: 728fe755181db6852830289d031d48725115531d2e90b26f7784b44d890a447e
- id: issue-695-design
  resource: ../../superpowers/reports/issue-695/design.md
  revision: 76f03acdefefdc03aff6973b0d2b49f6f3598837
  working_tree: clean
  sha256: 01880cfdffc342bb4b70a51d139ce77d08f3e932ba512500f72e7c1fb9920915
- id: issue-695-review
  resource: ../../superpowers/reports/issue-695/review.md
  revision: 76f03acdefefdc03aff6973b0d2b49f6f3598837
  working_tree: clean
  sha256: e0601ed731dfa3357e9482070fb313177bdb89fca0405c30e984a59bec851a38
- id: issue-454
  resource: ../../superpowers/specs/2026-09-14-issue-454-fixed-batches-design.md
  working_tree: untracked
  sha256: 10e05c4a8d518d5334e191a2999d2bd9f5c198569a3db650969f51adcc097dd1

- id: issue-599
  resource: ../../superpowers/specs/2026-09-25-issue-599-verify-preview-design.md
  revision: 99655f08bb941db0e9e888f3b5d373ec6f11440e
  working_tree: clean
  sha256: c024231b308a39d92f38fe7035630d6f364be449c80f98267197933fcf286d93

- id: issue-599-preview
  resource: ../../../crates/hoimin-cli/src/plan/preview.rs
  revision: 99655f08bb941db0e9e888f3b5d373ec6f11440e
  working_tree: clean
  sha256: 75fcd7faf4cf52262f120bb68912ae33f7e9728c3ed12eb1b329593e6d3c725b

- id: issue-599-schema
  resource: ../../json-schema/verify-preview.schema.json
  revision: 99655f08bb941db0e9e888f3b5d373ec6f11440e
  working_tree: untracked
  sha256: 8bdb3d181c6acfc34c99253135b14a07c5367b60f05e1f92f7e4908b533246e4

- id: issue-458
  resource: ../../superpowers/specs/2026-09-14-issue-458-verify-metrics-design.md
  working_tree: untracked
  sha256: 9348d92fc7357d35f6092240f58cd681672864de808daa41b989f0cb717c4887

- id: issue-480-symbol
  resource: ../../../crates/hoimin-cli/src/target/mod.rs
  working_tree: modified
  sha256: 97fc31c5b4eb885660de54c8711c4f187974e3c0b82ffca172a10f41ca7b2529
  revision: 98969d7a840362f78dceb12f91cc5188214f68d5
- id: issue-480-symbol-spec
  resource: ../../superpowers/specs/2026-09-14-issue-480-source-encoding-design.md
  working_tree: clean
  sha256: 55a99855ee216bfff5afdfd5ffdeeab6c13e1877ec751b0ee88c9a35cb78c37e
  revision: 98969d7a840362f78dceb12f91cc5188214f68d5
- id: issue-476-report
  resource: ../../superpowers/reports/2026-09-14-issue-476-symbol-diagnostics-review.md
  working_tree: untracked
  sha256: bff2a183f0c702c556d19c832b59836613d9e7a20975d89893d5d1c7fa0292ec
- id: issue-476-design
  resource: ../../superpowers/specs/2026-09-14-issue-476-symbol-diagnostics-design.md
  working_tree: untracked
  sha256: 9d9087b95cf13cedc4d75626d4cb87b93c41bb45f399a33dbb9a1cbe333c0092
- id: issue-476-target
  resource: ../../../crates/hoimin-cli/src/target/mod.rs
  working_tree: modified
  sha256: 2679747599bc69451f05128d6f6ab08fb22fb65460179d4f9a40d71e936aa3d5
  revision: 8b33167a049e3cae0fc05e96ccf2253c660b7023
- id: issue-476-analyzer
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  working_tree: modified
  sha256: b4e03399f3d7e6786fcecb7c876bd94232a16bea5c7d61f98feb31fdd5f69c19
  revision: 8b33167a049e3cae0fc05e96ccf2253c660b7023
- id: issue-476-tests
  resource: ../../../crates/hoimin-cli/tests/plan.rs
  working_tree: modified
  sha256: 410ad372b3ddf1ac57afcb9d1197f3395e02bf8e492553e74b7759f6ee1a3f47
  revision: 8b33167a049e3cae0fc05e96ccf2253c660b7023
- id: issue-453
  resource: ../../superpowers/specs/2026-09-14-issue-453-scoped-discovery-design.md
  sha256: cce7701d3552cc3887c8129b01589736e6defd31389db98de76ac32b619527aa
  working_tree: clean
  revision: e59ce2726f71e7c366b8855664b5cce43a13c6a6
- id: issue-475
  resource: ../../superpowers/specs/2026-09-14-issue-475-range-normalization-design.md
  sha256: a73a0b51c51b10d1d958585acbc0455d3c9d18ec340ef6acec053c2c5427081f
  working_tree: clean
  revision: f3440ac435747d831db6a8b0d93758242060a8cf
- id: issue-474
  resource: ../../superpowers/specs/2026-09-14-issue-474-selector-index-design.md
  working_tree: untracked
  sha256: 0325588c52127c73cb41fbf33ef9d9e244915a6de4de5f6b3134b31312500ef6

- id: issue-456
  resource: ../../superpowers/specs/2026-09-14-issue-456-validation-context-design.md
  working_tree: untracked
  sha256: a3f08279a0c60e016cef85ba37827f2b3be4fd5006bbb2068cce2c75bdf8991b
- id: issue-600-design
  resource: ../../superpowers/specs/2026-09-25-issue-600-target-membership-design.md
  revision: fb7f1f1a44d94a8ea6e5e19dfba8efbb6dc656c1
  working_tree: clean
  sha256: cd568b65781ba9aee5262f8452e8516d707fcdd74081097fcca9855cd5a86aa9

- id: issue-456-review
  resource: ../../superpowers/reports/2026-09-14-issue-456-validation-context-review.md
  working_tree: untracked
  sha256: 5042ef83fb71837543be9fb358b2453931bdf70ce77f6033381e5756632b09f0
- id: issue-600-review
  resource: ../../superpowers/reports/2026-09-25-issue-600-target-membership-review.md
  working_tree: untracked
  sha256: d4823d9add9a31d43a0cd1e80be6126b0593b80e1ca5638e44f9f766dcb593fc
- id: issue-600-code
  resource: ../../../crates/hoimin-cli/src/plan.rs
  working_tree: clean
  revision: 5c5dbea5b97eb1a4a58be3e90c272ba86c61ab33
  sha256: 6c25091c136ce679c1d1b1d4530977afefa016c79322fcd5cd858c35c71fb44f

- id: issue-456-code
  resource: ../../../crates/hoimin-cli/src/plan.rs
  working_tree: modified
  sha256: c5fa356be1e661d21ca8fd1837eca63fa30cc472b4ac815391e8005f086e90e9
  revision: 165a2d284a1af92eb02ffd214ba8c0070c2f3808
- id: issue-477
  resource: ../../superpowers/specs/2026-09-11-issue-477-import-roots-design.md
  working_tree: untracked
  sha256: a30ed5f129c635e75431212d590171ab03e5973640a4d6624775ee43fd81dcdd

- id: issue-473
  resource: ../../superpowers/specs/2026-09-11-issue-473-symbol-ranking-design.md
  working_tree: untracked
  sha256: d1748256ef730d871a2158c57e8c5601145d78e32288c269cac720f60ad10618

- id: issue-459
  resource: ../../superpowers/specs/2026-09-11-issue-459-record-size-design.md
  working_tree: untracked
  sha256: ced8501765b2babcd15bac56139f575c00cdb779a5149bb4a6e231ea75e0af7f

- id: issue-452
  resource: ../../superpowers/specs/2026-09-11-issue-452-shared-exclusions-design.md
  working_tree: untracked
  sha256: c61ec9ebe8ec989d0e39ede76d57a3513ccb5768631b6e081275448a92e2b89a
- id: initial
  resource: ../../superpowers/specs/2026-07-21-agent-plan-verify-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: ranked
  resource: ../../superpowers/specs/2026-07-27-ranked-plan-top-verify-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: diverse
  resource: ../../superpowers/specs/2026-09-08-issue-433-diverse-selection-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: readme
  resource: ../../usage.md
  revision: 5a45bdc3bd444771c4c57e30fe211f174850ce93
  working_tree: untracked
  sha256: 6894a34743882cc26fdd1f39cb536c94dfc20d5f1dcf33b3cb7f3da9b13e6bef
- id: development
  resource: ../../development.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: plan
  resource: ../../../crates/hoimin-cli/src/plan.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: issue-709-design
  resource: ../../superpowers/reports/issue-709/design.md
  working_tree: untracked
  sha256: 4b5d6005177bfb151a3a9a6dce053918c571ed970e9d2ed5c1761c2991773d21
- id: issue-709-plan
  resource: ../../superpowers/reports/issue-709/plan.md
  working_tree: untracked
  sha256: 4c475534f4f19a29ad98324afc6613cf6e897c1c1c7d3877205b2cd21e2f13eb
- id: issue-709-assessment
  resource: ../../superpowers/reports/issue-709/assessment.md
  working_tree: untracked
  sha256: c3fb86ebf25c5d4b8bfa2f9eb13fb57faf1d276c1e0e1d6706406da5d2b083fa
- id: issue-709-review
  resource: ../../superpowers/reports/issue-709/review.md
  working_tree: untracked
  sha256: 291049426d815f46c6b68d75c81415e80024165972fc60df65f0fcfb09ed9800
- id: issue-710-design
  resource: ../../superpowers/reports/issue-710/design.md
  working_tree: untracked
  sha256: 187d95a722fc212fa5748bd95b031035ecc34f89f43633a12d2b29e034d7701e

- id: issue-710-plan
  resource: ../../superpowers/reports/issue-710/plan.md
  working_tree: untracked
  sha256: 664949692555d984ab034f9468f745f958fb16b529dee9f0b75e10a30d060f7a

- id: issue-710-assessment
  resource: ../../superpowers/reports/issue-710/assessment.md
  working_tree: untracked
  sha256: 9e099f6d6125e40eed282924e41381fd0ed60d1a6f4f507dca3eeda14a21dece

- id: issue-710-review
  resource: ../../superpowers/reports/issue-710/review.md
  working_tree: untracked
  sha256: 37e8886b574a7226c882551b72683241c31bcd39ed0654f417c9569f63ed4e31

- id: issue-887-environment
  resource: ../../superpowers/plans/2026-10-14-issue-887.md

- id: issue-949-config
  resource: ../../../crates/hoimin-core/src/config.rs
  revision: ddb8ed9504beb04c06bfe11821ebcafffa332714
  working_tree: modified
  sha256: 9518c2b6370e7b91a835ce1a8d64aaf21c6530d7658cdceab142499250b5f642
- id: issue-949-record
  resource: ../../maintenance/2026-10-16-issue-949.md
  revision: 35c21145ccdfb6a044f9ae1dad2a5e4ff90deece
  working_tree: modified
  sha256: bca74014f517c33728e844b9e876a77b24ef3025c7e42fc75afb89d5849580f1
- id: issue-971-evidence
  resource: ../../maintenance/2026-10-18-issues-971-979/task-5-report.md
  working_tree: untracked
  sha256: 37f183a85687a1f2bdbf03067a57288c492458b37259268ae5b5171bb31c7d3f

- id: issue-979-compatibility
  resource: ../../compatibility.md
  revision: 4ebed4457519920e9a28f8f653d8ce96985acdad
  working_tree: clean
  sha256: 3db98744cba6c95b2828d98298038c52242135a2607f2855c58d100205243fb4

- id: issue-979-evidence
  resource: ../../maintenance/2026-10-18-issues-971-979/task-6-report.md
  working_tree: untracked
  sha256: 3cb4dde52478f7d80d8bf88f46dcedc2c504b9ed1449a9e29ffd58780026449c
- id: issue-985-admission
  resource: ../../maintenance/2026-10-18-issue-985.md
  revision: c293cf62325f2e39e07760770adeba4a592bea9d
  working_tree: clean
  sha256: d9584be78df3b4d8810d1c52e6188d09cabfbc6156769d7c85b7cf8049a5f305
- id: issue-989-defaults
  resource: ../../../crates/hoimin-core/src/config.rs
  revision: ac554aa9fe4d3907ed4a42f6c89486e6f599c897
  working_tree: clean
  sha256: 2642c2a372ddf6c4f2993aeed1f871bccd6ab2262a317f376a1b9eb7ed1d9782
- id: issue-989-evidence
  resource: ../../maintenance/2026-10-19-issue-989-default-selection.md
  revision: ac554aa9fe4d3907ed4a42f6c89486e6f599c897
  working_tree: clean
  sha256: f1d1bbc8c45525a6b489a98beab41b313ae24d4bbfd4c8c6362abdfd5f72116f

- id: issue-988-decoder
  resource: ../../../crates/hoimin-cli/src/plan/schema.rs
  revision: c4078e0dadc58bdbc7953b6d5a23cd76160d6ecf
  working_tree: clean
  sha256: 4ec0b7caf5caafe1227f072c0ee4dc0672813af355095c967508433b70dc740c
- id: issue-988-evidence
  resource: ../../maintenance/2026-10-19-issue-988-schema-probe.md
  revision: c4078e0dadc58bdbc7953b6d5a23cd76160d6ecf
  working_tree: clean
  sha256: 5f8ec28392a16d65c6588bc4a6722d1a41b643b23e0e695e74eb3fcd521c1c1a

- id: issue-984-compatibility
  resource: ../../compatibility.md
  revision: c4078e0dadc58bdbc7953b6d5a23cd76160d6ecf
  working_tree: modified
  sha256: 881f774b6a135f3d50f3acca3e379a0c8f0742349fd84964d526caa4073c421a

- id: issue-1003-source-acquisition
  resource: ../../../crates/hoimin-cli/src/plan/source.rs
  working_tree: untracked
  sha256: c6e684a074211c3c85618b02063003408ca4af1e80196666057593ac2e49d65b
- id: issue-1003-source-safety-tests
  resource: ../../../crates/hoimin-cli/src/plan/validation_tests.rs
  revision: ccef1941710e20cff8d391283cc21bcfbd7650aa
  working_tree: modified
  sha256: acac0b4024130ac6bdd91d68ec7805eefc2414e0b2123e9e138b8b00f24b288c
- id: issue-1003-source-heap
  resource: ../../../crates/hoimin-cli/tests/plan_source_heap.rs
  working_tree: untracked
  sha256: a41daec552a0ef497201a3876589934ac5271a825a41e70c7d3c864a84f1c905
---

# Checked normalized settings (Issue #1004)

`RunConfig<E>` now owns accepted normalized settings behind an immutable view.
Raw conversion, normalized construction data, and typed serde acceptance share
the semantic validators for selection, import roots, operators, argv, and limits.
Callers consume `into_data()` to edit fields and must pass the result through
`TryFrom` before execution. `PlanConfig` remains persisted data, with fallible
conversion to execution settings. Header validation still precedes source and
fingerprint work; `VerifiedPlan` retains checked selection IDs and protected inputs
with the accepted config; preview payload ownership is described below.[^six-perf-verify]

The request-to-snapshot environment transition still checks the exact requested
names. Configuration acceptance performs no filesystem work and does not establish
freshness or destination safety. Preparation owns those checks. These changes
preserve plan, report, and fingerprint representations. See the
[Rust caller migration](../../development.md#checked-execution-configuration).[^issue-1004-config][^issue-1004-preparation-tests]
Historical source revisions and verification records below retain their original
scope; local macOS checks do not establish native Linux or Windows results.

[^issue-1004-config]: [Checked configuration implementation](../../../crates/hoimin-core/src/config.rs).
[^issue-1004-preparation-tests]: [Preparation boundary regressions](../../../crates/hoimin-cli/tests/checked_config.rs).

# Checked execution selection (Issues #997 and #998)

`RunSelection::{all, explicit, ordered}` constructs execution membership and
immutable verification provenance together. Explicit accepts CandidateIds;
ordered accepts Top or Sample and checks the unique selected count. Sample
requires the exact saved ID order before deduplication. Empty Explicit and Top
selections remain valid when selected is zero. `RunState::with_selection`
consumes this owner; provenance cannot be attached later. Membership, ordered
Collecting/Ready state and explicit matched history have one owner.[^issue-998-selection]

Ordered discovery and waiting queues contain IDs only. After a complete spool
scan validates membership, an available worker requests its next body by ID from
the I/O-owned position index. The requested order, last duplicate body and
missing-ID timing remain observable contracts. See
[ordered discovery and stopping](runtime-lifecycle.md#ordered-candidate-discovery-and-stop-ownership)
for cancellation and read-failure ownership. This bounds waiting-body retention
by worker admission; selected metadata, plan loading and report retention have
separate costs.[^issue-1031-selection]

Ordered stopping borrows the requested IDs to build one temporary rank map per
merge. Stable sorting preserves requested order with candidate-sequence fallback
for IDs without a rank. The map is derived state rather than persistent selection
authority. Operation-count tests establish bounded rank preparation and lookup
work, not a wall-time performance guarantee.[^issue-997-stop]

[^issue-1031-selection]: [ID-only ordered phases](../../../crates/hoimin-core/src/machine/selection.rs).

Historical analyzer source entries retain their recorded paths and revisions.
For current analyzer ownership and retrieval of moved historical files, see
[the analyzer contract](analyzer.md#shared-pure-python-analysis-issues-995-and-996).

# Single normalized root (Issue #949)

`Selection.root` is the only stored project root in normalized run and plan
configuration. Raw conversion moves the root into selection; preparation and
run/plan conversion retain it. Target discovery, source reads, workspace copying,
process working directories and review project matching use that same value.
No filesystem normalization occurs during config conversion.[^issue-949-config]

Issue #949 removed the outer root at plan schema 9/report schema 7 without an adapter.
Current versions are plan 10/report 8, as described below.
Missing-selector and changed/symbol source checks remain in the shared selection
validator. The root mismatch error and its synchronization duty disappear.
The [migration notice](../../compatibility.md#single-normalized-project-root-in-0110)
explains regeneration and reader updates. The #843 section below records the
superseded two-field validation design.[^issue-949-config][^issue-949-record]

[^issue-949-config]: [Normalized configuration](../../../crates/hoimin-core/src/config.rs).
[^issue-949-record]: [Issue #949 design, self-reviews and verification](../../maintenance/2026-10-16-issue-949.md).


# 候補発見と実行の分離

`plan` は変異候補と実行条件を記録したファイル（manifest）を作る。この段階では、元コードのテスト実行（baseline）、テスト用コピーの作成、結果保存用sessionの作成を行わない。[^initial][^readme]

続く `verify` は保存した条件で候補を実行する。対象ソースと、利用者が変更検知用に明示した入力（fingerprint入力）が変わっていないかをbaseline前に確認する。baselineは毎回実行し、plan経由ではsessionの結果を再利用しない。manifestには実行条件が含まれるため、利用者が信頼するローカルのファイルを使う契約である。[^initial][^readme]

# 保存したEnumの現行名（Issue #888）

型付きの保存設定は、timeoutの`auto` / `fixed`、引数の`unix` / `windows`、operatorの`type_dict_mapping`を要求する。
旧名`Auto`、`Fixed`、`Unix`、`Windows`、`type_mapping`のserde aliasは廃止した。
現行writerの出力表現を変えず、旧名のplanを拒否する。旧planは再生成し、過去のreportは生成元バイナリで読むか実行し直す。
progressの設定を不透明なJSONとして扱う方針は維持し、typed設定を要求するplan/reviewと区別する。[^issue-888-canonical-enums]

旧名の拒否とdurationの正値制約は別の入力条件として扱う。
正準名の`fixed`ではzeroがserdeで拒否され、正の値では上限の意味検証へ進む。
旧名`Fixed`のunknown-variantエラーをzero検証の証拠には数えない。[^issue-888-canonical-enums][^issue-886-duration]

[^issue-888-canonical-enums]: [Issue 888の設計・実装・検証記録](../../superpowers/plans/2026-10-14-issue-888.md)。

# 対象を絞る指定の組合せ

対象ファイルを選んだ後、行範囲や関数・クラスなどのシンボル指定で、そのファイル内の範囲を狭める。`--changed` は明示的に選んだ対象とGitの変更行との共通部分を取る。複数指定を組み合わせる場合は、それぞれ単独で動くことに加え、範囲を交差させた結果も確認する必要がある。[^readme]

# 順位と実行集合

- `--candidate` は候補ID、`--top` は上位件数を指定する。両者は排他的で、どちらかの選択が必要である。
- `strict` は保存した順位の先頭から選ぶ。`diverse` は同点の候補群の中でファイルを順に巡回する。点数が高い候補群を先に消費する順序は保つ。
- `verify` は順位を再計算せず、planの実行上限を引き継ぐ。
- 候補発見を途中で打ち切った `truncated` なplanでは、上位N件は保持済み候補内の上位である。未発見の候補を含む全体の上位とは判定できず、verifyも完全実行とは報告しない。[^ranked][^readme]

`diverse` では、同点の候補をファイルごとの待ち行列（queue）に分ける。後続設計は、空になった待ち行列を巡回から外し、偏った分布で候補のないファイルを繰り返し調べる操作を減らす。総候補数をN、選ぶ件数をKとすると、待ち行列の作成はO(N)、選択時のqueue操作はO(K)を目標とする。選択順序を保つことが条件であり、この性能上の議論は同点の候補が特定ファイルに偏る場合を扱っている。[^diverse]

# 明示symbolの子要素とランキング

Issue #473 の設計では、明示symbolと同じファイルにある子symbolにも `explicit_symbol` の250点を一度だけ加える。`Box` は `Box.check` や `Box.Inner.check` に一致し、`BoxOther` には一致しない。親子のselectorが複数一致しても加点を重ねず、別ファイルの同名symbolには適用しない。[^issue-473]

順位の意味が変わるため、ランキング規則の版を3から4へ進める。保存形式のschema版は実装の3を維持する。旧ランキング版のplanはbaseline前に拒否して再生成を案内し、保存済みの順位を暗黙に変更しない。公開planの順位とverify --topの実行候補を照合する。既存Lean oracleの明示symbol入力は真偽値であり、今回の名前階層の解決自体を証明しているわけではない。[^issue-473]

# カタログ作成時のmanifest版の不一致

| 出典 | 記載・静的に観測した値 |
| --- | --- |
| 7月21日初期設計 | plan manifest v1 |
| README・開発資料 | plan v2の記述。開発資料ではランキング規則の版をv3として区別 |
| カタログ対象の `plan.rs` | `PLAN_SCHEMA_VERSION = 3`。読込み時にこの値との一致を要求 |

初期設計から版が変わったことに加え、カタログ作成時の公開文書と実装にも不一致があった。上表はコードを読んだ結果であり、CLIで旧版を入力する試験は行っていない。この表は当時の不一致の記録であり、Issue #473 の設計では現行資料をschema3・ranking4へ揃える。保存形式の版とランキング規則の版も別々に確認する。[^initial][^readme][^development][^plan]

# 関連する監査と再確認条件

複数の対象指定や打切り条件は[境界監査](../audits/boundary-2026-09.md)、候補順序と保持上限は[解析器](analyzer.md)を参照する。保存形式、順位規則、候補集合、変更検知用入力、verifyの準備段階を変更したら、設計・実装・公開説明を再照合する。

# Issue 477: import設定の保存

`--import-root` を対象選択から独立した順序付き設定としてplanに保存し、verifyへ引き継ぐ。import rootだけでは変異対象を指定したことにならない。plan schemaは実装のv3からv4へ進め、旧planはbaseline前に拒否して再生成を案内する。この独立ブランチのranking規則はv3を維持する。以前の資料の版表記はその時点の記録として扱う。[^issue-477]

[^initial]: [2026-07-21-agent-plan-verify-design.md](../../superpowers/specs/2026-07-21-agent-plan-verify-design.md)。
[^ranked]: [2026-07-27-ranked-plan-top-verify-design.md](../../superpowers/specs/2026-07-27-ranked-plan-top-verify-design.md)。
[^diverse]: [2026-09-08-issue-433-diverse-selection-design.md](../../superpowers/specs/2026-09-08-issue-433-diverse-selection-design.md)。
[^readme]: [利用方法](../../usage.md)。
[^development]: [development.md](../../development.md)。
[^plan]: [plan.rs](../../../crates/hoimin-cli/src/plan.rs)。

Issue #473と#477を統合した状態では、plan schemaは4、ranking ruleも4となる。前者はimport rootの保存形式、後者はsymbolの子孫への加点規則を表す独立した版である。個別の設計書にある版は、その設計時点の記録である。[^issue-473][^issue-477]

[^issue-477]: [2026-09-11-issue-477-import-roots-design.md](../../superpowers/specs/2026-09-11-issue-477-import-roots-design.md)。

[^issue-473]: [2026-09-11-issue-473-symbol-ranking-design.md](../../superpowers/specs/2026-09-11-issue-473-symbol-ranking-design.md)。

# 候補の保存サイズ上限（Issue #459）

planの候補にも実行用spoolと同じ2 MiBのレコード上限を適用する設計とした。JSONのエスケープとUTF-8、および末尾の改行1バイトを含むサイズで判定する。plan生成とverifyの事前検証で超過を拒否し、直接runする場合は既存のbaseline後の解析段階で不完全な実行として報告する。[^issue-459]

[^issue-459]: [Issue #459: Executable candidate record limits](../../superpowers/specs/2026-09-11-issue-459-record-size-design.md)。

# 対象探索とコピーの組込み除外（Issue #452）

仮想環境やキャッシュなどの組込み除外を対象探索とコピーで共有する設計とした。`--include` は組込み除外を解除しない。除外場所のファイルを `--file` または `--line` で指定した場合は、対象パスと除外場所の外を選ぶ対処方法を示して対象解決時に拒否する。[^issue-452]

[^issue-452]: [Issue #452: Shared workspace exclusions](../../superpowers/specs/2026-09-11-issue-452-shared-exclusions-design.md)。

# exact selectorの対象発見（Issue #453）

fileまたはlineだけを指定した場合、rootからの探索は指定ファイルとその親ディレクトリに限定する。root起点の探索、ignore/include/exclude、組込み除外、platformの大小文字規則は維持する。sourceまたはsymbolを含む場合は、対象列挙が必要なため全体探索を継続する。[^issue-453]

限定探索では、選択経路の外にあるportable pathへ変換できない名前を診断しない。指定ファイル、親経路、または全体探索で発見した不正な名前は従来どおり診断する。この境界により、無関係なファイル名がexact selectorの成否を左右しない。[^issue-453]

[^issue-453]: [Issue 453: Scope discovery for exact selectors](../../superpowers/specs/2026-09-14-issue-453-scoped-discovery-design.md)。

# 明示した行範囲とsymbolの正規化（Issue #475）

同じファイルへ指定した行範囲とsymbolは、selectorを検証しながら収集し、全selectorの解決後にファイルごとに一度正規化する。行範囲は開始位置で整列して重複・重なり・隣接を統合し、symbolは整列して重複を除く。入力途中の不正な行範囲とパスの診断順は変えない。[^issue-475]

この処理は明示selectorの解決だけを対象とする。Git変更行との交差、候補ランキングの行索引、対象ファイルの発見に使う別の正規化処理には適用しない。[^issue-475]

[^issue-475]: [Issue 475: Normalize explicit selector groups once](../../superpowers/specs/2026-09-14-issue-475-range-normalization-design.md)。

# 明示selectorから対象ファイルを引く索引（Issue #474）

明示したfile、line、symbolのmoduleを対象ファイルへ対応させる処理では、発見済みファイルからプラットフォーム別のパス同値キーを使ったPythonファイル索引を一度作る。各selectorはこの索引を検索する。発見済みファイル数をF、selector数をSとすると、順序付き索引の構築と検索に要する比較回数はO(F log F + S log F)となる。[^issue-474]

元の発見済みファイル一覧は、source rootからの列挙と出力順を決める正本として残す。Windowsで大小文字だけが異なる複数のパスが同じキーになる場合は、従来の順序で最初に見つかるPythonファイルの表記を索引に保存する。LinuxとmacOSで実行した試験からWindows実機の動作は判断しない。[^issue-474]

[^issue-474]: [Issue 474: Explicit selector file index](../../superpowers/specs/2026-09-14-issue-474-selector-index-design.md)。

# verifyのファイル単位の前処理（Issue #456）

要求候補のdescriptor検証では、同じファイルのハッシュ、UTF-8検証、行索引を一度だけ構築して共有する。ファイル単位で検証した結果を要求IDの従来の順序で取り出すため、複数ファイルにまたがるエラーの優先順を維持する。候補の内容・位置・stable IDと、後続の再発見による照合は省略しない。[^issue-456][^issue-456-code]

ソースbytesと借用contextは一ファイルの検証が終わると解放し、要求候補の参照と結果だけを保持する。ソース全体の前処理は要求ファイルのbytes合計に比例する。行内の列計算や再発見を含むverify全体の計算量を保証する変更ではない。操作回数とrelease計測、実行環境、未測定のメモリ指標は今回の検証記録を参照する。[^issue-456-review]

[^issue-456]: [2026-09-14-issue-456-validation-context-design.md](../../superpowers/specs/2026-09-14-issue-456-validation-context-design.md)。

[^issue-456-review]: [2026-09-14-issue-456-validation-context-review.md](../../superpowers/reports/2026-09-14-issue-456-validation-context-review.md)。

[^issue-456-code]: [plan.rs](../../../crates/hoimin-cli/src/plan.rs)。

# verifyの対象所属索引（Issue #600）

要求候補が選択対象に含まれるかの確認には、対象一覧から一度作る `HashSet<&Utf8Path>` を使う。キーに `Utf8Path` を借用することで、従来のパス成分による同値性を保つ。文字列への変換、大小文字の統一、ファイルシステム上の実体への解決は行わない。IDの存在確認、対象所属、ファイル読取り、descriptorとstable IDの検証という順序も維持する。[^issue-600-design][^issue-600-code]

対象数Fの索引構築と要求候補数Cの検索により、従来の候補ごとの全対象走査を除く。ハッシュ集合の通常の前提では所属確認はO(F+C)となるが、パス長とハッシュ衝突の費用、候補検証や再発見、verify全体の計算量を含む保証ではない。保存形式、ランキング、strict/diverseの選択は変更しない。[^issue-600-design]

操作回数の回帰テスト、パス同値性と複数エラーの順序確認、公開API・CLIの観測結果と限界は検証記録を参照する。[^issue-600-review]

[^issue-600-design]: [Issue #600: Index verify target membership](../../superpowers/specs/2026-09-25-issue-600-target-membership-design.md)。

[^issue-600-review]: [Issue #600: Target membership review and measurements](../../superpowers/reports/2026-09-25-issue-600-target-membership-review.md)。

[^issue-600-code]: [plan.rs](../../../crates/hoimin-cli/src/plan.rs)。

# 固定バッチの範囲選択（Issue #454）

`verify --top N --offset K` は、指定した選択規則による全体の順序からK件を飛ばし、続く最大N件を選ぶ。strictは保存順位、diverseは同点内でファイルを巡回する全体順序に対して範囲を適用する。同じplanと選択規則で隣接範囲を指定すれば、各範囲のID集合は重複しない。[^issue-454]

offsetは0始まりでtopとの併用を必須とし、候補ID指定とは併用しない。保持数以上のoffsetと空planはbaseline前に拒否する。末尾を越える件数は残りに切り詰め、max_mutantsは飛ばした件数を除く実際のバッチに適用する。truncated planの範囲は保持済み候補だけであり、完全実行の扱いへ変更しない。[^issue-454]

JSON/JSONLのmutant記録に実際の候補IDを残す。バッチの再実行では範囲と選択規則を保存し、同じID集合のレポートだけでprogress履歴を作る。異なるバッチのscoreや飽和判定は合成しない。[^issue-454]

[^issue-454]: [Issue 454: Fixed verify batches](../../superpowers/specs/2026-09-14-issue-454-fixed-batches-design.md)。

# 実行前の候補preview（Issue #599）

`verify PLAN --dry-run` は通常のverifyと同じplan・ソース・fingerprint・候補の検証を行い、選択結果を出力して終了する。baselineと変異テスト、workerコピー、session作成、実行metricsは発生しない。`--metrics` との併用は拒否する。候補ID・top指定の有効なpreviewはtruncated planでも終了コード0となる。sample指定はtruncated planを拒否し、入力不正は2となる。[^issue-695-design]実行時の資源確保までは確認しない。[^issue-599]

JSONとJSONLは、独立したschema 2の `verify_preview` オブジェクトを1件出力する。`candidates` 配列は選択順で、各行にID、保存rank、バッチ内の `selection_order`、path、lineを含む。rank・選択順・行番号は1始まりである。top指定の `offset` は0始まり、明示ID指定ではnullとなる。既存の選択metadataに加え、保持数を `retained_candidates` に記録する。[^task-10-current-preview]

Issue #599の導入時はschema 1だった。以下の当時の設計・実装・schemaの出典は、その履歴として保持する。[^issue-599][^issue-599-preview][^issue-599-schema]

strict/diverseの順序は既存の選択結果を使い、明示IDは候補の再発見順を使う。引数の順序や編集可能な保存sequenceには依存しない。選択順は実行予定の順序であり、並列workerの完了順や資源不足時の実行完了を保証しない。truncated planのpreviewは保持済み候補だけを対象とする。変異結果がないためprogress履歴には使わない。[^issue-599][^issue-599-preview]

[^task-10-current-preview]: [現在のpreview.rs](../../../crates/hoimin-cli/src/plan/preview.rs)。

[^issue-599]: [Issue 599: Verify selection preview](../../superpowers/specs/2026-09-25-issue-599-verify-preview-design.md)。
[^issue-599-preview]: [preview.rs](../../../crates/hoimin-cli/src/plan/preview.rs)。
[^issue-599-schema]: [verify-preview.schema.json](../../json-schema/verify-preview.schema.json)。

# verifyの運用metrics（Issue #458）

`verify --metrics PATH` は候補ID指定とstrict/diverseの上位選択に対応する。相対パスは呼出し時の作業ディレクトリを基準とし、planへ保存しない。出力先の正規化とshellのmetrics収集・保存はrunと共用する。出力設定以外の実行条件と候補選択は維持する。[^issue-458]

planの形式、候補、ソース、fingerprint、再発見の検証に失敗した場合、metrics出力先は変更しない。shell実行開始後のbaseline失敗、総時間制限、保存失敗はrunと同じ規則で扱う。metricsの各段階の時間には、先行するplan検証を含めない。[^issue-458]

[^issue-458]: [Issue 458: Verify operational metrics](../../superpowers/specs/2026-09-14-issue-458-verify-metrics-design.md)。

# 明示symbolの定義存在確認

Issue #476 の変更では、対象ファイルの解決後、Git変更行との交差前に、ASTの関数・クラス定義に指定qualnameが存在するかを確認する。メソッド、ネストした定義、async関数、packageの `__init__.py` を含む。代入名やimport先は定義とみなさない。不存在ならファイル、qualname、対応する元のselectorを含む診断で、run・plan・verifyをbaseline前にexit 2で拒否する。[^issue-476-design][^issue-476-target][^issue-476-analyzer]

定義が存在すれば、operator、profile、行範囲、Git差分によって候補がなくても有効である。候補上限やverifyの候補部分集合で検査対象を省略しない。構文不正は存在確認不能として扱い、不存在とは診断しない。今回の公開CLIテストでは、空のGit差分、別定義だけの変更、候補のない定義、verifyで実行候補のないファイルのselectorも確認する。[^issue-476-tests]

symbol指定ファイルは候補解析前にもparseする。既存のAST深さ制限を使うが、この対象解決時のI/Oとparseはanalyzer discovery timeoutの対象外であり、全処理の時間上限を新たに保証する変更ではない。[^issue-476-design]

[^issue-476-design]: [2026-09-14-issue-476-symbol-diagnostics-design.md](../../superpowers/specs/2026-09-14-issue-476-symbol-diagnostics-design.md).

[^issue-476-target]: [mod.rs](../../../crates/hoimin-cli/src/target/mod.rs).

[^issue-476-analyzer]: [rust.rs](../../../crates/hoimin-cli/src/analyzer/rust.rs).

[^issue-476-tests]: [plan.rs](../../../crates/hoimin-cli/tests/plan.rs).

今回の自己レビュー、検証結果と未確認範囲は[Issue #476 の報告](../../superpowers/reports/2026-09-14-issue-476-symbol-diagnostics-review.md)に記録する。[^issue-476-report]

[^issue-476-report]: [Issue #476: symbol diagnostics review](../../superpowers/reports/2026-09-14-issue-476-symbol-diagnostics-review.md).

# symbol定義確認での文字コード

Issue #480 は、Issue #476 の定義存在確認にも共通decoderを使う。Latin-1で書かれた関数名をUnicodeのqualnameとして照合し、候補解析と異なるUTF-8限定の読み込みを残さない。文字コードの対応範囲と元バイト位置の扱いは[解析契約](analyzer.md)を参照する。[^issue-480-symbol][^issue-480-symbol-spec]

[^issue-480-symbol]: [target/mod.rs](../../../crates/hoimin-cli/src/target/mod.rs).
[^issue-480-symbol-spec]: [Issue #480 design](../../superpowers/specs/2026-09-14-issue-480-source-encoding-design.md).

# 反復検証の導入判断（Issue #709）

反復実行の組み込みは保留とする。論文が測った個別テストの不安定化と、hoimin が観測するコマンド全体の終了状態は一致しない。固定候補の既存 verify 反復による限定試行では、制御された交互終了を観測できた一方、実パッケージの狭いチェックで分類の変化は得られなかった。これは不要・安定性の証明ではない。実利用で判断を変える終了状態の揺らぎと追加コストを記録し、必要性を再評価する。新しい CLI オプションや分類は追加していない。[^issue-709-assessment]

[^issue-709-design]: [design.md](../../superpowers/reports/issue-709/design.md)。

[^issue-709-plan]: [plan.md](../../superpowers/reports/issue-709/plan.md)。

[^issue-709-assessment]: [assessment.md](../../superpowers/reports/issue-709/assessment.md)。

[^issue-709-review]: [review.md](../../superpowers/reports/issue-709/review.md)。

# Issue #710: 開始行による選択の分散

`verify --top N --selection-policy line-diverse`は、同じscoreの候補をファイルと開始行でまとめ、保存rankに従う順で巡回する。上位scoreを先に処理し、全体の順序を決めてからoffset/topを適用する。複数行にまたがる変更は開始行のグループに属し、2巡目以降の候補も削除しない。既定はstrictのままとし、既存のdiverseはファイル単位の巡回を続ける。[^issue-710-design]

reportのpolicyは`line_round_robin_v1`。新しいpolicyを拒否する旧readerには更新が必要になる。planのrankや実行制限は変更せず、scoreの対象は選択した部分集合のままとする。少数候補で触れる行を増やす機能であり、不具合検出率の向上を保証するものではない。実装・Leanの照合計画と作成例の比較結果を別途記録した。[^issue-710-plan][^issue-710-assessment][^issue-710-review]

[^issue-710-design]: [design.md](../../superpowers/reports/issue-710/design.md)。

[^issue-710-plan]: [plan.md](../../superpowers/reports/issue-710/plan.md)。

[^issue-710-assessment]: [assessment.md](../../superpowers/reports/issue-710/assessment.md)。

[^issue-710-review]: [review.md](../../superpowers/reports/issue-710/review.md)。

# Issue #695: seed付きの無作為抽出

`verify PLAN --sample N --seed S` は、完全なplanに保存された候補集合から順位にかかわらず重複なしで抽出する。Nは正の整数、Sは0を含む符号なし64ビット整数で、両方の指定を必須とする。sampleはtop・候補ID・offset・順位用policyと併用できず、seedはsampleでのみ使える。母集団はplan作成時のselector・profile・operatorによって制限される。[^issue-695-design]

抽出前に空集合と `truncated: true` を拒否する。要求数が母集団数を超える場合は全候補を選び、実際の抽出数がplanの `max_mutants` を超える場合は実行前に拒否する。選択規則 `splitmix64_fisher_yates_v1` は64ビットの疑似乱数生成、剰余の偏りを避ける棄却、前方からの部分Fisher–Yates交換を固定する。同じplan・件数・seed・規則の版で候補IDと実行を割り当てる順序を再現する。並列実行の完了順は対象外である。[^issue-695-design]

previewとrunの選択metadataは `mode: sample`、`scope: sampled_candidates` を使い、要求数・実際の抽出数に加えて `sampling` に母集団数・seed・選択順のIDを残す。完了とscoreの対象は標本であり、標本外の候補をkilledに数えない。timeoutやキャンセルは従来の未完了判定を維持する。反復評価ではplan・件数・seedを保存し、追跡対象の変更でplanを再作成した場合は選択IDも確認する。progressは従来の候補同一性・曖昧性の判定を続ける。[^issue-695-design][^issue-695-review]

実装コミット `76f03ac` に含まれる記録では、macOS arm64でRust 2,720件・Python 504件、4,096ケースのプロパティテスト、Lean生成値と公開CLIの360ケース照合、計1,615,243入力のファジングを実施した。Leanの証明はモデルの交換・prefix・受付条件に関するもので、全入力に対するRustの正しさを証明したものではない。比較測定の対象は作成したPythonプロジェクト2件であり、実運用での速度向上や実不具合の検出率は未確認である。抽出規則・レポート・実行順を変更するときは、これらの契約と検証範囲を再確認する。[^issue-695-review]

[^issue-695-design]: [Seeded verification sampling](../../superpowers/reports/issue-695/design.md)。
[^issue-695-review]: [Issue 695: review and verification record](../../superpowers/reports/issue-695/review.md)。

## Typed environment snapshots (Issue #887)

CLI normalization creates `RunConfig<EnvironmentRequest>` with canonical names.
The run and plan frontends capture native values once during preparation;
execution, fingerprint generation, and persisted `PlanConfig` use the validated
`EnvironmentSnapshot`. `Untracked` has no metadata; `Captured` contains private,
nonempty canonical names and a valid aggregate digest. The transition rejects a
snapshot for another name set. Verify recaptures the saved names and rejects a
changed snapshot before the baseline. [^issue-887-environment]

Before Issue #949, plan schema 8 and report schema 6 retained the explicit `environment` object
introduced in plan schema 6 and report schema 5.
The earlier omitted-field encoding was also produced by the then-current writer;
its removal is a versioned format change, not a legacy-only cleanup. Regenerate
older plans and retain older reports with their producing binary. Fingerprint
schema 11 preserves the same bytes, checked against pre-refactor exact digests.
The 32-case environment oracle retains its tracked/untracked and value-state
semantics; it does not prove digest collision resistance or native OS encoding.

[^issue-887-environment]: [Issue 887 design and verification record](../../superpowers/plans/2026-10-14-issue-887.md).

## Issue #759の段階的検証の調査

screen/confirmの別planを作り、screenで確実にsurviveした同一descriptorの候補だけを独立したconfirm verifyへ進める運用は、既存機能で可能だった。
保存planのcommandをverifyでoverrideしない。
stageごとにbaseline成功、complete、実terminationを検査し、timeout・失敗・未実行を確認済みsurvivorと混同しない。
各reportを分けて保持し、異なるcommandのscoreや反復試行を混ぜない。[^issue-759-staged-report]

2026-10-11のWindows/Python 3.14の7候補fixtureでは、二段階と単独全体の最終survivor集合は一致し、生成markerの持越しもなかった。
ただしbaseline/reset込みwall timeは5.519秒対3.919秒で、二段階の方が長かった。
confirmに人工sleepを置いた一回の小規模測定であり、実OSSの高速化の根拠ではない。
具体的な遅い利用projectの需要と費用低減が確認できるまで、product APIや外部manifest/DSLは追加しない。
stage metadata、自動集約、resumeを製品機能として実装したとは扱わない。[^issue-759-staged-report]

[^issue-759-staged-report]: [#759 段階的検証の実現性調査](../../superpowers/reports/2026-10-11-issue-759-staged-verification.md)。条件、運用手順、セルフレビュー各5回。

# Historical configuration root consistency (Issue #843)

At Issue #843, `RunConfig::validate()` and `PlanConfig::validate()` rejected a configuration
whose `root` differs from `selection.root`. The shared selection validator
owns this rule. Equality compares path components without filesystem access
or canonicalization; callers must validate after public-field mutation or
deserialization.[^issue-843-config]

The common manifest header validator delegates normalized configuration checks
to `PlanConfig::validate()`. Schema, kind and ranking-version errors retain
precedence, and root mismatch retains the diagnostic `selection root differs
from normalized config root`. Other selection errors now precede fingerprint
environment errors, and invalid configuration precedes invalid manifest records
or candidates. Read-only review does not capture the live environment; verify
still does. Saved fields, schema versions and fingerprint encoding are unchanged
by this validation refactor.[^issue-843-config][^issue-843-plan]

[^issue-843-config]: [Configuration validation](../../../crates/hoimin-core/src/config.rs).
[^issue-843-plan]: [Manifest validation](../../../crates/hoimin-cli/src/plan.rs).

# 検証済み plan の実行入口（Issue #850）

Rust から保存済み plan を実行する場合は、`prepare_verify` または `prepare_verify_selection` が返す `VerifiedPlan` を使う。設定・候補選択・選択の由来・保護対象は、この値から実行側へまとめて渡す。参照による確認と metrics 出力先だけの上書きを経て、借用 writer では `VerifiedPlan::execute` を呼び出す。CLI の所有 writer も同じ検証済み情報を使い、既存の出力処理を維持する。[^issue-850-execution]

設定や候補 ID を個別に渡せた selected/ordered 実行ラッパーは削除した。公開された設定フィールドや可変参照から `VerifiedPlan` の検証済み設定を書き換えることはできない。実行時に候補 ID が見つからない場合の拒否は、shell 内のテストから非公開の実行関数を呼んで確認する。CLI・候補順序・plan/session/report の保存形式は変更しない。[^issue-850-execution]

[^issue-850-execution]: [実行 API の整理・移行・検証記録](../../maintenance/2026-10-13-verified-execution.md)。

# Nonzero duration input boundary (Issue #886)

`NonZeroDuration` rejects zero during raw CLI conversion and serde
construction through the same checked constructor. Saved run and plan
configurations therefore reject zero before semantic validation. Plan verify
and review retain their invalid-input categories and exit code 2; the serde
diagnostic identifies `duration`, while raw CLI errors retain the timeout flag.
Later validation already rejected zero before this change; this is a type
invariant repair, not an unchecked-execution finding.[^issue-886-duration]

The wrapper guarantees positivity only. The inclusive timeout ceiling,
automatic timeout calculation, overflow checks, and individual jobs/process bounds
remain configuration-validation responsibilities. Valid Duration objects,
fixed timeout variant names, and schemas remain intact. This invalid-input
tightening does not require a release-floor increase.
The verification record covers the existing Lean corpus adapter, not a new
formal proof.[^issue-886-duration]

[^issue-886-duration]: [Issue 886 implementation and verification record](../../superpowers/plans/2026-10-14-issue-886.md).


## Typed changed selection (Issue #899)

Plan schema 8 and report schema 6 store changed selection as null when disabled,
or as an object containing required bounded `context` and optional `diff_base`
when enabled. Boolean changed values and the former sibling context/base fields
are rejected. Regenerate older plans; preserve older reports with their producing
binary or rerun. No conversion updates existing files. The CLI's flags, explicit
selector intersections, Git base semantics, and effective target fingerprints
retain their meaning. [^issue-899-changed-selection][^issue-899-changed-review]

[^issue-899-changed-selection]: [Typed changed selection design](../../superpowers/specs/2026-10-15-issue-899-design.md).

[^issue-899-changed-review]: [Issue 899 review and verification](../../superpowers/reports/2026-10-15-issue-899-review.md). Current executed scopes, fixture corrections, limitations and release-floor checks.

## Validated verification selection (Issue #911)

`VerificationSelection` owns counts and a private mode enum. Explicit IDs and
Top carry truncation; only Top carries a variable policy; only Sample carries
validated sampling evidence. Mode, scope and fixed policies derive from that
state. Constructors and Deserialize require a positive request, a selected count
bounded by the request, and nonempty unique sample IDs bounded by population.
A sample's count equals its ordered evidence length and its plan is complete.
Non-sample reports may select zero candidates. [^issue-911-design]

Current writer JSON and version numbers remain stable. Contradictory known fields
are rejected, including null sampling on a non-sample mode; additive outer report
metadata remains accepted. The sampling object remains closed as documented in
the schema. Human output and preview use typed projections. The dated review
records executed checks and limitations; it does not establish new formal
properties of sampling or dispatch. [^issue-911-review]

[^issue-911-design]: [Typed verification selection design](../../superpowers/reports/issue-911/design.md).

[^issue-911-review]: [Review and verification](../../superpowers/reports/issue-911/review.md).


## Git and Python line units (Issue #915)

Tracked Git patches count LF-delimited rows. Python physical rows count LF, CRLF,
and bare CR; `a\rb\n` spans one Git row and two physical rows. The private
`GitChangedRanges` value carries excluded/normalized tracked ranges until its
consuming conversion reads the current bytes. The resolver then combines those
physical ranges with unborn-indexed and untracked ranges, which already use
physical rows. Public `LineRange`, target intersections, context, and persisted
schemas retain their existing meanings. [^issue-915-design][^issue-915-git][^issue-915-tracked]

The tracked collector retains the current full-diff path for unscoped requests
and global rename pairing. Unit separation does not change binary/deletion
exclusions, scoped path batching, Git context expansion, or merge-base selection.
The implementation record distinguishes pre-refactor characterization, temporary
sensitivity probes, and current verification; it adds no formal proof or
cross-platform execution claim. [^issue-915-review]

[^issue-915-design]: [Git line-unit design](../../superpowers/plans/issue-915/design.md).
[^issue-915-git]: [Git resolution and consuming conversion](../../../crates/hoimin-cli/src/target/git.rs).
[^issue-915-tracked]: [Tracked patch collection](../../../crates/hoimin-cli/src/target/git/tracked.rs).
[^issue-915-review]: [Review and verification](../../superpowers/plans/issue-915/review.md).

# Required saved plan arrays (Issue #923)

`PlanConfig` requires explicit `import_roots` and `fingerprint_files` arrays.
Missing and null values fail decoding before verify can start a baseline; `[]`
remains a valid configured list. Raw CLI defaults, candidate selection and
fingerprint calculation remain unchanged. Issue #923 retained plan schema 7;
Issue #936 below advances it to 8. Regenerate plans missing either array.
The compatibility notice records the 0.9.0 release floor.[^issue-923-config][^issue-923-compatibility]

[^issue-923-config]: [Normalized configuration types](../../../crates/hoimin-core/src/config.rs).
[^issue-923-compatibility]: [Required saved arrays compatibility notice](../../compatibility.md#required-saved-configuration-arrays-in-090).

# Single saved fingerprint input list (Issue #936)

Plan schema 8 retains only `normalized_config.fingerprint_inputs`. Both verify's
live-record comparison and review's protected-store identity checks use that
list. Mutation targets remain in `sources`; source/environment validation,
worker-copy filtering and the execution-time protected-input rechecks keep their
existing roles. The nested list still rejects missing/null values, duplicate
paths, malformed hashes and non-normalized paths.[^issue-936-plan][^issue-936-review-input]

Verify and review share schema-first decoding so obsolete fields do not hide
regeneration guidance. Schema-7 and older plans are rejected without conversion;
current-schema plans containing the obsolete top-level field are also rejected.
Typed decoding of the original JSON bytes rejects duplicate known fields rather
than silently retaining one value.
Regenerate using the intended version, environment and original plan arguments.
The release floor is 0.10.0 because the 0.9 series has already shipped. Existing
files are retained; report, session, ranking and fingerprint schemas and
fingerprint bytes are unchanged.[^issue-936-compatibility]

Parameterized reader tests cover old schemas and malformed nested lists. The
review-store tests protect a fingerprint-only file by direct path and hardlink,
assert the specific alias error and retain the input bytes. These are native
local test observations; they do not establish execution on every supported
platform.[^issue-936-tests]

[^issue-936-plan]: [Plan writer, shared decoder and verification](../../../crates/hoimin-cli/src/plan.rs).
[^issue-936-review-input]: [Review protected input identity checks](../../../crates/hoimin-cli/src/review/input.rs).
[^issue-936-tests]: [Reader and store protection tests](../../../crates/hoimin-cli/src/review/tests.rs).
[^issue-936-compatibility]: [Schema-8 regeneration notice](../../compatibility.md#single-saved-fingerprint-input-list-in-0100).

# One normalized ranked request (Issue #959)

`VerifySelection::Top { count, policy, offset }` is the sole ranked-request
variant. The parser supplies zero for an omitted offset; raw CLI input retains
`Option<usize>` and clap constraints so an explicitly supplied zero still
requires top. Rust prefix callers specify `offset: 0`; the former `TopRange`
variant and compatibility construction are removed. Plan resolution and preview
read the normalized offset directly.[^issue-959-cli][^issue-959-plan][^issue-959-preview]

This representation change retains full-policy ordering before paging, suffix
clipping, max-mutants rejection on actual selected length and diagnostic
precedence. On an empty plan, zero reports the empty-top error, while a positive
offset reports an invalid range first. Saved schemas and selection metadata
retain their formats. Tests compare omitted/zero requests and previews across
policies; the maintenance record distinguishes the new normalization test from
unchanged-behavior characterization and records executed checks and limits.
Existing paging and line-diverse Lean corpus comparisons are reused; no new
model or proof is claimed.[^issue-959-config-tests][^issue-959-tests][^issue-959-record]

[^issue-959-cli]: [Normalized CLI request](../../../crates/hoimin-cli/src/cli.rs).
[^issue-959-plan]: [Selection resolution](../../../crates/hoimin-cli/src/plan.rs).
[^issue-959-preview]: [Preview projection](../../../crates/hoimin-cli/src/plan/preview.rs).
[^issue-959-tests]: [Plan and preview tests](../../../crates/hoimin-cli/tests/plan.rs).
[^issue-959-record]: [Issue 959 implementation and verification](../../maintenance/2026-10-16-issue-959.md).

[^issue-959-config-tests]: [CLI normalization tests](../../../crates/hoimin-cli/tests/cli_config.rs).

# Discovery configuration ownership (Issue #979)

The composed raw CLI arguments share run/plan normalization without copying all
fields through another public structure. Native test argv remains platform data;
selector and path inputs still require UTF-8. Plan has no runtime session/resume
or metrics settings.[^issue-971-evidence]

`DiscoveryConfig` belongs only to `PlanConfig`, which requires the saved positive
`discovery.analyzer_timeout`. Direct run rejects the option and contains no
placeholder field. Verify uses the saved timeout for one deadline across all
targets; expiration prevents manifest publication or baseline execution.
Cancellation can return before a blocking worker exits; the worker retains its
resources until it stops. Synchronous reads and parsing are not forcibly
interruptible. Runtime total/baseline/mutant deadlines remain separate.
Issue #979 introduced plan 10/report 8/fingerprint 12 and floor 0.12.0.
Issue #984 kept these plan/report shapes and raised fingerprint to 13 and the
floor to 0.13.0 to exclude pre-restoration results from resume. SQLite session 5,
ranking 4 and candidate schema 1/ID v1 stay unchanged; current plans need no
regeneration for that execution fix. See the [discovery migration notice](../../compatibility.md#discovery-timeout-ownership-in-0120)
and [baseline restoration notice](../../compatibility.md#baseline-restoration-and-result-reuse-in-0130).[^issue-979-compatibility][^issue-979-evidence][^issue-984-compatibility]

Validation of independently invalid plan options can report discovery validation
before another invalid limit. Mandatory selector/argv diagnostics, individual
invalid-value diagnostics and discovery validation before project I/O remain
covered; mixed-invalid option priority is not a compatibility promise.
The recorded Task6 ruling explains that scope.[^issue-979-evidence]

[^issue-971-evidence]: [CLI composition and native argv checks](../../maintenance/2026-10-18-issues-971-979/task-5-report.md).
[^issue-979-compatibility]: [Current discovery ownership and migration](../../compatibility.md).
[^issue-979-evidence]: [Timeout ownership, controlled deadline checks and version evidence](../../maintenance/2026-10-18-issues-971-979/task-6-report.md).

# Regular-file plan inputs (Issue #985)

Verify admits its input through the shared regular-file opener, then reads the
validated descriptor. On Unix, nonblocking open plus descriptor metadata rejects
writerless FIFOs and symlinks to them before content parsing. Symlinks to regular
files remain accepted, and verify protects both the requested manifest spelling
and its canonical target. The change adds no review-style 64 MiB limit and does
not establish a deadline for all filesystem I/O.[^issue-985-admission]

Public CLI regressions cover direct and symlink FIFOs with an external deadline
and child reaping, missing paths, directories, regular-file parsing and large
inputs. The implementation record separates macOS execution from unexecuted
platform behavior.[^issue-985-admission]

[^issue-985-admission]: [Regular-file admission and verification record](../../maintenance/2026-10-18-issue-985.md).

# Current default operator selection (Issue #989)

One explicit selection defines the current 54 runtime defaults. Default
membership is independent of the complete operator catalog and selector-family
membership. The historical `all_legacy` helper is removed; omitted CLI selection
uses current defaults, while explicit selections remain exact after exclusions
and retain their saved membership during verification.[^issue-989-defaults][^issue-989-evidence]

Tests compare default names and serialized order with an independent fixed
literal list. Separate core and CLI cases check explicit subsets; analyzer
fixtures select the operators relevant to their own scenario. This removes a
historical policy from test setup without expanding defaults or saved subsets.
The record distinguishes behavior characterization from fixture compilation and
Clippy corrections.[^issue-989-evidence]

[^issue-989-defaults]: [Current operator-selection owner](../../../crates/hoimin-core/src/config.rs).
[^issue-989-evidence]: [Default-selection refactor and verification](../../maintenance/2026-10-19-issue-989-default-selection.md).


# Schema routing without a full JSON tree (Issue #988)

Review and verify share a schema probe that stores only the top-level version.
The normal schema-first encoding stops the probe before candidate bodies;
schema-last input skips preceding values without building a generic JSON tree.
Current plans are strictly decoded from the original bytes, so duplicate known
fields, unknown fields and trailing content remain rejected. The probe never
accepts a plan.[^issue-988-decoder]

Unsupported versions receive a structural JSON and end-of-input check before
regeneration guidance. That check skips scalar UTF-8, surrogate and numeric-range
semantics; those invalid scalars can therefore receive the version error instead
of a scalar error. Such inputs are still rejected. Mixed-invalid input and field
order can change which error is reported; there is no old-schema decoder or
conversion.[^issue-988-decoder][^issue-988-evidence]

Counted-reader properties check prefix scanning independently of full typed
validation. Allocation tests compare the actual decoder with direct typed
decoding for first/last schema positions and small/large candidate payloads.
Temporarily restoring the old JSON-tree allocation fails the same resource gate.
These checks measure scan work and requested live allocation, not process RSS,
allocator overhead or an I/O deadline.[^issue-988-evidence]

[^issue-988-decoder]: [Schema-routing owner](../../../crates/hoimin-cli/src/plan/schema.rs).
[^issue-988-evidence]: [Schema routing and allocation verification](../../maintenance/2026-10-19-issue-988-schema-probe.md).

[^issue-984-compatibility]: [Baseline restoration and current execution fingerprint](../../compatibility.md#baseline-restoration-and-result-reuse-in-0130).

[^issue-998-selection]: [crates/hoimin-core/src/machine/selection.rs](../../../crates/hoimin-core/src/machine/selection.rs).
[^issue-997-stop]: [crates/hoimin-core/src/machine.rs](../../../crates/hoimin-core/src/machine.rs).


# Capability-relative project source acquisition (Issue #1003)

Plan source hashing and requested-candidate byte validation share one lazy
PortableFileReader per operation. Root opening and file acquisition run on the
blocking pool. Hashing reuses WorkerRoot's streaming hash from the safely opened
descriptor; candidate validation keeps one file's bytes and context at a time.
Requested-ID diagnostic ordering, source record sorting, hashes and identities
remain unchanged. A missing source now uses the shared reader's not-found wording
while preserving the source path and PlanError category.[^issue-1003-source-acquisition]

Native macOS tests synchronize after target resolution and before acquisition,
then replace the ordinary source with a writerless FIFO, a symlink to a regular
file or a symlink to a FIFO. Both consumers reject those replacements; a watchdog
bounds and reaps a regressed child. Existing WorkerRoot parent-replacement tests
cover retained capability access.[^issue-1003-source-safety-tests]

A public plan-creation allocation test observes only the source stage, excluding
analyzer allocations. Its 1, 16 and 64 MiB sources stay below 256 KiB of additional
requested live heap; restoring whole-source reading exceeds that bound. This
measures requested live allocation, not RSS, allocator overhead, all plan memory
or an I/O deadline. Native Linux/Windows results are not established by these
macOS checks. A started blocking operation can continue after its caller is
cancelled.[^issue-1003-source-heap]

[^issue-1003-source-acquisition]: [Plan source acquisition](../../../crates/hoimin-cli/src/plan/source.rs).
[^issue-1003-source-safety-tests]: [Synchronized source and descriptor regressions](../../../crates/hoimin-cli/src/plan/validation_tests.rs).
[^issue-1003-source-heap]: [Production source-stage allocation gate](../../../crates/hoimin-cli/tests/plan_source_heap.rs).


# Human display boundaries (Issue #1002)

Preview projects each external string through `HumanText` at rendering time.
C0/C1 controls, including ESC, CR, LF and TAB, and Unicode line separators
U+2028/U+2029 become visible escapes. Ordinary Unicode remains readable, and each
candidate occupies one renderer-generated line. Quoted mutation text uses the
same policy with quote/backslash escaping; logical paths, candidate IDs and
JSON/JSONL strings remain unchanged.[^issue-1002-human-text][^issue-1002-preview]

Native macOS public-binary tests create actual ESC-, CR-, LF- and TAB-bearing
filenames and a Japanese filename. They exercise all preview formats, compare
logical path/ID values, and check the command marker remains absent and the plan
unchanged. These checks do not establish native Linux/Windows behavior or permit
NUL in a native filename. The issue concerns terminal presentation; it does not
establish arbitrary code execution.[^issue-1002-preview-tests]

[^issue-1002-human-text]: [Shared human text projection](../../../crates/hoimin-cli/src/human_text.rs).
[^issue-1002-preview]: [Preview rendering boundary](../../../crates/hoimin-cli/src/plan/preview.rs).
[^issue-1002-preview-tests]: [Public preview and diagnostic regressions](../../../crates/hoimin-cli/tests/plan.rs).


# Analyzer membership after target selection (Issue #1006)

Per-file analyzer admission prepares the raw range union once and reuses it
across token, AST and annotation producers. Empty input means all lines;
nonempty reversed-only input means none. Prefix-maximum inclusive endpoints
preserve arbitrary unsorted/overlapping/nested ranges without requiring caller
normalization. Symbol admission remains a conjunction. Public `--line` and
noncontiguous `--changed` coverage compares candidate objects, IDs, ordering and
truncation; this internal index changes no saved format or ranking rule.
See [analyzer ownership and limits](analyzer.md#prepared-admission-and-deferred-positions-issues-1006-and-1008)
and the [fixed-workload evidence](../../maintenance/2026-10-20-issue-1006/verification.json).
[^perf-three-selection]

[^perf-three-selection]: [Prepared raw-range union](../../../crates/hoimin-python/src/analyzer/rust/selection.rs).

# Target Git environment policy (Issue #1026)

Every target-inspection Git command removes `GIT_DIFF_OPTS` through the shared
command builder. Hoimin's requested changed context therefore controls both
ordinary scoped patches and global rename pairing. The parent environment and
baseline/mutant command environments retain the variable; literal pathspec and
existing diff semantics remain in place.[^issue-1026-git]

Public plan regressions compare candidates, IDs, ordering, ranking and saved
configuration for omitted, zero and nonzero context across absent, zero and
nonzero ambient Git settings. A runtime regression checks that test commands
still receive the ambient setting. The [Git review record](../../maintenance/2026-10-20-issues-1026-1034/git.md)
records red/green evidence and platform limits. Recheck this policy when adding
a target Git command or changing rename/context handling.[^issue-1026-environment-tests]

The owned runner's cancellation and runtime-lifetime requirements are described
in the [runtime contract](runtime-lifecycle.md#owned-target-git-commands-issue-1032).

[^issue-1026-git]: [Shared target Git command policy](../../../crates/hoimin-cli/src/target/git.rs).
[^issue-1026-environment-tests]: [Public environment and context regressions](../../../crates/hoimin-cli/tests/changed_git_environment.rs).

# Independent worker and process configuration (Issue #1033)

Raw run/plan input and normalized configuration accept `jobs > max_processes`,
including `--jobs 4 --max-processes 1`. Individual checks remain: positive
values, at most 256 workers, and a process allowance representable by `u32`.
Checked construction, saved `RunConfig` decoding and plan-to-run conversion
apply these constraints without a cross-field ordering rule.[^issue-1033-config]

A plan/verify integration regression preserves the 4/1 settings and verifies
that preparation has not executed the test command. Saved formats and defaults
do not change. Runtime enforcement remains per root tree; see
[resource scope and native test conditions](runtime-lifecycle.md#independent-worker-and-root-process-limits-issue-1033).
[^issue-1033-verify-test]

[^issue-1033-config]: [Raw and normalized configuration validation](../../../crates/hoimin-core/src/config.rs).
[^issue-1033-verify-test]: [Saved-plan verification regression](../../../crates/hoimin-cli/tests/plan.rs).


# Preview ownership (Issue #1041)

`prepare_verify_selection` returns a `VerifiedPlan` containing execution settings,
selected IDs and protected input paths. It retains no preview rows or candidate
original/replacement text. `prepare_verify_preview_selection` builds a
`VerifyPreview` after the same manifest, source, fingerprint, ranking and selected
candidate checks. The CLI chooses this entry point only for dry runs; saved and
preview schemas are unchanged.[^six-perf-verify]

A real public-preparation allocator gate independently varies candidate text and
selected count after runtime warmup. It bounds the retained allocation delta,
not peak validation heap or process RSS. Validation still parses the manifest
and source, and dry runs still own their selected display payload.[^six-perf-verify-heap]

[^six-perf-verify]: [plan.rs](../../../crates/hoimin-cli/src/plan.rs).

[^six-perf-verify-heap]: [manifest_buffer_lifetime.rs](../../../crates/hoimin-cli/tests/manifest_buffer_lifetime.rs).

The [implementation and measurement record](../../maintenance/2026-10-21-issues-1039-1040-1041-1042-1044-1045/README.md) separates operation gates,
retained heap, guarded elapsed observations and sampled RSS limits.[^six-perf-record]

[^six-perf-record]: [Six-issue implementation, measurements and validation](../../maintenance/2026-10-21-issues-1039-1040-1041-1042-1044-1045/README.md).
