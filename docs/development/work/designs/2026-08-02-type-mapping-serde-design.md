# Type mapping operator serialization design

## Problem

`MutationOperator::TypeMapping` uses the derived Serde spelling
`type_mapping`, while the CLI, analyzer protocol, fingerprints, and candidate
records use the canonical name `type_dict_mapping`. Plan manifests can therefore
contain two names for the same operator and cannot be edited using the documented
canonical spelling.

## Design

Give the enum variant an explicit Serde rename to `type_dict_mapping`, making
serialization and deserialization agree with `MutationOperator::as_str()` and
all other string boundaries. Accept `type_mapping` only as a Serde alias so
already-written manifests remain readable; new output always uses the canonical
name. No CLI or analyzer alias is introduced.

## Tests

Plan-config tests assert that a plan selecting the mapping operator serializes
`type_dict_mapping` and that a historical `type_mapping` value can still be
deserialized to `MutationOperator::TypeMapping`.
