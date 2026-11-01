#!/usr/bin/env python3
"""Temporary stage observer; apply in an idle checkout, run Cargo separately, restore.

Usage: python3 measure-exception-stages.py apply-baseline CHECKOUT
       python3 measure-exception-stages.py apply CHECKOUT
       cargo test -p hoimin-cli --test exception_stage_probe -- --nocapture
       python3 measure-exception-stages.py restore CHECKOUT

Supports baseline 03c7e00 and the incremental implementation. No Cargo commands
are executed here. Backups preserve exact bytes; restore refuses modified probes.
"""
import hashlib
import json
import pathlib
import sys

CLI = 'crates/hoimin-cli/src/analyzer/exception_project.rs'
PYTHON = 'crates/hoimin-python/src/analyzer/exception_hierarchy.rs'
LIB = 'crates/hoimin-python/src/lib.rs'
TEST = 'crates/hoimin-cli/tests/exception_stage_probe.rs'
STATE = '.exception-stage-probe-backup'

OBSERVER = '''
// Temporary measurement-only callback, removed by measure-exception-stages.py.
static EXCEPTION_PROBE: std::sync::Mutex<Option<fn(&str, usize, usize)>> = std::sync::Mutex::new(None);
pub fn set_exception_probe(callback: Option<fn(&str, usize, usize)>) {
    *EXCEPTION_PROBE.lock().unwrap() = callback;
}
pub fn exception_probe(event: &str, decoded: usize, copied: usize) {
    let callback = *EXCEPTION_PROBE.lock().unwrap();
    if let Some(callback) = callback { callback(event, decoded, copied); }
}
'''

def replace_once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f'expected exactly one anchor: {old[:100]!r}')
    return source.replace(old, new, 1)

def instrument_cli(source, incremental):
    source = replace_once(source, '        let reader = crate::workspace::PortableFileReader::open',
        '        hoimin_python::exception_probe("load_begin", 0, 0);\n        let reader = crate::workspace::PortableFileReader::open')
    source = replace_once(source, '            let bytes = reader',
        '            hoimin_python::exception_probe("acquire_begin", 0, 0);\n            let bytes = reader')
    needle = '            if decoded.text().len() > MAX_FILE_BYTES {'
    copied = '0' if incremental else 'sources.iter().map(|(_, text): &(Utf8PathBuf, String)| text.len()).sum()'
    source = replace_once(source, needle,
        f'            hoimin_python::exception_probe("decoded", decoded.text().len(), {copied});\n' + needle)
    if incremental:
        source = replace_once(source, '            if let Ok(builder) = &mut builder {',
            '            hoimin_python::exception_probe("acquire_end", decoded.text().len(), 0);\n            if let Ok(builder) = &mut builder {')
        source = replace_once(source, '                builder.add_source(&path, decoded.text(), cancelled);',
            '                hoimin_python::exception_probe("parse_begin", decoded.text().len(), 0);\n                builder.add_source(&path, decoded.text(), cancelled);\n                hoimin_python::exception_probe("parse_end", decoded.text().len(), 0);')
        source = replace_once(source, '        let index = Arc::new(',
            '        hoimin_python::exception_probe("summary_retained", 0, 0);\n        hoimin_python::exception_probe("index_begin", 0, 0);\n        let index = Arc::new(')
        source = replace_once(source, '        *self.index.lock()',
            '        hoimin_python::exception_probe("index_end", 0, 0);\n        hoimin_python::exception_probe("index_retained", 0, 0);\n        *self.index.lock()')
    else:
        needle = '            sources.push((path, decoded.text().to_owned()));'
        source = replace_once(source, needle, needle + '''
            hoimin_python::exception_probe("copied", decoded.text().len(), sources.iter().map(|(_, text)| text.len()).sum());
            drop(decoded);
            drop(bytes);
            hoimin_python::exception_probe("acquire_end", 0, sources.iter().map(|(_, text)| text.len()).sum());''')
        source = replace_once(source, '        *self.index.lock()', '''        let body_capacity: usize = sources.iter().map(|(_, text)| text.capacity()).sum();
        hoimin_python::exception_probe("body_capacity", 0, body_capacity);
        drop(sources);
        hoimin_python::exception_probe("index_retained", 0, 0);
        *self.index.lock()''')
    source = replace_once(source, '        Ok(index)\n',
        '        hoimin_python::exception_probe("load_end", 0, 0);\n        Ok(index)\n')
    return source

