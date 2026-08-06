# Collection and Structural Mutation Operators Design

Date: 2026-08-06  
Status: design approved for written-spec review

## Context

The Python analyzer currently emits token-level replacements for comparisons,
boolean operators, arithmetic operators, unary signs, literals, control-flow
keywords, and a separate set of type-annotation replacements. Several
high-value Python defects are expressed as collection API choices or literal
shape choices instead of a single token. Examples include using `any` where
`all` was intended, mutating a list at the wrong end, and using a list where a
tuple was required.

This feature adds those candidates to the normal runtime operator set. Every
new operator is enabled by default, has a stable operator ID, can be excluded
individually, and is grouped into selectors for users who need a narrower run.

## Goals

- Detect the approved collection, string, aggregate, bitwise, structural, and
  boundary mutations in Python source.
- Keep every emitted replacement syntactically valid for the supported source
  shape and preserve source order, line/symbol selection, profiles, and
  candidate deduplication.
- Avoid mutating calls to shadowed builtins. Conservative false negatives are
  preferable to changing the meaning of an unrelated local function.
- Make all approved operators part of the default runtime selection while
  retaining per-operator and family exclusions.
- Document the new IDs, default behavior, supported shapes, and known semantic
  differences in the README and development documentation.

## Non-goals

- Type inference for arbitrary user-defined collection classes.
- Proving that a method receiver is a concrete `list`, `set`, `dict`, or `str`.
- `append`/`pop` in this release. Their argument and return-value contracts are
  not symmetric; a later operator can target a narrower statement-only form.
- Set literals to `frozenset` literals. Python has no `frozenset` literal, and
  wrapping a set literal in a constructor is a different structural operation.
- Comprehension rewrites, assignment-target rewrites, or mutations that require
  evaluating an expression more than once.

## Operator inventory

The following IDs are added to `MutationOperator::all()` and the default
runtime selection. The exact candidate shapes below are part of the contract.

| ID | Default | Supported replacement |
| --- | --- | --- |
| `collection_any_all` | yes | `any(x)` ↔ `all(x)` |
| `collection_list_tuple` | yes | `list(x)` ↔ `tuple(x)` and list/tuple literals |
| `collection_set_frozenset` | yes | `set(x)` ↔ `frozenset(x)` |
| `collection_append_insert` | yes | `seq.append(x)` ↔ `seq.insert(0, x)` |
| `collection_min_max` | yes | `min(...)` ↔ `max(...)` |
| `collection_set_add_discard` | yes | `seq.add(x)` ↔ `seq.discard(x)` |
| `collection_set_remove_discard` | yes | `seq.remove(x)` ↔ `seq.discard(x)` |
| `collection_string_starts_ends` | yes | `.startswith(...)` ↔ `.endswith(...)` |
| `collection_string_split_rsplit` | yes | `.split(...)` ↔ `.rsplit(...)` |
| `bitwise_and_or` | yes | `&` ↔ `\|` |
| `bitwise_shift` | yes | `<<` ↔ `>>` |
| `structure_append_extend` | yes | `seq.append(x)` ↔ `seq.extend([x])` |
| `structure_mapping_get_subscript` | yes | `mapping.get(k)` ↔ `mapping[k]` |
| `structure_sort_reverse` | yes | `seq.sort()` ↔ `seq.reverse()` |
| `structure_sorted_reversed` | yes | `sorted(x)` ↔ `reversed(x)` |
| `structure_index_neighbor` | yes | integer literal indices move by one |
| `structure_slice_neighbor` | yes | integer literal slice bounds move by one |

The names are intentionally independent even when two operators share a
method. For example, users can exclude `structure_append_extend` while
retaining `collection_append_insert`.

## Candidate semantics

### Calls with matching contracts

The following replacements only require changing a callable or method name and
therefore preserve the argument list. Calls with unsupported keyword or star
argument forms are skipped where the two APIs do not have identical contracts.

- `any` and `all` require exactly one positional argument and no keywords.
- `list`, `tuple`, `set`, and `frozenset` accept zero or one positional
  argument and no keywords.
- `min` and `max` have the same Python call contract; supported positional and
  keyword arguments are preserved.
- `startswith` and `endswith` preserve their positional or keyword arguments.
- `split` and `rsplit` preserve their positional or keyword arguments.
- `sorted` and `reversed` are restricted to exactly one positional argument
  and no keywords because their broader signatures differ.

Bare builtin calls are emitted only when the target name is not conservatively
shadowed by a module binding, import, function/class name, parameter, or
assignment in the source file. Explicit qualified builtin calls are outside
this first version.

### List and tuple calls and literals

`collection_list_tuple` covers both constructor calls and load-context literal
expressions:

- `list(value)` ↔ `tuple(value)` and `list()` ↔ `tuple()`.
- `[a, b]` ↔ `(a, b)`.
- `[a]` ↔ `(a,)`; the required singleton comma is generated.
- `[]` ↔ `()`.
- `(a,)` ↔ `[a,]`; a trailing comma is valid in a list literal.
- Bare comma tuples such as `a, b` are supported when the AST range is an
  expression, while call argument lists are not treated as tuple literals.

