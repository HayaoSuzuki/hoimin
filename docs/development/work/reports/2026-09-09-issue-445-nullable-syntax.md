# Issue #445 nullable annotation syntax repair

Nullable-removal candidates now preserve the source syntax required by the
retained type expression. Union removal retains explicit operand parentheses
through Ruff token boundaries. Optional removal locates its own brackets after
the complete base expression, keeps its original interior, and groups
multiline/comment-bearing interiors when removal would otherwise invalidate the
annotation.

Focused Rust regressions require one candidate and reparse the complete mutated
module for Optional, both union orders, parameter and return annotations, and
comments. The public-plan CPython contracts evaluate type members for variable,
parameter, and return annotations, including unparenthesized multiline Optional
forms, nested grouping, comments, and ordinary single-line controls. The issue
reproduction changes `{int, str, NoneType}` to `{int, str}`.

The initial focused regression failed before the implementation because
`Optional[(int\n | str)]` produced `int\n | str`. After the repair,
`cargo test --offline -p hoimin-cli --lib
analyzer::rust::rust_tests::nullable_removal_preserves_multiline_annotation_syntax
-- --exact` passed 1 test, and `cargo test --offline -p hoimin-cli --test
operator_function_contracts` passed 14 tests. `cargo fmt --all -- --check` and
`git diff --check` both exited 0. The controller owns full-workspace, MSRV,
Clippy, and independent CLI-matrix verification.

## Review round 1 repair

The retained side of an unparenthesized multiline union is now grouped before
replacement, independently of annotation-level parentheses outside the candidate
span. Optional bracket interiors retain leading and trailing whitespace around
an already-grouped operand; only an exact full-interior match may use the
existing grouping directly. Exact-replacement/reparse and public-plan CPython
contracts cover both repairs.

The public-plan contracts are split into annotation-site, Optional-layout, and
union-layout tests to keep each behavioral scope small enough for
warnings-denied Clippy. Focused verification passed: the analyzer regression
passed 1 test, `operator_function_contracts` passed 16 tests, and
`cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`,
formatting, and diff checks exited 0. The controller owns the final workspace
and CLI-matrix verification.

## Review round 2 repair

Trivia detection now inspects gaps between Ruff parser tokens and explicit
comment/newline tokens. It groups actual multiline or commented source without
mistaking `#` inside a string literal for a comment. Regressions and public-plan
contracts cover both `resolve("#") | None` and `Optional[resolve("#")]`, while
the multiline/comment cases remain covered. The focused analyzer test passed,
the 16-test contract target passed, and warnings-denied all-targets/all-features
Clippy, formatting, and diff checks passed.

## Final controller verification

Final production commit:96d8ee4. On this source:

- `cargo test --offline --workspace --all-features -- --test-threads=1`:1597passed,0failed,13ignored,67test groups.
- `cargo +1.88 check --offline --locked --workspace --all-targets --all-features`:passed.
- Workspace/all-targets/all-features Clippy with warnings denied:passed in implementer final gate.
- Independent actual CLI/CPython matrix:54/54passed (48original cases plus6review-boundary contexts).
- Actual py_compile-only run: baseline Exit(0), mutant survived/Exit(0), score0, CLI exit1. Before the change it was killed/Exit(1), score1, CLI exit0.
- Task specification and quality review passed after two fix rounds; final whole-branch review approved with no required findings.

The review fixes preserve multiline union context, Optional interior whitespace and ordinary expressions containing a literal `#`. No candidate dropping or result-policy changes were introduced. Design and plan each have three recorded self-review rounds.

The CLI binary was built by the full workspace test invocation. Local logs: `/private/tmp/hoimin-445-workspace-final2.log`, `/private/tmp/hoimin-445-msrv-final2.log`, `/private/tmp/hoimin-445-matrix-final2.log`, `/private/tmp/hoimin-445-cli-run-final2.log`.

<details>
<summary>Independent matrix reproduction</summary>

Run with the target CLI path as the first argument under the selected CPython interpreter.

```python
import json, subprocess, tempfile, sys
from pathlib import Path
B=sys.argv[1]
python=sys.executable
expressions=[
 'int | str',
 '(int\n | str)',
 '((int # inner\n | str))',
 '(int | str # trailing\n)',
]
annotations=[]
for e in expressions:
 annotations.extend([f'Optional[{e}]', f'None | ({e})', f'({e}) | None'])
annotations.extend(['Optional[int\n | str]', 'Optional[\n # leading\n int | str # trailing\n]', '(Optional)[(int\n | str)]', '(typing\n .Optional)[int\n | str]'])
annotations.extend(['(int\n | str\n | None)', 'Optional[\n (int | str)\n]'])
results=[]
with tempfile.TemporaryDirectory(prefix='hoimin-445-matrix-') as d:
 path=Path(d)/'case.py'
 for ai,a in enumerate(annotations):
  for context in ['variable','parameter','return']:
   body={'variable':f'x: {a}\n', 'parameter':f'def f(x: {a}):\n    pass\n', 'return':f'def f() -> {a}:\n    pass\n'}[context]
   source='import typing\nfrom typing import Optional\n'+body
   compile(source,'case.py','exec')
   path.write_text(source)
   r=subprocess.run([B,'plan','--root',d,'--file','case.py','--operators','type_nullable_remove','--allow-best-effort-memory','--min-free-space','1B','--','true'],capture_output=True,text=True,check=True)
   cs=json.loads(r.stdout)['candidates']
   status='missing candidate' if len(cs)!=1 else 'ok'
   if len(cs)==1:
    c=cs[0]; b=source.encode(); start=c['span']['start'];end=start+c['span']['length']
    mutated=b[:start]+c['replacement'].encode()+b[end:]
    path.write_bytes(mutated)
    target='case' if context=='variable' else 'case.f'
    key={'variable':'x','parameter':'x','return':'return'}[context]
    harness=f'import case,typing; t=typing.get_type_hints({target})[{key!r}]; assert set(typing.get_args(t))=={{int,str}},repr(t)'
    p=subprocess.run([python,'-B','-c',harness],cwd=d,capture_output=True,text=True)
    if p.returncode: status=p.stderr.strip().splitlines()[-1]
   results.append({'case':ai,'context':context,'status':status})
for r in results: print(json.dumps(r))
print(json.dumps({'total':len(results),'passed':sum(x['status']=='ok' for x in results)}))
```

</details>
