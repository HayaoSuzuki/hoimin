# Issue 755: discussion forms

Base: `1b2288e29223292939b313af1809d5c2ebebab7d`.
Branch: `docs/discussion-report-templates`.

The maintainer approved Bug reports and Q&A forms, a minimal-reproduction guide,
README navigation, and activation only after merge. External Issue/PR submissions
remain closed. This supersedes the original Issue-form proposal in #755.

## Design self-reviews

1. Intake policy: Discussions provide conversation; maintainers create tracked
   Issues. A discussion does not promise implementation or invite pull requests.
2. Scope: two static category forms; no workflow, bot, new service or input runner.
3. Required information: five bug-report fields; only the question is mandatory
   for Q&A. Unavailable versions can be stated when startup fails.
4. Data: snippets are sufficient; repositories, archives and full logs are not
   required. Public excerpts need review by their author.
5. Deployment: default-branch templates and repository/category settings are
   separate. Activation and rendered-form checks follow merge.

## Plan self-reviews

1. Read the current Issue, submission policy, README and reporting-related CLI docs.
2. Check GitHub's discussion-form schema rather than copying Issue-only metadata.
3. Use category-matching filenames and document category format/slug verification.
4. Add the guide and navigation, with an OKF Playbook for the repeatable setup.
5. Validate YAML, fields, links, source hashes and representative report contents;
   do not add browser automation, sample posts or runtime mutation campaigns.

## Implementation self-reviews

1. Forms use supported `title` and `body` keys, without Issue-only `name` or
   `description`, implicit labels or external service dependencies.
2. Bug fields distinguish expected/actual outcomes, baseline failures, stage,
   resource mode and command arguments; optional diagnostics remain optional.
3. A startup failure can supply its installation command and unknown version
   without source/test snippets or Python/runtime diagnostics.
4. README replaces the invitation to contribute with maintainer-development
   links and explicitly states the external Issue/PR policy.
5. Setup instructions do not claim that forms are already live; no repository
   settings, submission restrictions, dependencies, CLI contracts or version changed.

## Testing self-reviews

1. Check YAML maps, supported fields/attributes, unique IDs, boolean validations,
   mandatory inputs, dropdown uniqueness and textarea render types.
2. Check relative links and repository-local absolute links, OKF frontmatter,
   footnotes, index reachability and the guide source hash.
3. Walk through candidate, baseline, resource and startup reports against the
   documented fields; verify a general Q&A does not require runtime details.
4. Run whitespace and repository static checks. No Rust/Python runtime behavior
   changed, so additional unit/E2E or mutation testing would not verify the forms.
5. Keep GitHub rendering and category matching explicitly unverified until merge
   and activation. Do not fabricate discussions or widen permissions to test them.

## Validation results

- Both YAML files passed duplicate-key, supported-key, field/ID, required-input,
  dropdown and code-block checks. Bug reports has nine fields, five required;
  Q&A has three fields, one required. No label setup is needed.
- Local links, the new Playbook source hash/footnote, catalog reachability and
  all 42 OKF pages' frontmatter/reserved-file structure passed.
- Repository Ruff format/check, ty and whitespace checks passed.
- Fresh read-only review found no actionable issues in forms, setup or policy.

| Manual scenario review | Information available without extra required fields |
| --- | --- |
| Unexpected candidate | Source/test snippets, operator/location, expected/actual behavior |
| Baseline failure | Ordinary test command and baseline diagnostic, distinguished from mutant results |
| Resource control | OS/CPU, flags, backend mode and relevant limits |
| Startup failure | Failed installation/startup command and unavailable-version statement; no Python/source requirement |
| General Q&A | Question alone; attempts and environment remain optional |

GitHub settings showed Discussions disabled at task start; that remains the
intended pre-merge state. Category matching and GitHub form rendering are not
claimed as verified. No Cargo build ran for this docs/config change, and the
previous work's build artifacts were already cleaned.
