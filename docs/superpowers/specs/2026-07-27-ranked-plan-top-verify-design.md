# Ranked Plans and Top-N Verification Design

## Purpose

Make hoimin's plan-and-verify workflow useful without requiring users or agents to
manually extract candidate IDs. `plan` will rank discovered mutation candidates,
record the deterministic ranking and its reasons in the plan manifest, and `verify`
will accept `--top N` to execute the highest-ranked retained candidates.

This feature improves two user-facing properties together:

- prioritize candidates that are more closely tied to explicit or changed targets
  and are more likely to affect behavior; and
- reduce the manual work between `plan` and `verify`.

The ranking is an execution-order heuristic, not a probability estimate or a claim
that lower-ranked mutants are unimportant.

## User Experience

The intended workflow is:

```console
hoimin plan --root . --source src --changed \
  -- python -m pytest -q > PLAN.json
hoimin verify PLAN.json --top 10 --format json
```

`verify` retains exact candidate selection:

```console
hoimin verify PLAN.json \
  --candidate ID1 \
  --candidate ID2 \
  --format json
```

Selection is always explicit. A verify invocation must provide either one or more
`--candidate` options or exactly one `--top N` option.

### CLI rules

- `--top N` requires `N >= 1`.
- `--top` and `--candidate` are mutually exclusive.
- Omitting both remains an error.
- When `N` exceeds the number of retained candidates, verify executes every retained
  candidate.
- Top-N candidates execute in ascending manifest rank.
- Verify never recomputes ranking from the current source tree or Git state.

## Ranking Model

Ranking is computed after discovery and before the plan manifest is rendered.
It uses only normalized plan selectors, the discovered candidate, and changed-line
information already produced for `--changed`. It does not implicitly query Git when
`--changed` is absent.

Each candidate receives additive reasons:

| Reason code | Condition | Score |
| --- | --- | ---: |
| `explicit_line` | Candidate is within an explicit `--line` range | 300 |
| `explicit_symbol` | Candidate is within an explicit `--symbol` target | 250 |
| `changed_line` | Candidate is selected from a changed line | 200 |
| `high_value_control` | Comparison, membership, identity, boolean, unary `not`, boolean literal, or break/continue operator | 100 |
| `arithmetic` | Binary, augmented, or unary arithmetic operator | 70 |
| `type_annotation` | Type-annotation operator | 50 |

Selectors form the same union or changed-line intersection that discovery already
defines. All applicable ranking reasons accumulate; ranking does not change which
candidates are retained.

Candidates are sorted by:

1. descending score;
2. root-relative path;
3. source line;
4. source column;
5. operator ID; and
6. candidate ID.

Ranks are one-based, unique, contiguous integers in sorted order. This tie-breaking
chain makes identical plan inputs produce identical rankings.

## Plan Manifest Contract

The plan manifest schema version will be incremented. Every candidate in the new
version contains:

```json
{
  "id": "candidate-id",
  "rank": 1,
  "score": 300,
  "ranking_reasons": [
    {
      "code": "explicit_symbol",
      "score": 250
    },
    {
      "code": "type_annotation",
      "score": 50
    }
  ]
}
```

Reason order is deterministic: selector reasons first in descending selector score,
then the operator-category reason. `score` must equal the sum of the reason scores.

New-version manifests are rejected before baseline execution when:

- ranks are missing, duplicated, zero, or non-contiguous;
- candidate array order disagrees with ascending rank;
- a reason has an unknown code or the wrong fixed score;
- reasons are duplicated or out of deterministic order; or
- the recorded score does not equal the reason sum.

The existing source, normalized-configuration, selection-root, candidate-integrity,
and fingerprint validation remains in force.

### Legacy manifests

Legacy plan manifests remain usable with explicit `--candidate` selection. They do
not contain a trustworthy ranking, so `--top` rejects them before baseline execution
with a message that instructs the user to regenerate the plan or use `--candidate`.
Legacy candidates are not assigned an implicit array-order ranking.

## Truncated Plans

A truncated plan contains only the retained prefix produced before the discovery
limit. `verify --top N` is allowed against that manifest, but it means “top N among
the retained candidates,” not “top N among every candidate that might exist.”

Human output and structured verify metadata will preserve the plan's truncated state
and identify the selection scope as `retained_candidates`. Documentation must not
describe a truncated top-N run as complete coverage or a global top-N selection.

## Internal Boundaries and Data Flow

The analyzer continues to discover candidates without ranking them. A separate,
pure ranking component accepts normalized selection context plus candidates and
returns ranked plan candidates.

```text
normalize selectors
  -> discover candidates
  -> build ranking context
  -> compute candidate reasons and score
  -> stable sort and assign ranks
  -> validate and render plan manifest
  -> verify selects explicit IDs or manifest top N
```

Responsibilities are:

- **Analyzer:** discover mutation candidates and source metadata.
- **Ranking component:** calculate reasons, scores, tie-breaking order, and ranks.
- **Plan manifest:** persist the completed ranking as versioned invocation data.
- **Manifest validation:** verify ranking and existing integrity invariants before
  project work.
- **Verify selection:** choose either exact IDs or the ranked retained prefix.
- **Rendering:** expose ranks and reasons in structured and human plan output and
  describe top-N selection scope in verify output.

The ranking component must not read files, run Git, or mutate candidates. This keeps
ranking independently testable and prevents verify from changing a plan's meaning.

## Error Handling

CLI selection errors use exit code 2 and occur before reading or executing project
tests where argument parsing permits:

- `--top 0`;
- `--top` combined with any `--candidate`; and
- neither selection mode.

Manifest compatibility or integrity errors also use exit code 2 and occur before
baseline execution:

- `--top` with a legacy manifest lacking ranking data;
- invalid rank, score, or reason invariants; and
- any existing manifest validation failure.

`N` greater than the retained candidate count is not an error. It selects every
retained candidate and reports the actual selected count.

## Testing Strategy

Unit and integration tests will cover:

- deterministic score, reason, tie-break, and rank generation;
- accumulation and priority of explicit line, explicit symbol, and changed-line
  reasons;
- every operator category and its fixed score;
- stable ordering by path, line, column, operator ID, and candidate ID;
- new manifest serialization, schema validation, and round trips;
- rejection of missing, duplicate, zero, non-contiguous, reordered, or tampered
  ranking data before baseline execution;
- `verify --top N` selecting and executing only the ascending-rank prefix;
- `N` below, equal to, and above the retained candidate count;
- mutual exclusion of `--top` and `--candidate`;
- rejection of zero and missing selections;
- legacy manifests remaining valid for `--candidate` and being rejected for
  `--top`;
- explicit treatment of truncated plans as retained-subset rankings;
- JSON, human output, JSON Schemas, CLI help, and README examples; and
- existing Linux, macOS, and Windows behavior.

## Success Criteria

The feature is complete when:

1. A user can plan and execute prioritized candidates with two commands and without
   extracting candidate IDs.
2. The selected candidates and their order are reproducible from the saved manifest.
3. Every rank is explainable through structured reason codes and fixed scores.
4. Exact `--candidate` workflows and legacy manifests remain supported as specified.
5. Truncated plans cannot be mistaken for a ranking over undiscovered candidates.
6. All repository tests, schema contracts, wheel smoke tests, and platform CI checks
   pass.

## Non-Goals

- Learning rankings from historical mutation results.
- Claiming that rank predicts defect probability.
- Automatically running candidates when verify selection is omitted.
- Changing candidate discovery or filtering behavior.
- Combining `--top` and explicit candidate IDs.
- Re-ranking a saved plan during verify.