List and tuple comprehensions are excluded. List/tuple expressions in store or
delete context, including unpacking assignment targets, are excluded. Source
text inside the literal is preserved; only delimiters and the singleton comma
are changed.

### Collection method replacements

- `collection_append_insert` accepts exactly `append(value)` and
  `insert(0, value)` with positional arguments and no star expansion. The
  entire call expression is replaced so the argument shape remains valid.
- `structure_append_extend` accepts `append(value)` and the inverse form only
  for `extend([value])` with a singleton list literal. This prevents a
  multi-element extension from being silently reduced to one append.
- `collection_set_add_discard` and `collection_set_remove_discard` accept one
  positional argument and no keywords. The differing membership and
  exception behavior is intentional mutation semantics.
- `structure_sort_reverse` accepts zero-argument calls only. `sort` calls with
  `key` or `reverse` options are excluded.

Method candidates use the receiver and method name from the AST but do not
attempt type inference. This matches the analyzer's existing syntax-directed
operator model; a custom object with a matching method may therefore receive a
valid but intentionally disruptive mutant.

### Mapping access

`structure_mapping_get_subscript` accepts load-context expressions with a
simple receiver (`Name` or `Attribute`) and exactly one key expression:

- `mapping.get(key)` ↔ `mapping[key]`.
- `.get` calls with a default value, keywords, or star expansion are skipped.
- Slices, assignment targets, and delete targets are skipped.

The transformation intentionally exercises the difference between a default
return and `KeyError`; it does not claim that every receiver is a mapping.

### Bitwise and boundary mutations

- `bitwise_and_or` and `bitwise_shift` are token replacements and follow the
  existing source-order, unary/context, and selection rules.
- `structure_index_neighbor` targets load-context subscripts whose index is a
  plain decimal integer literal. It emits a `+1` candidate and, for positive
  values, a `-1` candidate. Negative literals, expressions, and slices are
  excluded so no index expression is evaluated twice.
- `structure_slice_neighbor` targets plain decimal integer literals in slice
  start, stop, or step positions. It emits adjacent values when the result is
  syntactically valid; a step mutation that would produce zero is skipped.
  Empty bounds, non-decimal literals, and arbitrary expressions are excluded.

## Analyzer architecture

Token scanning remains responsible for the existing single-token operators.
The Python AST visitor gains a second candidate pass for calls, literals,
subscripts, and slices. Both passes produce the same `AnalyzerCandidate`
shape, then share the existing selection, focused-profile filtering,
deduplication, source ordering, and `max_candidates` truncation.

Structural replacements use one contiguous AST range. A small source-rewrite
helper builds the replacement from the original receiver/argument text so
comments, string spelling, and nested expressions remain unchanged. Every
replacement is parsed in tests before it is accepted as a candidate.

The visitor records conservative file-level bindings for the builtin names
used by the call operators. A binding anywhere in the source file suppresses
the corresponding bare-builtin candidates. This deliberately sacrifices some
coverage to avoid mutating a shadowed callable.

## Configuration and compatibility

- Add all IDs to `MutationOperator::all()` and the default runtime selection.
- Add selector families `collection_ops`, `structure_ops`, and `bitwise_ops`.
- `--operators` and `--exclude-operators` continue to accept individual IDs
  and families; default runs include every new ID.
- Persisted plan/session configuration uses the existing operator-name strings.
  A changed default operator set changes the configuration fingerprint, so an
  old session is not silently mixed with a run using the expanded default.
- The analyzer protocol already carries operator names as strings; protocol
  validation obtains the new names from `MutationOperator::from_name`.

## Testing strategy

1. **Configuration tests**: default selection, valid names, family expansion,
   individual exclusion, persisted selection, and compatibility fingerprints.
2. **Analyzer tests**: exact candidates and replacements for every operator,
   source order, line/symbol filters, duplicate suppression, focused profile,
   shadowed builtins, unsupported argument forms, comprehensions, assignment
   targets, and boundary overflow/zero-step cases.
3. **Parse-preservation tests**: apply every emitted replacement to its source
   and reparse the result. Include empty/singleton/multi-element literals,
   trailing commas, starred elements, nested calls, and comments.
4. **CLI integration tests**: run a small fixture through representative
   collection and structural mutants, verify the operator IDs in JSON output,
   and verify default runs include the new candidates.
5. **Documentation contracts**: README operator tables and development
   guidance enumerate all IDs, families, default behavior, unsupported shapes,
   and how to exclude high-impact families.

## Documentation and delivery

The specification, implementation plan, README/development updates, source,
and tests are committed in the same feature worktree and delivered in one PR.
Because this PR changes runtime behavior and default candidate counts, it is
not a documentation-only change and must run the normal CI matrix.

## Risks and mitigations

- **Candidate volume**: default inclusion increases candidates. Existing
  `max_mutants` and `max_candidates` remain the hard bounds; family selectors
  and individual exclusions provide escape hatches.
- **Semantic false positives**: conservative builtin shadow detection and
  strict argument/context filters prevent invalid or obviously unrelated
  rewrites.
- **Custom methods**: method replacements are syntax-directed and documented
  as such; tests verify parseability rather than pretending to infer types.
- **Source formatting**: replacements are range-local and parse-checked,
  avoiding a whole-file formatter or AST reserializer.
