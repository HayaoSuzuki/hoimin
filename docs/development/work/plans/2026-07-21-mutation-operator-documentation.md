# Mutation Operator Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the README's `Mutation operators` section describe the current default and opt-in operator-selection contract without MVP terminology.

**Architecture:** This is a documentation-only change to `README.md`. The runtime operator list remains the default set; the surrounding prose will distinguish it from the opt-in type-annotation families and define the ordering of explicit inclusion and exclusion selectors. Existing documentation-contract coverage executes every fenced README command, and targeted text checks will guard the terminology and selection contract.

**Tech Stack:** Markdown, Rust integration test harness, Cargo.

## Global Constraints

- Change only the `Mutation operators` section in `README.md`.
- Do not change CLI behavior, operator IDs, defaults, analyzer behavior, or JSON contracts.
- The default set comprises the 13 runtime operators currently represented by the existing list.
- `--operators` selects the explicit operator set; `--exclude-operators` removes individual IDs or selector families from that set.
- Keep type annotation operators opt-in and retain the existing selector families, seven IDs, example, and `killed` semantics.
- Commit with `[skip ci]` in the subject as requested by the user.

---

### Task 1: Refresh the mutation-operator reference

**Files:**

- Modify: `README.md:65-91`
- Test: `crates/hoimin-cli/tests/report_handler.rs:43-145` (existing `documentation_contract`)

**Interfaces:**

- Consumes: `MutationOperatorSelection::all_legacy()` and `parse_selector()` in `crates/hoimin-core/src/config.rs`.
- Produces: an accurate public explanation of the default runtime set and the opt-in type-annotation selection contract.

- [ ] **Step 1: Confirm the implemented selection contract and baseline documentation test**

Run:

```bash
sed -n '65,91p' README.md
sed -n '129,207p' crates/hoimin-core/src/config.rs
cargo test -p hoimin-cli --test report_handler documentation_contract
```

Expected: the README contains `The MVP operator set is:`, the core code defaults to `all_legacy()` and expands the three `type_` selector families, and the documentation contract passes.

- [ ] **Step 2: Replace the operator-section prose**

In `README.md`, replace the paragraph beginning `The MVP operator set is:` and the following default-selection paragraph with:

```markdown
The default runtime operator set is:

- equality (`==` ↔ `!=`) and ordered comparisons (`<`, `<=`, `>`, `>=`);
- membership (`in` ↔ `not in`) and identity (`is` ↔ `is not`);
- boolean `and` ↔ `or`;
- binary and augmented `+` ↔ `-`;
- `*` ↔ `/` and `//` ↔ `%`;
- unary `+` ↔ `-`;
- removal of unary `not`;
- `True` ↔ `False`;
- `break` ↔ `continue`.

Without `--operators`, a run selects all 13 runtime operators and does not mutate type annotations. Supplying `--operators` (comma-separated) selects an explicit operator set instead; `--exclude-operators` then removes individual operators or selector families from that set.
```

Leave the following type-annotation example, selector-family reference, individual IDs, and `killed` explanation unchanged.

- [ ] **Step 3: Check the required text and absence of historical wording**

Run:

```bash
rg -n -F 'The default runtime operator set is:' README.md
rg -n -F 'all 13 runtime operators' README.md
rg -n -F 'The MVP operator set is:' README.md
rg -n -F 'type_nullable,type_collections' README.md
rg -n -F 'type_nullable_remove' README.md
```

Expected: the first, second, fourth, and fifth commands each print one matching line; the third command prints no matches and exits with status 1.

- [ ] **Step 4: Run the documentation contract**

Run:

```bash
cargo test -p hoimin-cli --test report_handler documentation_contract
```

Expected: PASS. The test parses and executes each fenced README `hoimin run` command and validates JSON output against the published schemas.

- [ ] **Step 5: Inspect the final diff and commit the documentation update without CI**

Run:

```bash
git diff --check
git diff -- README.md
git add README.md
git commit -m "docs: clarify mutation operator selection [skip ci]"
```

Expected: no whitespace errors; the diff is limited to `Mutation operators`; Git creates a commit whose subject includes `[skip ci]`.
