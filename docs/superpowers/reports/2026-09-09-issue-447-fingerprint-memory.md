# Issue #447 fingerprint input memory report

## Task 1 implementation

Fingerprint resolution now stores `Option<blake3::Hash>` in its ordered selection map. Each exact
root-relative read is hashed immediately, allowing its byte buffer to drop before the next exact
file is read. Glob-selected files keep the existing final traversal and safe reader; that read is
hashed before the unchanged hex string is emitted. The resolver API, sorted records, duplicate
handling, read order, error categories, and root-relative safety checks remain unchanged.

## Compatibility coverage

The resolver tests cover binary bytes including `0xff`, normalized exact aliases and duplicates,
glob/exact overlap, exact selection order, and a second resolve after changing a file. Existing
missing, directory, symlink, symlink-parent, non-UTF-8, unreadable, unsafe-path, and error-order
tests remain green.

## Performance RED

The issue's reproduced before-binary CLI measurement is the performance RED for this change. With
three samples per condition and the same 16 MiB files, exact-file peak RSS grew from 30.8 MB for
one file to 148.3 MB for eight files, while the equivalent glob input stayed around 31 MB. Hashes
and candidates matched. This is retained-input memory evidence, rather than an OOM or process
resource-limit claim. The controller owns the paired after measurement.

## Validation

All Cargo commands used `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target` and
`--offline` where applicable.

| Command | Result |
| --- | --- |
| `cargo test --offline -p hoimin-cli --test fingerprint_inputs` before change | 20 passed |
| Same command after change | 23 passed |
| `cargo test --offline -p hoimin-cli --test cli_config fingerprint` | 1 passed, 54 filtered |
| `cargo test --offline -p hoimin-cli --test plan fingerprint` | 3 passed, 43 filtered |
| `cargo fmt --all` and `git diff --check` | passed |

No new dependency, public API, schema, fingerprint algorithm, unsafe code, or CI memory/timing
threshold was added.

## Task report

Status: DONE

Concerns: none. Whole-workspace, MSRV, Clippy, paired RSS, independent review, and PR checks are
owned by the controller.

## Final controller verification

Final production/test commit:cb461a7. Full workspace/all-features:1595passed,0failed,13ignored,67groups. MSRV1.88 and Clippy workspace/all-targets/all-features warnings denied passed. Task specification/quality review and final whole-branch review both approved without required findings. Design and plan each contain three self-review rounds.

## Paired actual CLI memory measurement

macOS arm64, Rust1.98 debug binaries, each input file16MiB. Immutable before binary built from main58817cf; after binary built from this worktree by the completed workspace test command. One fixture root is shared by each old/new pair; three pairs per file count. All runs exited0 and every pair had identical complete fingerprint_inputs and candidates. The measured value is `/usr/bin/time -l` maximum resident set size in bytes. All local builds and test suites finished before measurement.

| Exact files | Before median RSS | After median RSS |
|---:|---:|---:|
|1|30,441,472 B|30,932,992 B|
|4|81,182,720 B|31,129,600 B|
|8|148,291,584 B|31,047,680 B|

The fixed-size digest map removes the aggregate file-byte retention. One-file RSS is effectively unchanged within measurement variability; the change does not make single-file reading streaming or demonstrate a particular total-command speedup. No CI RSS threshold was introduced.

Logs: `/private/tmp/hoimin-447-workspace.log`, `hoimin-447-msrv.log`, `hoimin-447-clippy.log`, `hoimin-447-benchmark.log`.

<details>
<summary>Paired memory reproduction (macOS)</summary>

Run this Python script with the baseline and modified CLI paths as its two arguments. It creates and removes its own fixture.

```python
import json, subprocess, tempfile, re, sys, statistics
from pathlib import Path
before,after=sys.argv[1:]
with tempfile.TemporaryDirectory(prefix='hoimin-447-benchmark-') as d:
    root=Path(d);(root/'case.py').write_text('x = 1 + 2\n')
    for n in [1,4,8]:
        for i in range(n):
            path=root/f'data{i}.bin'
            if not path.exists():
                with path.open('wb') as out:
                    for _ in range(16): out.write(b'x'*(1024*1024))
        samples={'before':[], 'after':[]}
        options=sum((['--fingerprint-file',f'data{i}.bin'] for i in range(n)),[])
        for _ in range(3):
            documents={}
            for mode,binary in [('before',before),('after',after)]:
                result=subprocess.run(['/usr/bin/time','-l',binary,'plan','--root',d,'--file','case.py','--operators','binary_add_sub','--allow-best-effort-memory','--min-free-space','1B',*options,'--','true'],capture_output=True,text=True,timeout=30)
                assert result.returncode==0,result.stderr
                documents[mode]=json.loads(result.stdout)
                samples[mode].append(int(re.search(r'(\d+)\s+maximum resident set size',result.stderr).group(1)))
            assert documents['before']['fingerprint_inputs']==documents['after']['fingerprint_inputs']
            assert documents['before']['candidates']==documents['after']['candidates']
        print(json.dumps({'files':n,'before_rss_bytes':statistics.median(samples['before']),'after_rss_bytes':statistics.median(samples['after']),'samples':samples}))
```

</details>
