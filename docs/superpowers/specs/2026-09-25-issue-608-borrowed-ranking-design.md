# Issue 608: borrowed ranking revalidation

Ranking revalidation clones every retained mutation candidate, including original/replacement text, before sorting an expected list. Replace this with validation over borrowed candidates while retaining all-candidate checks and the existing generic semantic-ranking error.

Extract the existing line index, resolved-symbol index and changed-selection flag into a shared ranking context. Its per-candidate method computes the same ordered reason vector from a borrowed candidate. Plan creation continues to own and sort candidates through the existing comparator. Revalidation computes each candidate's expected reasons and score, checks its one-based position rank, and checks adjacent candidates with the same comparator. The comparator is unchanged: descending score, then path, line, column, operator and ID. No candidate text is copied.

This checks the same fixed point as re-ranking: every candidate has its derived metadata, ranks match positions, and the list is sorted. The existing stable sort preserves the input order of equal comparator keys; adjacent validation accepts those ties as before, including otherwise different candidate bodies. Empty slices remain valid. Do not replace semantic validation with the structural validator: the latter has distinct errors and extra conditions, while callers already run it separately. No manifest/ranking version, selection policy, validation ordering or public API change is needed.

Alternatives considered: sorting borrowed ranked wrappers would remove text clones but retain an unnecessary full expected vector and sort; cloning one candidate at a time still scales temporary heap with its body. Shared derived metadata plus an ordered pass is simpler and removes both costs. Ordinary equivalence and tampering tests cover this finite synchronous comparison; no state-machine model or Lean run is needed.

## Design self-review

1. Traced plan creation and shared normal/preview preparation. Revalidation still visits all retained entries before selected candidate analysis; no top-count shortcut is introduced. Shared context preserves explicit-line normalization, dot-boundary symbol matching and reason order.
2. Compared full stable re-ranking equality with metadata-plus-adjacency checks. Equal-key stability and unknown-operator behavior at this private function boundary must remain unchanged; reusing structural validation would alter that contract, so it stays separate.
3. Reviewed memory claims: context size depends on selectors and one small reason vector on reason count. Candidate original/replacement fields remain borrowed. Regression measurement must isolate the production validation call, keep 64 valid candidates fixed, vary body sizes, and avoid claiming whole-process RSS or timing improvements.
