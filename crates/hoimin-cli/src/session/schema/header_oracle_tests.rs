use std::fs;
use std::io::{self, ErrorKind, Read, Seek};
use std::slice;

use serde::Deserialize;

use super::{inspect_existing_database, read_wal_header};

const CORPUS: &str = include_str!("../../../../../formal/HoiminOracle/corpus/session-header.jsonl");
const READ_ERROR: &str = "session header test read error";

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    reads: Vec<Event>,
    file: Option<Vec<u8>>,
    outcome: Outcome,
    requests: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Event {
    Bytes { data: Vec<u8> },
    Failed { fault: Fault },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Fault {
    Interrupted,
    UnexpectedEof,
    PermissionDenied,
    InvalidData,
}

impl From<Fault> for ErrorKind {
    fn from(fault: Fault) -> Self {
        match fault {
            Fault::Interrupted => Self::Interrupted,
            Fault::UnexpectedEof => Self::UnexpectedEof,
            Fault::PermissionDenied => Self::PermissionDenied,
            Fault::InvalidData => Self::InvalidData,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Immutable,
    Ordinary,
    Failed { fault: Fault },
}

impl From<bool> for Outcome {
    fn from(wal: bool) -> Self {
        if wal { Self::Immutable } else { Self::Ordinary }
    }
}

fn classify(result: &io::Result<bool>, id: &str) -> Outcome {
    match result {
        Ok(wal) => (*wal).into(),
        Err(error) => {
            assert_eq!(error.to_string(), READ_ERROR, "{id} original I/O error");
            let fault = match error.kind() {
                ErrorKind::Interrupted => Fault::Interrupted,
                ErrorKind::UnexpectedEof => Fault::UnexpectedEof,
                ErrorKind::PermissionDenied => Fault::PermissionDenied,
                ErrorKind::InvalidData => Fault::InvalidData,
                unexpected => panic!("{id}: unexpected read error {unexpected:?}"),
            };
            Outcome::Failed { fault }
        }
    }
}

struct Script<'a> {
    events: slice::Iter<'a, Event>,
    pending: &'a [u8],
    requests: Vec<usize>,
    id: &'a str,
}

impl Read for Script<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        assert!(!buffer.is_empty(), "{} empty read request", self.id);
        self.requests.push(buffer.len());
        if self.pending.is_empty() {
            let next = self
                .events
                .next()
                .unwrap_or_else(|| panic!("{} read after the end of the script", self.id));
            match next {
                Event::Bytes { data } => self.pending = data,
                Event::Failed { fault } => return Err(io::Error::new((*fault).into(), READ_ERROR)),
            }
        }
        let count = buffer.len().min(self.pending.len());
        let (copied, pending) = self.pending.split_at(count);
        buffer[..count].copy_from_slice(copied);
        self.pending = pending;
        Ok(count)
    }
}

fn check_file(data: &[u8], case: &Case) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    fs::write(&path, data).unwrap();
    let routed =
        inspect_existing_database(&path).unwrap_or_else(|error| panic!("{}: {error}", case.id));
    assert_eq!(Outcome::from(routed), case.outcome, "{} path", case.id);

    let mut file = fs::File::open(&path).unwrap();
    let read = read_wal_header(&mut file).unwrap();
    assert_eq!(Outcome::from(read), case.outcome, "{} file", case.id);
    assert_eq!(
        file.stream_position().unwrap(),
        data.len().min(case.requests[0]) as u64,
        "{} file cursor",
        case.id
    );
    assert_eq!(fs::read(&path).unwrap(), data, "{} file contents", case.id);
}

#[test]
fn session_header_matches_lean_oracle() {
    let mut files = 0;
    let mut streams = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        let mut script = Script {
            events: case.reads.iter(),
            pending: &[],
            requests: Vec::new(),
            id: &case.id,
        };
        let observed = classify(&read_wal_header(&mut script), &case.id);
        assert_eq!(observed, case.outcome, "{} outcome", case.id);
        assert_eq!(script.requests, case.requests, "{} reads", case.id);
        if let Some(data) = &case.file {
            check_file(data, &case);
            files += 1;
        } else {
            streams += 1;
        }
    }
    assert!(files > 0 && streams > 0, "incomplete header oracle");
}
