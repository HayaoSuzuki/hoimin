# Mutation operator documentation

## Goal

Update only the `Mutation operators` section of the README so it describes
the current operator-selection contract rather than the historical MVP.

## Scope

- Describe the 13 default runtime operators as the default operator set.
- State that `--operators` selects an explicit set and that
  `--exclude-operators` removes operators from that selected set.
- Keep the type-annotation operators explicitly opt-in, with their three
  selector families and seven individual operator IDs.
- Preserve the existing type-checker command and the `killed` semantics.

## Non-goals

- Change CLI behavior, operator IDs, defaults, or analyzer behavior.
- Document the unimplemented focused mutation profile.
- Restructure unrelated README sections.

## Documentation shape

The section opens with “The default runtime operator set is:” followed by the
existing runtime mutation categories. A concise selection paragraph then
distinguishes the default set from an explicit `--operators` selection and
explains the ordering of `--exclude-operators`. The opt-in type-annotation
paragraph, selector-family/individual-ID reference, example, and `killed`
semantics remain immediately after it.

## Verification

- Ensure the README no longer contains “MVP operator set”.
- Run the existing `documentation_contract` test, which parses and executes
  all fenced README `hoimin run` commands.
