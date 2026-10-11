# Questions and bug reports

External Issues and pull requests are not accepted. Maintainers use Issues to
track work they choose to undertake. Public usage questions and possible defects
belong in [Discussions](https://github.com/HayaoSuzuki/hoimin/discussions).

Choose **Bug reports** for unexpected behavior or **Q&A** for usage questions.
The forms request enough context to discuss a problem without requiring a full
repository or complete logs. If a defect needs tracked work, a maintainer creates
an Issue and links the discussion. Posting does not commit the maintainer to a fix.

## Information to include

For a possible defect, provide a short summary, hoimin version, OS/CPU, the command,
and expected versus actual behavior. Run `hoimin --version` when possible; if
startup fails, provide the installed/downloaded version or state that it is
unavailable. Include the Python version used by the test command when known.

Copy the full command, including arguments after `--`, and relevant configuration.
Mention `run`, `plan`, `verify`, resume, `progress` or `review` when applicable.
If available, include the exit code, resource-control mode, explicit best-effort
flags and relevant memory/process/time limits. A failing baseline and a killed
or surviving mutant describe different outcomes; identify which one you observed.

For a usage question, describe the intended result and the question. Commands,
things already tried and environment details are optional when they do not help
explain it.

## Minimal reproduction

Work in your own local copy and keep the failure you observed while shrinking it:

1. Select one source file or symbol, one small test and the relevant configuration.
2. Remove unrelated code, test dependencies and input data in small steps. Re-run
   your original command after each reduction so that the reported behavior stays.
3. Include small source/test snippets, setup steps and the exact command. Record
   expected and actual results separately. Installation failures can be reported
   without a runnable source/test example.

For **unexpected mutation candidates**, retain the expression, annotations and
bindings needed to produce the candidate. Include its operator/location and the
relevant source excerpt. Do not assume that fewer lines preserve name resolution.

For a **baseline failure**, retain the small test and dependencies needed for the
failure. Include its ordinary test-command result and hoimin's baseline diagnostic;
do not label it as a surviving mutant.

For an **OS/resource-control problem**, retain the OS/CPU, resource-control mode,
explicit best-effort flags and limits. Keep one small source/test pair when possible.
Do not remove the limit or backend setting that triggers the problem just to make
the example smaller.

Discussions are public. Inspect excerpts before publishing and remove credentials,
private paths and data you cannot share. Small redacted snippets are sufficient;
uploading an archive, full report or repository is not required. This repository
does not add bots or CI jobs that fetch or execute discussion submissions.

Maintainers can follow the [discussion maintenance procedure](development.md#discussion-maintenance)
when changing categories or forms.
