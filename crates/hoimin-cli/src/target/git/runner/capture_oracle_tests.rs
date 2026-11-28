use super::{CaptureError, STDERR_TRUNCATED, read_pipe};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Event {
    Block { byte: u8, count: usize },
    IoError,
}

struct FixturePipe {
    events: Vec<Event>,
    next: usize,
    offset: usize,
    started: usize,
}

impl AsyncRead for FixturePipe {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.offset == 0 {
            self.started += 1;
        }
        match self.events.get(self.next).copied() {
            Some(Event::Block { byte, count }) => {
                assert!(count > 0);
                let length = (count - self.offset).min(output.remaining());
                output.initialize_unfilled_to(length).fill(byte);
                output.advance(length);
                self.offset += length;
                if self.offset == count {
                    self.next += 1;
                    self.offset = 0;
                }
                Poll::Ready(Ok(()))
            }
            Some(Event::IoError) => Poll::Ready(Err(io::Error::other("fixture pipe I/O failure"))),
            None => Poll::Ready(Ok(())),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Stream {
    Stdout,
    Stderr,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Ok { runs: Vec<(u8, usize)> },
    Overflow,
    IoError,
}

#[derive(Deserialize)]
struct Expected {
    reads: usize,
    outcome: Outcome,
}

#[derive(Deserialize)]
struct Fixture {
    id: String,
    schema: u32,
    mode: String,
    stream: Stream,
    limit: usize,
    input: Vec<Event>,
    expected: Expected,
}

#[tokio::test]
async fn git_pipe_capture_matches_lean() {
    let mut ids = BTreeSet::new();
    let corpus = include_str!("../../../../../../formal/HoiminOracle/corpus/git-capture.jsonl");
    for line in corpus.lines() {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);
        let mut pipe = FixturePipe {
            events: fixture.input,
            next: 0,
            offset: 0,
            started: 0,
        };
        let diagnostic = matches!(fixture.stream, Stream::Stderr);
        let actual = read_pipe(&mut pipe, fixture.limit, diagnostic).await;
        assert_eq!(pipe.started, fixture.expected.reads, "{}", fixture.id);
        match (fixture.expected.outcome, actual) {
            (Outcome::Ok { runs }, Ok(captured)) => {
                let bytes: Vec<_> = runs
                    .into_iter()
                    .flat_map(|(byte, count)| std::iter::repeat_n(byte, count))
                    .collect();
                assert_eq!(captured, bytes, "{}", fixture.id);
                let ceiling = fixture.limit
                    + if diagnostic {
                        STDERR_TRUNCATED.len()
                    } else {
                        0
                    };
                assert!(captured.capacity() <= ceiling, "{}", fixture.id);
            }
            (Outcome::Overflow, Err(CaptureError::StdoutLimit(limit))) => {
                assert_eq!(limit, fixture.limit, "{}", fixture.id);
            }
            (Outcome::IoError, Err(CaptureError::Io(error))) => {
                assert_eq!(
                    error.to_string(),
                    "fixture pipe I/O failure",
                    "{}",
                    fixture.id
                );
            }
            (_, other) => panic!("{}: unexpected capture outcome: {other:?}", fixture.id),
        }
        assert!(ids.insert(fixture.id));
    }
    assert_eq!(ids.len(), 858);
}
