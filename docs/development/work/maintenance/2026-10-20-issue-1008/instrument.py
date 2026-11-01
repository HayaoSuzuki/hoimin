"""Temporary private constructor/drop logging; never expose a production API.

Run only after preserving ordinary release executables and linking both ordinary
probes. This script backs up/restores the exact candidate.rs bytes. Eager variant
is task3-base candidate.rs (Tasks1/2 remain), not a pristine original checkout.
"""
import difflib,hashlib,json,pathlib,subprocess,sys
ROOT=pathlib.Path.cwd(); BASE=pathlib.Path('/tmp/hoimin-perf-three-task3'); file=ROOT/'crates/hoimin-core/src/candidate.rs'
backup=BASE/'candidate-production.rs'; mode=sys.argv[1]
def sha(s):return hashlib.sha256(s.encode()).hexdigest()
if mode=='restore':
 file.write_bytes(backup.read_bytes());sys.exit(0)
if not backup.exists():backup.write_bytes(file.read_bytes())
source=(subprocess.check_output(['git','show','e5e33603a4ed6a269d4131fbb6dbe99a409de05c:crates/hoimin-core/src/candidate.rs'],text=True) if mode=='eager' else backup.read_text())
original=source
source=source.replace('impl PythonSourceIndex {','''static OBS_INDEX_BUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static OBS_INDEX_LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
impl Drop for PythonSourceIndex {
    fn drop(&mut self) {
        let live = OBS_INDEX_LIVE.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) - 1;
        eprintln!("INDEX_EVENT drop live={live}");
    }
}
impl PythonSourceIndex {''')
needle='''        Self {
            source_len: u32::try_from(source.len())'''
assert source.count(needle)==1
source=source.replace(needle,'''        let build = OBS_INDEX_BUILDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let live = OBS_INDEX_LIVE.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        eprintln!("INDEX_EVENT build count={build} live={live} bytes={}", source.len());
        Self {
            source_len: u32::try_from(source.len())''')
needle='''        if decoded.encoding() == PythonSourceEncoding::Latin1 {'''
assert source.count(needle)==1
source=source.replace(needle,'''        eprintln!("INDEX_EVENT validation bytes={}", source.len());
'''+needle)
file.write_text(source)
(BASE/f'instrument-{mode}.patch').write_text(''.join(difflib.unified_diff(original.splitlines(True),source.splitlines(True),fromfile='candidate.rs uninstrumented '+mode,tofile='candidate.rs instrumented '+mode)))
(BASE/f'instrument-{mode}.json').write_text(json.dumps(dict(mode=mode,product_base='e5e33603a4ed6a269d4131fbb6dbe99a409de05c',original_source_sha256=sha(original),instrumented_source_sha256=sha(source),scope='Only candidate.rs instrumented. Eager uses task3-base file; rest of working tree includes Task1/2 and current task3 private location argument removal. Not pristine original. Constructor logs after allocation; Drop logs at real ownership end. Global atomics include all threads; observe one fixture/process.'),indent=2))
