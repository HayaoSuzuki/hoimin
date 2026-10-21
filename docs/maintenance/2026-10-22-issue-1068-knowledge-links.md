# Knowledge reference repair (Issue #1068)

Checked on 2026-10-22 against baseline `d1bf54987cb196b2bcb497fffe763ab06420fd66`. The original 52-page inventory had 27 missing Markdown targets (7 pages, 12 paths) and 24 missing relative source entries (7 pages); the union covered 8 pages. These counts overlap.

Current navigation now follows `hoimin-python` for pure analysis; the CLI handler and spool store remain CLI owners. Historical sources retain their original `revision`, `working_tree` and `sha256`. `original_resource` records the removed path; `recovered_revision`/`recovered_blob` identify independently recovered bytes. A comparison commit is never presented as an uncommitted snapshot.

## Recovery inventory

For each SHA-256 source, the recorded revision and every distinct blob at the original repository path in `git log --all --format=%H -- PATH` were checked until a match was found. Hashless committed sources were read at their recorded revision. Of 24 entries, 22 are recovered (20 hash matches and 2 hashless revision matches); two snapshots remain unavailable. Duplicate source entries count separately. Git history here means locally reachable commits, not remote or dangling objects.

| Concept / source ID | Original path | Recovered commit / blob, or search limit |
| --- | --- | --- |
| `docs/knowledge/audits/evaluation-order-2026-09.md` / `issue-561-tests` | `crates/hoimin-cli/src/analyzer/rust/method_replacement_tests.rs` | `d2b8e83f054c13eacfceac84e37803fbe71512de` / `4035c7a9ef65f12f8f732dd196880a86db6591d3` |
| `docs/knowledge/audits/implicit-finally.md` / `implementation` | `crates/hoimin-cli/src/analyzer/rust.rs` | `e69fb312ffc91fff88c0d97b5bf0fc7df9151af7` / `9632ea3a1ef94abeb83dc812f8b88d8e4eab559e` |
| `docs/knowledge/audits/nullable-gates-2026-09.md` / `issue-565-implementation` | `crates/hoimin-cli/src/analyzer/rust.rs` | `75c1ddfd59a1c2ebf1bc9de05efb33c571c7c328` / `cd357cb58423ac7118156e30ed1c3c21859bbb46` |
| `docs/knowledge/audits/with-finally-2026-09.md` / `issue-557-tests` | `crates/hoimin-cli/src/analyzer/rust/finally_transfer_tests.rs` | `f071781e20bc87c13b537ecd620e5ed06e9cd1f6` / `f8e15ee81a7800ca2335c532ed3fb3f11db03535` |
| `docs/knowledge/audits/with-finally-2026-09.md` / `issue-556-implementation` | `crates/hoimin-cli/src/analyzer/rust.rs` | Unavailable; 126 distinct historical blobs searched |
| `docs/knowledge/audits/with-finally-2026-09.md` / `implementation` | `crates/hoimin-cli/src/analyzer/rust.rs` | `5e631efc46a6e8c0b9fcf2d7a74536e57f79a369` / `939402db0c261de0cccc67863f674c9808a741a6` |
| `docs/knowledge/design/analyzer.md` / `issue-935-bindings` | `crates/hoimin-cli/src/analyzer/rust.rs` | `341e153c093e2677b443d3ef85bdf29fc618ebdf` / `479adf70e261535378115c7447f15ed9bc5e036e` |
| `docs/knowledge/design/analyzer.md` / `issue-935-tests` | `crates/hoimin-cli/src/analyzer/known_imports_tests.rs` | `341e153c093e2677b443d3ef85bdf29fc618ebdf` / `48eef2b970f162d942d3ec0ae828834f09c32066` |
| `docs/knowledge/design/analyzer.md` / `issue-929-live-codes` | `crates/hoimin-cli/src/analyzer/types.rs` | `54a72e541a80db4a9df3225e728345146f636180` / `7bb09dc798f41391aef4007368035c85ee731456` |
| `docs/knowledge/design/analyzer.md` / `issue-914-class-context` | `crates/hoimin-cli/src/analyzer/rust.rs` | `75df00cabcebed7c839d6d77fa1b8ce96b9a858f` / `7da5342a7c9d53dcc4a69c6b545b8af9797965d4` |
| `docs/knowledge/design/analyzer.md` / `issue-871-acquisition` | `crates/hoimin-cli/src/analyzer/rust/depth.rs` | Unavailable; 8 distinct historical blobs searched |
| `docs/knowledge/design/analyzer.md` / `string-segment-implementation` | `crates/hoimin-cli/src/analyzer/rust/string_segments.rs` | `a6965910bbc745e74aeb539628b3e2dc66c2a9d3` / `222b44d69d371e9a438b9108cc8b337a27249c38` |
| `docs/knowledge/design/analyzer.md` / `string-segment-exclusions` | `crates/hoimin-cli/src/analyzer/rust/string_literals.rs` | `a6965910bbc745e74aeb539628b3e2dc66c2a9d3` / `9f442e22a9f93a6b7f7c6be4319c94cac9e23352` |
| `docs/knowledge/design/analyzer.md` / `issue-560-code` | `crates/hoimin-cli/src/analyzer/rust.rs` | `b91d3659215a150a6c8c24f0233f4c3880b52e28` / `9f6c00fb1c1a342046a7269798b31a1a23b29d1f` |
| `docs/knowledge/design/analyzer.md` / `issue-564-code` | `crates/hoimin-cli/src/analyzer/rust.rs` | `f6f5d96c099fb880884b2b7cb29717ff33d70d75` / `c1aae5af5bde2e4ee38061fc675b7dc38ff97dde` |
| `docs/knowledge/design/analyzer.md` / `implementation` | `crates/hoimin-cli/src/analyzer/rust.rs` | `a7daea0b557cd435c1e55b540392fbdd116348e1` / `664a96c54d8060dc78a7ae1f95a3dbba5d448f3b` |
| `docs/knowledge/design/analyzer.md` / `issue-930-token-operators` | `crates/hoimin-cli/src/analyzer/rust.rs` | `59fff0890b78e0169b0318b2a8fe637dd0056334` / `2f893f051b10ef4bc2ee544864e67373dbc67b98` |
| `docs/knowledge/design/analyzer.md` / `issue-950-candidate` | `crates/hoimin-cli/src/analyzer/types.rs` | `59fff0890b78e0169b0318b2a8fe637dd0056334` / `b553107f55877be0d240b9a50e8d01a536b33ceb` |
| `docs/knowledge/design/analyzer.md` / `issue-950-retention` | `crates/hoimin-cli/src/analyzer/rust.rs` | `59fff0890b78e0169b0318b2a8fe637dd0056334` / `2f893f051b10ef4bc2ee544864e67373dbc67b98` |
| `docs/knowledge/design/analyzer.md` / `issue-950-tests` | `crates/hoimin-cli/src/analyzer/rust_tests.rs` | `59fff0890b78e0169b0318b2a8fe637dd0056334` / `f0c52ed905f8e6cedc9968253adb57fff4cf985d` |
| `docs/knowledge/design/analyzer.md` / `issue-978-payload` | `crates/hoimin-cli/src/analyzer/payload.rs` | `4ebed4457519920e9a28f8f653d8ce96985acdad` / `571abd603802a87be2909407d7f5e245409d2f9f` |
| `docs/knowledge/design/analyzer.md` / `issue-986-walker` | `crates/hoimin-cli/src/analyzer/visitor.rs` | `2ec902ae2353c38f2da1a74cbf6552785387e42d` / `a81bd3c074217587c6c2578f344dbd8c8b15854d` |
| `docs/knowledge/design/annotation-builtins.md` / `issue-548-implementation` | `crates/hoimin-cli/src/analyzer/rust.rs` | `10be4aef140e19bb906bdbb834ca025ac7c3aee9` / `939402db0c261de0cccc67863f674c9808a741a6` |
| `docs/knowledge/design/selection-plan-verify.md` / `issue-476-analyzer` | `crates/hoimin-cli/src/analyzer/rust.rs` | `9d9e79247ac4f9146c623102c7e5503bcd1e94ab` / `92b156d3861c23a8fcfd6aad85cec63ec40a36e1` |

