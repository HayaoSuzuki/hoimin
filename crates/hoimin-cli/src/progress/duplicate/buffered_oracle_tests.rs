use super::compare_buffered;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    io::{self, BufRead, ErrorKind, Read},
};

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    mode: String,
    id: String,
    left: Input,
    right: Input,
    events: Vec<Event>,
    outcome: Outcome,
}

#[derive(serde::Deserialize)]
struct Input {
    chunks: Vec<Vec<u8>>,
    fault: Option<Fault>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Side {
    Left,
    Right,
}

impl Side {
    fn label(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Fault {
    Interrupted,
    IoError,
}

impl Fault {
    fn kind(self) -> ErrorKind {
        match self {
            Self::Interrupted => ErrorKind::Interrupted,
            Self::IoError => ErrorKind::Other,
        }
    }
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Event {
    Fill { side: Side },
    Consume { side: Side, count: usize },
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Equal,
    Different,
    Failed { side: Side, fault: Fault },
}

struct ReadTrace<'a> {
    id: &'a str,
    remaining: RefCell<std::slice::Iter<'a, Event>>,
}

impl ReadTrace<'_> {
    fn check(&self, event: &Event) {
        assert_eq!(
            self.remaining.borrow_mut().next(),
            Some(event),
            "{}",
            self.id
        );
    }
}

struct ObservedReader<'a> {
    input: Input,
    side: Side,
    trace: &'a ReadTrace<'a>,
    index: usize,
    offset: usize,
}

impl<'a> ObservedReader<'a> {
    fn new(input: Input, side: Side, trace: &'a ReadTrace<'a>) -> Self {
        assert!(input.chunks.iter().all(|chunk| !chunk.is_empty()));
        Self {
            input,
            side,
            trace,
            index: 0,
            offset: 0,
        }
    }
}

impl Read for ObservedReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let buffer = self.fill_buf()?;
        let count = output.len().min(buffer.len());
        output[..count].copy_from_slice(&buffer[..count]);
        if count != 0 {
            self.consume(count);
        }
        Ok(count)
    }
}

impl BufRead for ObservedReader<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.trace.check(&Event::Fill { side: self.side });
        if let Some(chunk) = self.input.chunks.get(self.index) {
            Ok(&chunk[self.offset..])
        } else if let Some(fault) = self.input.fault {
            Err(io::Error::new(fault.kind(), self.side.label()))
        } else {
            Ok(&[])
        }
    }

    fn consume(&mut self, count: usize) {
        let chunk = &self.input.chunks[self.index];
        assert!(count <= chunk.len() - self.offset);
        self.trace.check(&Event::Consume {
            side: self.side,
            count,
        });
        self.offset += count;
        if self.offset == chunk.len() {
            self.index += 1;
            self.offset = 0;
        }
    }
}

fn observation(result: io::Result<bool>, id: &str) -> Outcome {
    match result {
        Ok(true) => Outcome::Equal,
        Ok(false) => Outcome::Different,
        Err(error) => {
            let side = match error.to_string().as_str() {
                "left" => Side::Left,
                "right" => Side::Right,
                message => panic!("{id}: unexpected I/O cause {message:?}"),
            };
            let fault = match error.kind() {
                ErrorKind::Interrupted => Fault::Interrupted,
                ErrorKind::Other => Fault::IoError,
                kind => panic!("{id}: unexpected I/O kind {kind:?}"),
            };
            Outcome::Failed { side, fault }
        }
    }
}

#[test]
fn buffered_comparison_matches_lean_read_traces() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/progress-buffered-comparison.jsonl"
    ));
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let case: Case = serde_json::from_str(row).unwrap();
        let id = &case.id;
        assert_eq!(case.schema, 1, "{id}");
        assert_eq!(case.mode, "internal-fixture", "{id}");
        assert!(ids.insert(id.clone()), "{id}");
        let trace = ReadTrace {
            id,
            remaining: RefCell::new(case.events.iter()),
        };
        let mut left = ObservedReader::new(case.left, Side::Left, &trace);
        let mut right = ObservedReader::new(case.right, Side::Right, &trace);

        let actual = compare_buffered(&mut left, &mut right);

        assert_eq!(observation(actual, id), case.outcome, "{id}");
        assert!(trace.remaining.borrow().as_slice().is_empty(), "{id}");
    }
    assert_eq!(ids.len(), 324);
}
