# Binary diff path parsing plan

1. Add an integration regression with `pkg/x and y.py` plus a legitimate
   changed `pkg/y.py`; run it red.
2. Request `git diff --numstat -z` for the same base/HEAD range and parse binary
   paths, including NUL-delimited rename records.
3. Remove human binary-sentence parsing from the patch parser.
4. Run focused target tests, formatting, lint, workspace tests, and diff checks.
5. Request independent review, commit, push, and create a PR closing #105.
