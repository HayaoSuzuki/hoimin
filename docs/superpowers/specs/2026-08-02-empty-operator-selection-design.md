# Empty mutation-operator selection design

## Problem

Operator include/exclude normalization can remove every mutation operator, but
normalized run and plan validation currently accepts the empty set. The command
then performs setup and baseline work before reporting a successful zero-mutant
run, while plan verification can fail later with a misleading missing-candidate
error.

## Design

Add an `is_empty` query to `MutationOperatorSelection` and a dedicated
`ConfigError::EmptyMutationOperatorSelection` whose message identifies
`--operators` and `--exclude-operators` as the conflicting inputs. A shared
validator rejects an empty normalized set.

Apply the validator in all normalized configuration entry points:

- immediately after raw include/exclude expansion in `RunConfig::try_from`, so
  CLI commands fail before workspace or baseline work;
- `RunConfig::validate`, covering programmatically constructed run configs;
- `PlanConfig::validate`, covering persisted or hand-edited manifests.

## Tests

Core configuration tests cover a raw include/exclude pair that cancels to empty,
an empty normalized `RunConfig`, and an empty deserialized `PlanConfig`. Existing
valid operator-selection tests guard unchanged non-empty behavior.
