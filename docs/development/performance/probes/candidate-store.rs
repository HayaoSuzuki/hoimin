//! Copy to crates/hoimin-cli/examples/profile_candidate_store.rs to run.
//! A bounded, dependency-free measurement driver, not a timing assertion.
use std::hint::black_box;
use std::io::Write;
use std::time::Instant;

use hoimin_cli::analyzer::CandidateCursor;
use hoimin_cli::analyzer::CandidateStore;
use hoimin_core::{ByteSpan, MutationCandidate};

#[derive(Default)]
struct Counter(usize);

impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = &args[1];
    let count: u64 = args[2].parse().unwrap();
    let size: usize = args[3].parse().unwrap();
    assert!((1..=100_000).contains(&count) && size <= 1_000_000);
    let text = args.get(4).map_or("a", String::as_str);
    let mut candidate = MutationCandidate {
        id: "probe".into(),
        sequence: 1,
        path: "src/calc.py".into(),
        span: ByteSpan {
            start: 0,
            length: 1,
        },
        original: text.repeat(size),
        replacement: "b".into(),
        operator: "string_literal_empty".into(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: "0".repeat(64),
    };
    let directory = tempfile::tempdir().unwrap();
    let mut store = CandidateStore::new_in(count, directory.path()).unwrap();
    if mode == "replay" {
        for sequence in 1..=count {
            candidate.sequence = sequence;
            store.push(&candidate).unwrap();
        }
    }
    let spool = store.finish().unwrap();
    let mut store = CandidateStore::new_in(count, directory.path()).unwrap();
    let mut cursor = CandidateCursor::START;
    let mut bytes = 0;
    let start = Instant::now();
    for sequence in 1..=count {
        candidate.sequence = sequence;
        match mode.as_str() {
            "count" => {
                let mut counter = Counter::default();
                serde_json::to_writer(&mut counter, black_box(&candidate)).unwrap();
                bytes += black_box(counter.0 + 1);
            }
            "encode" => {
                let mut encoded = serde_json::to_vec(black_box(&candidate)).unwrap();
                encoded.push(b'\n');
                bytes += black_box(encoded.len());
            }
            "push" => store.push(black_box(&candidate)).unwrap(),
            "replay" => {
                let (record, next) = CandidateStore::replay_one(&spool, cursor).unwrap().unwrap();
                black_box(record);
                cursor = next;
            }
            _ => panic!("unknown mode"),
        }
    }
    let elapsed = start.elapsed();
    println!(
        "{{\"mode\":{mode:?},\"count\":{count},\"size\":{size},\"seconds\":{},\"bytes\":{bytes}}}",
        elapsed.as_secs_f64()
    );
}