def instrument_python(source, incremental):
    if not incremental:
        source = replace_once(source, '            let parsed = match depth::parse_checked(source, cancelled) {',
            '            exception_probe("parse_begin", 0, 0);\n            let parsed = match depth::parse_checked(source, cancelled) {')
        source = replace_once(source, '            index.modules.insert(path.clone(), summary);',
            '            index.modules.insert(path.clone(), summary);\n            drop(parsed);\n            exception_probe("parse_end", 0, 0);')
        source = replace_once(source, '        index.resolve_relative_imports(cancelled)?;', '''        let body_capacity: usize = sources.iter().map(|(_, text)| text.capacity()).sum();
        exception_probe("summary_retained", 0, body_capacity);
        exception_probe("index_begin", 0, 0);
        index.resolve_relative_imports(cancelled)?;''')
        source = replace_once(source, '        index.build_visible(cancelled)?;\n        Ok(index)',
            '        index.build_visible(cancelled)?;\n        exception_probe("index_end", 0, 0);\n        Ok(index)')
    return source + OBSERVER

TEST_SOURCE = r'''//! Temporary stage probe. Removed after collecting evidence.
use camino::Utf8PathBuf;
use hoimin_core::{MutationOperatorSelection, MutationProfile, TargetSlice};
use std::sync::Mutex;
mod heap_tracking {
    include!("support/heap_tracking.rs");
    pub fn live() -> usize { LIVE.load(Ordering::Relaxed) }
}
#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;
#[derive(Clone, Copy, Default, Debug)]
struct Metrics {
    baseline: usize,
    stage_start: usize,
    acquire_peak: usize,
    parse_peak: usize,
    index_peak: usize,
    summary_retained: usize,
    index_retained: usize,
    acquired_decoded_peak: usize,
    project_copy_peak: usize,
    decoded_plus_copy_peak: usize,
    body_capacity: usize,
}
static METRICS: Mutex<Metrics> = Mutex::new(Metrics {
    baseline: 0, stage_start: 0, acquire_peak: 0, parse_peak: 0, index_peak: 0,
    summary_retained: 0, index_retained: 0, acquired_decoded_peak: 0,
    project_copy_peak: 0, decoded_plus_copy_peak: 0, body_capacity: 0,
});
fn observe(event: &str, decoded: usize, copied: usize) {
    let mut m = METRICS.lock().unwrap();
    let live = heap_tracking::live();
    match event {
        "load_begin" => { *m = Metrics::default(); m.baseline = live; }
        "acquire_begin" | "parse_begin" | "index_begin" => {
            m.stage_start = live.saturating_sub(m.baseline);
            heap_tracking::begin();
        }
        "acquire_end" | "parse_end" | "index_end" => {
            let peak = m.stage_start + heap_tracking::finish();
            match event {
                "acquire_end" => m.acquire_peak = m.acquire_peak.max(peak),
                "parse_end" => m.parse_peak = m.parse_peak.max(peak),
                _ => m.index_peak = m.index_peak.max(peak),
            }
        }
        "summary_retained" => {
            m.summary_retained = live.saturating_sub(m.baseline).saturating_sub(copied);
        }
        "index_retained" => m.index_retained = live.saturating_sub(m.baseline),
        "body_capacity" => m.body_capacity = copied,
        _ => {}
    }
    if event == "decoded" || event == "copied" {
        m.acquired_decoded_peak = m.acquired_decoded_peak.max(decoded);
        m.project_copy_peak = m.project_copy_peak.max(copied);
        m.decoded_plus_copy_peak = m.decoded_plus_copy_peak.max(decoded + copied);
    }
}
#[test]
fn production_exception_project_stage_measurements() {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let targets = [TargetSlice { path: "m00.py".into(), lines: vec![], symbols: vec![] }];
    let operators: MutationOperatorSelection = serde_json::from_str("[\"exception_hierarchy\"]").unwrap();
    let suffix = "\nclass Root(Exception): pass\nclass Child(Root): pass\ntry: action()\nexcept Child: pass\n";
    let source = format!("#{}{suffix}", "x".repeat(65536 - 1 - suffix.len()));
    std::fs::write(root.join("m00.py"), &source).unwrap();
    drop(runtime.block_on(hoimin_cli::analyzer::discover_targets(&root, &targets, &operators, MutationProfile::Full, 16)).unwrap());
    let mut written = 1;
    hoimin_python::set_exception_probe(Some(observe));
    for count in [1, 4, 16] {
        for index in written..count { std::fs::write(root.join(format!("m{index:02}.py")), &source).unwrap(); }
        written = count;
        let discovery = runtime.block_on(hoimin_cli::analyzer::discover_targets(&root, &targets, &operators, MutationProfile::Full, 16)).unwrap();
        let metrics = *METRICS.lock().unwrap();
        eprintln!("exception-stages files={count} {metrics:?}");
        assert_eq!(discovery.candidates.len(), 1);
        assert_eq!(discovery.candidates[0].original, "Child");
        assert_eq!(discovery.candidates[0].replacement, "Root");
        assert!(discovery.diagnostics.is_empty());
        assert!(!discovery.truncated);
    }
    hoimin_python::set_exception_probe(None);
}
'''

