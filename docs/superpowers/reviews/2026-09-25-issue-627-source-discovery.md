# Issue 627 review and verification

Design and plan were committed before implementation as `c8ea85e`, with three
review passes each. Production changes are confined to target discovery scope;
the core resolver and original ordered Selection are unchanged.

## Implementation reviews

1. Traced source unions, symbol module lookup, and exact/line errors through the
   core resolver. Indexed sources only prune discovery; original ordering remains
   authoritative for symbols. Root-equal sources and invalid normalization retain
   broad discovery. Exact requested paths outside sources remain discoverable so
   the existing FileOutsideSource error still comes from resolution.
2. Reviewed component boundaries, native filenames, and both walk passes. String
   ancestor lookup cannot confuse alpha with alpha_extra. Malformed selected
   descendants use native ancestors and the existing platform equality key, so
   selected errors remain visible while unrelated malformed entries are pruned.
   Include restoration uses the same scope without changing its root or overrides.
3. Reviewed performance against the old exact-selector path. Added an empty-source
   fast path to avoid unnecessary ancestor scanning for exact-only requests.
   Source lookups check component prefixes against an index rather than scanning
   all configured sources. Root-level siblings still require enumeration; no
   global constant-time claim is made.

## Test reviews

1. The RED/GREEN gate counts visits and collected records on the real production
   walker. Fixed selected contents include a Python and non-Python file. A broad
   walk over the same fixture is the sensitivity control. Added a separate ignored
   include-restoration gate so testing only the normal walk cannot hide regressions.
2. Compared scoped results with broad-discovery inputs to the unchanged resolver,
   and asserted literal selected paths and symbol precedence independently. Fixed
   a test helper that initially converted resolution errors into empty results;
   successful cases now unwrap success and outside-source cases assert the error.
   Public plan compares complete candidate arrays across source/exact selectors
   and all unrelated-file scales, and asserts the test command never ran.
3. Audited path/platform premises. Cover overlapping/normalized/root/file sources,
   source prefixes, ignored restores, exclusions, built-in protected directories,
   hidden files, case rules, selected/unselected malformed names, and symlinks.
   APFS rejected creation of a non-UTF-8 fixture before discovery; that native
   case runs on other Unix platforms, while macOS runs the literal-backslash
   boundary. This is documented platform scope, not a claim of macOS coverage for
   a filename its filesystem cannot create.

## Measured traversal

| Unrelated files | Scoped visits | Scoped records | Broad visits | Broad records |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 4 | 2 | 4 | 2 |
| 1,000 | 4 | 2 | 1,004 | 1,002 |
| 5,000 | 4 | 2 | 5,004 | 5,002 |

Before the change, the 1,000-file case retained 1,002 records and failed the gate.
Afterward all sizes pass; the broad control still grows as shown. The separate
include-restoration fixture selects one file despite 1,000 restored unrelated
files, with fewer than 12 visits. These observations concern entry visits and
retained records, not heap bytes, RSS, syscall count, or a timing guarantee.

## Validation and independent review

Focused discovery tests: 5 passed, 1 ignored. Public discovery correspondence:
5 passed. Public plan candidate equality: 1 passed. Both exact CI Clippy commands
and formatting checks passed. Full workspace: 2,427 passed, 0 failed, 22 ignored
across 113 test/doc-test result groups.

Root independently reviewed production/design and found no blockers in subtree
and ancestor membership, normalized-root fallback, preserved symbol source order,
or native selected-descendant diagnostics. The requested source-file, prefix,
overlap/normalization, ignore/include, outside-file, malformed-name, and complete
public candidate-array cases are covered as described above.

Rebased the unpublished branch onto main `2498cc1` after the full suite. Both
commits had unchanged range-diff entries, with no conflicts. Cleaned local crate
artifacts before repeating discovery unit tests, the full plan and public
source-discovery integration binaries, both exact CI Clippy commands, and both
format checks; all passed. The full-workspace count above predates this rebase.

## Published-branch integration

Merged main `9f112ba` after Issues 619, 628, and 629 landed. The only conflict
was adjacent additions in docs/development.md; retained both the source-discovery
contract and environment-fingerprint documentation. Reviewed the automatic plan
test merge: its difference from main is still exactly the Issue 627 test. The
discovery implementation and correspondence tests are unchanged from the reviewed
published branch; no new production behavior was introduced by resolution.

After cleaning local crate artifacts, discovery unit tests and the source_discovery,
plan, missing_source, target_handler, and fingerprint_env_plan integration binaries
passed: 154 passed, 0 failed, 2 ignored. Both exact CI Clippy commands, both format
checks, and git diff --check passed. These checks cover discovery composition with
the newly merged missing-source diagnostic, environment snapshot verification,
and streaming workspace implementation. The earlier full-workspace result remains
the full-suite evidence; this integration used focused tests and static checks.