## issue-556-implementation

The recorded `modified` bytes for `crates/hoimin-cli/src/analyzer/rust.rs` with SHA-256 `17c79e4a1cb5a22cd1deeec35ede2988288ca9145245c874bee23229e72a6af8` were not found among 126 distinct historical blobs at that path. The comparison revision `282e941e4c1a5a23303d30beca881d7bbfde7763` does not reconstruct this evidence. The original metadata remains in `docs/knowledge/audits/with-finally-2026-09.md`. Current implementation navigation is provided there, but it does not establish the historical claims. Recovery would require the original working-tree snapshot or a blob with the recorded hash.

## issue-871-acquisition

The recorded `modified` bytes for `crates/hoimin-cli/src/analyzer/rust/depth.rs` with SHA-256 `bda56c389adde16eecbfdf6ce13726a1138f2a117442cc82ed3c541aa3eeab3c` were not found among 8 distinct historical blobs at that path. The comparison revision `49a34a63ac4e995bfefb156f85ff3a934b594e6b` does not reconstruct this evidence. The original metadata remains in `docs/knowledge/design/analyzer.md`. Current implementation navigation is provided there, but it does not establish the historical claims. Recovery would require the original working-tree snapshot or a blob with the recorded hash.

## Recheck

Run `python3 tools/knowledge_references.py` from the repository root. Its JSON inventory includes every parsed Markdown target and source resource, missing local targets, and pages unreachable from `docs/knowledge/index.md`. Save output when comparing a file move; check both reference kinds and inspect ownership descriptions. Run `.venv/bin/pytest tests/test_knowledge_references.py` for controlled fixtures. Follow the structure/content checks in [OKF workflow](../okf-workflow.md).

For a recovered source, run `git show RECOVERED_REVISION:ORIGINAL_REPOSITORY_PATH | shasum -a 256` and compare to the retained `sha256`; `git rev-parse RECOVERED_REVISION:ORIGINAL_REPOSITORY_PATH` must equal `recovered_blob`. For unavailable sources, search distinct original-path blobs as above and compare their SHA-256, keeping the limitation until an exact match is found.

This is a local path, reachability and Git-byte check. It does not authenticate external URLs, validate every heading anchor, rerun historical tests, or reverify all prose claims. No build, Cargo test or benchmark was run for this documentation repair.