def digest(data):
    return hashlib.sha256(data).hexdigest()

def main():
    operation, checkout = sys.argv[1:]
    root = pathlib.Path(checkout).resolve()
    state = root / STATE
    if operation == 'restore':
        manifest = json.loads((state / 'manifest.json').read_text())
        for path, expected in manifest['patched'].items():
            target = root / path
            if path == TEST and not target.exists():
                continue
            accepted = {expected}
            if path in manifest['original']:
                accepted.add(digest((state / path).read_bytes()))
            if digest(target.read_bytes()) not in accepted:
                raise RuntimeError(f'refusing to overwrite changed probe file: {path}')
        for path in manifest['original']:
            (root / path).write_bytes((state / path).read_bytes())
        (root / TEST).unlink(missing_ok=True)
        import shutil
        shutil.rmtree(state)
        print('Restored exact original bytes and removed temporary test.')
        return
    if operation not in {'apply', 'apply-baseline'}:
        raise RuntimeError('operation must be apply, apply-baseline or restore')
    if state.exists() or (root / TEST).exists():
        raise RuntimeError('backup or temporary test already exists; refusing overwrite')
    original = {path: (root / path).read_bytes() for path in [CLI, PYTHON, LIB]}
    inputs = original
    if operation == 'apply-baseline':
        import subprocess
        inputs = {path: subprocess.check_output(['git', 'show', f'03c7e00:{path}'], cwd=root) for path in original}
    incremental = 'ExceptionIndexBuilder' in inputs[CLI].decode()
    modified = {
        CLI: instrument_cli(inputs[CLI].decode(), incremental).encode(),
        PYTHON: instrument_python(inputs[PYTHON].decode(), incremental).encode(),
        LIB: inputs[LIB] + b'\npub use rust::exception_hierarchy::{exception_probe, set_exception_probe};\n',
        TEST: TEST_SOURCE.encode(),
    }
    for path, data in original.items():
        destination = state / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    (state / 'manifest.json').write_text(json.dumps({
        'original': list(original), 'patched': {path: digest(data) for path, data in modified.items()},
        'mode': 'incremental' if incremental else 'baseline',
    }, indent=2))
    try:
        for path, data in modified.items():
            (root / path).write_bytes(data)
    except BaseException:
        for path, data in original.items():
            (root / path).write_bytes(data)
        (root / TEST).unlink(missing_ok=True)
        import shutil
        shutil.rmtree(state)
        raise
    print('Applied temporary observer:', 'incremental' if incremental else 'baseline')

if __name__ == '__main__':
    main()
