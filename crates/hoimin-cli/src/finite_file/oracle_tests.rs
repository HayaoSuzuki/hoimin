use super::{SnapshotReader, is_snapshot_length_change};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, ErrorKind, Read, Seek},
};

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    mode: String,
    id: String,
    length: u64,
    calls: Vec<Call>,
    expected: Vec<Expected>,
}

#[derive(serde::Deserialize)]
struct Call {
    capacity: usize,
    reply: Reply,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Reply {
    Available { bytes: usize },
    Interrupted,
    IoError,
}

#[derive(serde::Deserialize)]
struct Expected {
    requested: Option<usize>,
    consumed: u64,
    #[serde(flatten)]
    result: ObservedRead,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct ObservedRead {
    remaining: u64,
    outcome: Outcome,
    delivered: usize,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    EmptyRequest,
    Data,
    Eof,
    Grew,
    Shrank,
    Interrupted,
    IoError,
}

struct ScriptedRead {
    reply: Reply,
    requested: Option<usize>,
    consumed: usize,
}

impl ScriptedRead {
    fn new(reply: Reply) -> Self {
        Self {
            reply,
            requested: None,
            consumed: 0,
        }
    }
}

impl Read for ScriptedRead {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        assert!(self.requested.replace(output.len()).is_none());
        match self.reply {
            Reply::Available { bytes } => {
                let count = output.len().min(bytes);
                output[..count].fill(0);
                self.consumed = count;
                Ok(count)
            }
            Reply::Interrupted => Err(ErrorKind::Interrupted.into()),
            // This has the same kind as a detected shrink, with a different cause.
            Reply::IoError => Err(ErrorKind::UnexpectedEof.into()),
        }
    }
}

fn observe(reader: &mut SnapshotReader<impl Read>, capacity: usize, context: &str) -> ObservedRead {
    let mut buffer = vec![42; capacity];
    let result = reader.read(&mut buffer);
    let (outcome, delivered) = match result {
        Ok(0) if capacity == 0 => (Outcome::EmptyRequest, 0),
        Ok(0) => (Outcome::Eof, 0),
        Ok(count) => (Outcome::Data, count),
        Err(error) if is_snapshot_length_change(&error) => match error.kind() {
            ErrorKind::InvalidData => (Outcome::Grew, 0),
            ErrorKind::UnexpectedEof => (Outcome::Shrank, 0),
            kind => panic!("{context}: unexpected length-change error kind {kind:?}"),
        },
        Err(error) if error.kind() == ErrorKind::Interrupted => (Outcome::Interrupted, 0),
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => (Outcome::IoError, 0),
        Err(error) => panic!("{context}: unexpected read error {error:?}"),
    };
    assert!(
        buffer[..delivered].iter().all(|&byte| byte == 0),
        "{context}"
    );
    assert!(
        buffer[delivered..].iter().all(|&byte| byte == 42),
        "{context}"
    );
    ObservedRead {
        remaining: reader.remaining,
        outcome,
        delivered,
    }
}

fn compare_scripted(case: &Case) {
    let mut reader = SnapshotReader {
        file: ScriptedRead::new(Reply::Interrupted),
        remaining: case.length,
    };
    for (step, (call, expected)) in case.calls.iter().zip(&case.expected).enumerate() {
        let context = format!("{} scripted step {step}", case.id);
        reader.file = ScriptedRead::new(call.reply);

        let actual = observe(&mut reader, call.capacity, &context);

        assert_eq!(actual, expected.result, "{context}");
        assert_eq!(reader.file.requested, expected.requested, "{context}");
        assert_eq!(reader.file.consumed as u64, expected.consumed, "{context}");
    }
}

fn compare_file(case: &Case) {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(case.length).unwrap();
    let mut reader = SnapshotReader::new(File::open(file.path()).unwrap()).unwrap();
    for (step, (call, expected)) in case.calls.iter().zip(&case.expected).enumerate() {
        let context = format!("{} file step {step}", case.id);
        let Reply::Available { bytes } = call.reply else {
            unreachable!("file cases contain only available bytes");
        };
        let before = reader.file.stream_position().unwrap();
        file.as_file().set_len(before + bytes as u64).unwrap();

        let actual = observe(&mut reader, call.capacity, &context);
        let after = reader.file.stream_position().unwrap();

        assert_eq!(actual, expected.result, "{context}");
        assert_eq!(after - before, expected.consumed, "{context}");
    }
}

#[test]
fn descriptor_length_reads_match_lean_traces() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/snapshot-reads.jsonl"
    ));
    let mut ids = BTreeSet::new();
    let mut file_cases = 0;
    for row in corpus.lines() {
        let case: Case = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        assert_eq!(case.calls.len(), case.expected.len(), "{}", case.id);

        compare_scripted(&case);
        if case.length <= 5
            && case
                .calls
                .iter()
                .all(|call| matches!(call.reply, Reply::Available { .. }))
        {
            compare_file(&case);
            file_cases += 1;
        }
    }
    assert_eq!(ids.len(), 364);
    assert_eq!(file_cases, 122);
}
