use std::io::{self, SeekFrom};

use camino::Utf8PathBuf;
use hoimin_core::OutputSpoolRef;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;

const PIPE_CHUNK_BYTES: usize = 8 * 1024;
const PIPE_CHANNEL_CHUNKS: usize = 8;
const TRUNCATION_MARKER: &[u8] = b"\n[... hoimin output truncated ...]\n";

pub(crate) fn pipe_channel() -> (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) {
    mpsc::channel(PIPE_CHANNEL_CHUNKS)
}

pub(crate) async fn drain_pipe<R>(
    mut reader: R,
    sender: mpsc::Sender<Vec<u8>>,
) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
{
    loop {
        let mut buffer = vec![0; PIPE_CHUNK_BYTES];
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Ok(());
        }
        buffer.truncate(read);
        if sender.send(buffer).await.is_err() {
            return Ok(());
        }
    }
}

pub(crate) async fn collect_output(
    path: Utf8PathBuf,
    token: String,
    max_retained: u64,
    receiver: mpsc::Receiver<Vec<u8>>,
) -> io::Result<OutputSpoolRef> {
    let spool = OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(path)
        .await
        .map(FileOutputSink);
    collect_output_with_sink(token, max_retained, receiver, spool).await
}

trait OutputSink: Send {
    async fn write_ring(&mut self, capacity: u64, position: u64, chunk: &[u8]) -> io::Result<u64>;

    async fn finalize(&mut self, capacity: u64, position: u64, truncated: bool) -> io::Result<()>;
}

struct FileOutputSink(File);

impl OutputSink for FileOutputSink {
    async fn write_ring(&mut self, capacity: u64, position: u64, chunk: &[u8]) -> io::Result<u64> {
        write_ring(&mut self.0, capacity, position, chunk).await
    }

    async fn finalize(&mut self, capacity: u64, position: u64, truncated: bool) -> io::Result<()> {
        if truncated {
            finalize_truncated(&mut self.0, capacity, position).await
        } else {
            self.0.flush().await
        }
    }
}

async fn collect_output_with_sink<S: OutputSink>(
    token: String,
    max_retained: u64,
    mut receiver: mpsc::Receiver<Vec<u8>>,
    spool: io::Result<S>,
) -> io::Result<OutputSpoolRef> {
    let (mut spool, mut first_error) = match spool {
        Ok(spool) => (Some(spool), None),
        Err(error) => (None, Some(error)),
    };
    let mut observed = 0_u64;
    let mut ring_position = 0_u64;
    while let Some(chunk) = receiver.recv().await {
        let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        observed = add_observed(observed, chunk_len);
        if let Some(file) = spool.as_mut() {
            match file.write_ring(max_retained, ring_position, &chunk).await {
                Ok(position) => ring_position = position,
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    spool = None;
                }
            }
        }
    }
    let retained = observed.min(max_retained);
    if let Some(file) = spool.as_mut() {
        let finalized = file
            .finalize(max_retained, ring_position, observed > max_retained)
            .await;
        if let Err(error) = finalized
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(OutputSpoolRef {
        token,
        retained,
        observed,
    })
}

async fn write_ring(
    file: &mut File,
    capacity: u64,
    position: u64,
    chunk: &[u8],
) -> io::Result<u64> {
    if capacity == 0 || chunk.is_empty() {
        return Ok(position);
    }
    let keep = retained_chunk_len(capacity, chunk.len());
    let skipped = chunk.len() - keep;
    let skipped = u64::try_from(skipped).unwrap_or(u64::MAX);
    let start = advance_ring(position, skipped, capacity);
    let first = retained_chunk_len(capacity - start, keep);

    file.seek(SeekFrom::Start(start)).await?;
    file.write_all(&chunk[chunk.len() - keep..chunk.len() - keep + first])
        .await?;
    if first < keep {
        file.seek(SeekFrom::Start(0)).await?;
        file.write_all(&chunk[chunk.len() - keep + first..]).await?;
    }

    let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
    Ok(advance_ring(position, chunk_len, capacity))
}

fn advance_ring(position: u64, amount: u64, capacity: u64) -> u64 {
    let amount = amount % capacity;
    if position >= capacity - amount {
        position - (capacity - amount)
    } else {
        position + amount
    }
}

async fn finalize_truncated(spool: &mut File, capacity: u64, ring_position: u64) -> io::Result<()> {
    if capacity == 0 {
        spool.set_len(0).await?;
        return spool.flush().await;
    }
    spool.flush().await?;
    let marker_len = u64::try_from(TRUNCATION_MARKER.len()).unwrap_or(u64::MAX);
    let marker_len = if capacity > marker_len { marker_len } else { 0 };
    let tail_len = capacity - marker_len;
    let tail_start = advance_ring(ring_position, marker_len, capacity);
    let staging = tempfile::tempfile()?;
    let mut staging = File::from_std(staging);

    if marker_len != 0 {
        staging.write_all(TRUNCATION_MARKER).await?;
    }
    copy_ring_range(spool, &mut staging, capacity, tail_start, tail_len).await?;
    staging.flush().await?;
    staging.seek(SeekFrom::Start(0)).await?;
    spool.set_len(0).await?;
    spool.seek(SeekFrom::Start(0)).await?;
    tokio::io::copy(&mut staging, spool).await?;
    spool.flush().await
}

async fn copy_ring_range(
    source: &mut File,
    destination: &mut File,
    capacity: u64,
    start: u64,
    length: u64,
) -> io::Result<()> {
    let first = length.min(capacity - start);
    source.seek(SeekFrom::Start(start)).await?;
    copy_exact_range(source, destination, first).await?;
    let remaining = length - first;
    if remaining != 0 {
        source.seek(SeekFrom::Start(0)).await?;
        copy_exact_range(source, destination, remaining).await?;
    }
    Ok(())
}

async fn copy_exact_range(
    source: &mut File,
    destination: &mut File,
    length: u64,
) -> io::Result<()> {
    let copied = tokio::io::copy(&mut (&mut *source).take(length), destination).await?;
    if copied == length {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "output ring was shorter than its retained byte count",
        ))
    }
}

fn retained_chunk_len(remaining: u64, chunk_len: usize) -> usize {
    usize::try_from(remaining)
        .unwrap_or(usize::MAX)
        .min(chunk_len)
}
fn add_observed(observed: u64, chunk_len: u64) -> u64 {
    observed.saturating_add(chunk_len)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::Duration;

    use serde::Deserialize;

    use super::{
        OutputSink, add_observed, advance_ring, collect_output, collect_output_with_sink,
        pipe_channel, retained_chunk_len,
    };

    struct FirstWriteFails {
        code: u64,
    }

    impl OutputSink for FirstWriteFails {
        fn write_ring(
            &mut self,
            _capacity: u64,
            _position: u64,
            _chunk: &[u8],
        ) -> impl std::future::Future<Output = std::io::Result<u64>> {
            std::future::ready(Err(std::io::Error::other(format!(
                "lean-error-{}",
                self.code
            ))))
        }

        fn finalize(
            &mut self,
            _capacity: u64,
            _position: u64,
            _truncated: bool,
        ) -> impl std::future::Future<Output = std::io::Result<()>> {
            std::future::poll_fn(|_| panic!("a failed sink must not be finalized"))
        }
    }

    const OUTPUT_RETENTION_CORPUS: &str =
        include_str!("../../../../formal/HoiminOracle/corpus/output-retention.jsonl");

    #[derive(Clone, Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct AuditCase {
        schema: u64,
        id: String,
        mode: String,
        scenario: String,
        capacity: u64,
        chunks: Vec<Vec<u8>>,
        observed_seed: u64,
        arithmetic_increment: u64,
        error_code: Option<u64>,
        expected_observed: u64,
        expected_retained: u64,
        expected_position: u64,
        expected_drained_chunks: u64,
        expected_bytes: Vec<u8>,
        expected_error_code: Option<u64>,
    }

    fn expected_contract() -> BTreeMap<&'static str, (&'static str, &'static str)> {
        [
            ("empty_zero", ("strict", "success")),
            ("empty_capacity", ("strict", "success")),
            ("fits_partitioned", ("strict", "success")),
            ("exact_capacity", ("strict", "success")),
            ("one_over_tiny", ("strict", "success")),
            ("marker_exact_boundary", ("strict", "success")),
            ("marker_plus_one", ("strict", "success")),
            ("recorded_receive_order", ("internal-fixture", "success")),
            ("large_chunk", ("internal-fixture", "success")),
            ("multiple_wraps", ("internal-fixture", "success")),
            ("partition_single", ("internal-fixture", "success")),
            ("partition_many", ("internal-fixture", "success")),
            (
                "create_failure_drains",
                ("internal-fixture", "create_error"),
            ),
            (
                "first_write_failure_drains",
                ("internal-fixture", "write_error"),
            ),
            ("observed_u64_saturation", ("model-only", "arithmetic")),
            ("fixture_timeout", ("infrastructure-error", "harness")),
        ]
        .into_iter()
        .collect()
    }

    fn validate_audit_case(case: &AuditCase) {
        let chunk_bytes = case.chunks.iter().fold(0_u64, |total, chunk| {
            total.saturating_add(u64::try_from(chunk.len()).unwrap())
        });
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(
            case.expected_retained,
            case.expected_observed.min(case.capacity),
            "{}",
            case.id
        );
        assert!(
            case.capacity == 0 || case.expected_position < case.capacity,
            "{}",
            case.id
        );
        match (case.mode.as_str(), case.scenario.as_str()) {
            ("strict" | "internal-fixture", "success") => {
                assert_eq!(case.observed_seed, 0, "{}", case.id);
                assert_eq!(case.arithmetic_increment, 0, "{}", case.id);
                assert_eq!(case.error_code, None, "{}", case.id);
                assert_eq!(case.expected_error_code, None, "{}", case.id);
                assert_eq!(case.expected_observed, chunk_bytes, "{}", case.id);
                assert_eq!(
                    u64::try_from(case.expected_bytes.len()).unwrap(),
                    case.expected_retained,
                    "{}",
                    case.id
                );
                assert_eq!(
                    u64::try_from(case.chunks.len()).unwrap(),
                    case.expected_drained_chunks,
                    "{}",
                    case.id
                );
            }
            ("internal-fixture", "create_error" | "write_error") => {
                assert!(!case.chunks.is_empty(), "{}", case.id);
                assert_eq!(case.observed_seed, 0, "{}", case.id);
                assert_eq!(case.arithmetic_increment, 0, "{}", case.id);
                assert!(case.error_code.is_some(), "{}", case.id);
                assert_eq!(case.error_code, case.expected_error_code, "{}", case.id);
                assert_eq!(case.expected_observed, chunk_bytes, "{}", case.id);
                assert_eq!(case.expected_position, 0, "{}", case.id);
                assert!(case.expected_bytes.is_empty(), "{}", case.id);
                assert_eq!(
                    u64::try_from(case.chunks.len()).unwrap(),
                    case.expected_drained_chunks,
                    "{}",
                    case.id
                );
            }
            ("model-only", "arithmetic") => {
                assert_eq!(case.capacity, 0, "{}", case.id);
                assert!(case.chunks.is_empty(), "{}", case.id);
                assert_eq!(case.error_code, None, "{}", case.id);
                assert_eq!(case.observed_seed, u64::MAX - 2, "{}", case.id);
                assert_eq!(case.arithmetic_increment, 8, "{}", case.id);
                assert_eq!(case.expected_observed, u64::MAX, "{}", case.id);
                assert_eq!(case.expected_retained, 0, "{}", case.id);
                assert_eq!(case.expected_position, 0, "{}", case.id);
                assert_eq!(case.expected_drained_chunks, 0, "{}", case.id);
                assert!(case.expected_bytes.is_empty(), "{}", case.id);
                assert_eq!(case.expected_error_code, None, "{}", case.id);
            }
            ("infrastructure-error", "harness") => {
                assert_eq!(case.capacity, 0, "{}", case.id);
                assert!(case.chunks.is_empty(), "{}", case.id);
                assert_eq!(case.observed_seed, 0, "{}", case.id);
                assert_eq!(case.arithmetic_increment, 0, "{}", case.id);
                assert_eq!(case.error_code, None, "{}", case.id);
                assert_eq!(case.expected_observed, 0, "{}", case.id);
                assert_eq!(case.expected_retained, 0, "{}", case.id);
                assert_eq!(case.expected_position, 0, "{}", case.id);
                assert_eq!(case.expected_drained_chunks, 0, "{}", case.id);
                assert!(case.expected_bytes.is_empty(), "{}", case.id);
                assert_eq!(case.expected_error_code, None, "{}", case.id);
            }
            _ => panic!("crossed mode/scenario in {}", case.id),
        }
    }

    fn parse_audit_corpus(input: &str) -> Vec<AuditCase> {
        let cases = input
            .lines()
            .enumerate()
            .map(|(index, line)| {
                serde_json::from_str::<AuditCase>(line)
                    .unwrap_or_else(|error| panic!("line {}: {error}", index + 1))
            })
            .collect::<Vec<_>>();
        let contract = cases
            .iter()
            .map(|case| {
                (
                    case.id.as_str(),
                    (case.mode.as_str(), case.scenario.as_str()),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(contract, expected_contract());
        assert_eq!(contract.len(), cases.len());
        cases.iter().for_each(validate_audit_case);
        cases
    }

    #[test]
    fn lean_output_retention_corpus_is_closed_and_typed() {
        assert_eq!(parse_audit_corpus(OUTPUT_RETENTION_CORPUS).len(), 16);
    }

    #[test]
    fn lean_output_retention_corpus_rejects_contract_drift() {
        let rows = OUTPUT_RETENTION_CORPUS
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();

        let mut unknown = rows.clone();
        unknown[0]["unexpected"] = serde_json::json!(true);
        assert!(std::panic::catch_unwind(|| parse_audit_corpus(&render_rows(unknown))).is_err());

        let mut crossed = rows.clone();
        crossed[0]["mode"] = serde_json::json!("model-only");
        assert!(std::panic::catch_unwind(|| parse_audit_corpus(&render_rows(crossed))).is_err());

        let mut duplicate = rows.clone();
        duplicate[0]["id"] = duplicate[1]["id"].clone();
        assert!(std::panic::catch_unwind(|| parse_audit_corpus(&render_rows(duplicate))).is_err());

        let mut unrelated = rows;
        unrelated[0]["error_code"] = serde_json::json!(7);
        assert!(std::panic::catch_unwind(|| parse_audit_corpus(&render_rows(unrelated))).is_err());

        let mut ignored_error_premise = OUTPUT_RETENTION_CORPUS
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        ignored_error_premise[12]["observed_seed"] = serde_json::json!(1);
        assert!(
            std::panic::catch_unwind(|| {
                parse_audit_corpus(&render_rows(ignored_error_premise))
            })
            .is_err()
        );

        let mut ignored_harness_output = OUTPUT_RETENTION_CORPUS
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        ignored_harness_output[15]["expected_position"] = serde_json::json!(1);
        assert!(
            std::panic::catch_unwind(|| {
                parse_audit_corpus(&render_rows(ignored_harness_output))
            })
            .is_err()
        );
    }

    fn render_rows(rows: Vec<serde_json::Value>) -> String {
        rows.into_iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn lean_success_rows_match_collect_output() {
        for case in parse_audit_corpus(OUTPUT_RETENTION_CORPUS)
            .into_iter()
            .filter(|case| case.scenario == "success")
        {
            let directory = tempfile::tempdir().unwrap();
            let path = camino::Utf8PathBuf::from_path_buf(directory.path().join("output")).unwrap();
            let (sender, receiver) = pipe_channel();
            for chunk in &case.chunks {
                sender.send(chunk.clone()).await.unwrap();
            }
            drop(sender);

            let output = collect_output(path.clone(), case.id.clone(), case.capacity, receiver)
                .await
                .unwrap();
            assert_eq!(output.observed, case.expected_observed, "{}", case.id);
            assert_eq!(output.retained, case.expected_retained, "{}", case.id);
            assert_eq!(fs::read(path).unwrap(), case.expected_bytes, "{}", case.id);
            assert_eq!(
                case.chunks.iter().fold(0, |position, chunk| {
                    advance_ring(position, u64::try_from(chunk.len()).unwrap(), case.capacity)
                }),
                case.expected_position,
                "{}",
                case.id
            );
            assert_eq!(
                u64::try_from(case.chunks.len()).unwrap(),
                case.expected_drained_chunks,
                "{}",
                case.id
            );
        }
    }

    #[test]
    fn lean_saturation_row_matches_owned_arithmetic_seam() {
        let case = parse_audit_corpus(OUTPUT_RETENTION_CORPUS)
            .into_iter()
            .find(|case| case.scenario == "arithmetic")
            .unwrap();
        assert_eq!(
            add_observed(case.observed_seed, case.arithmetic_increment),
            case.expected_observed
        );
    }

    #[tokio::test]
    async fn collector_drains_to_eof_and_keeps_first_write_error() {
        let (sender, receiver) = pipe_channel();
        let mut collector = tokio::spawn(collect_output_with_sink(
            "token".into(),
            8,
            receiver,
            Ok(FirstWriteFails { code: 11 }),
        ));

        for value in 0_u8..32 {
            tokio::time::timeout(Duration::from_secs(1), sender.send(vec![value]))
                .await
                .expect("collector continues draining")
                .expect("collector owns the receiver");
        }
        assert!(
            !collector.is_finished(),
            "collector waits for EOF after the error"
        );
        drop(sender);

        let error = tokio::time::timeout(Duration::from_secs(1), &mut collector)
            .await
            .expect("collector reaches EOF")
            .expect("collector task")
            .expect_err("first write fails");
        assert_eq!(error.to_string(), "lean-error-11");
    }

    #[tokio::test]
    async fn lean_fault_rows_keep_the_first_error_and_reach_eof() {
        for case in parse_audit_corpus(OUTPUT_RETENTION_CORPUS)
            .into_iter()
            .filter(|case| matches!(case.scenario.as_str(), "create_error" | "write_error"))
        {
            let code = case.expected_error_code.unwrap();
            let (sender, receiver) = pipe_channel();
            let collector = match case.scenario.as_str() {
                "create_error" => tokio::spawn(collect_output_with_sink::<FirstWriteFails>(
                    case.id.clone(),
                    case.capacity,
                    receiver,
                    Err(std::io::Error::other(format!("lean-error-{code}"))),
                )),
                "write_error" => tokio::spawn(collect_output_with_sink(
                    case.id.clone(),
                    case.capacity,
                    receiver,
                    Ok(FirstWriteFails { code }),
                )),
                _ => unreachable!(),
            };
            for chunk in &case.chunks {
                sender.send(chunk.clone()).await.unwrap();
            }
            drop(sender);
            let error = collector.await.unwrap().unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("lean-error-{code}"),
                "{}",
                case.id
            );
        }
    }

    #[tokio::test]
    async fn collector_drains_to_eof_after_spool_create_failure() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let invalid_spool_path = camino::Utf8PathBuf::from_path_buf(directory.path().to_owned())
            .expect("UTF-8 temporary path");
        let (sender, receiver) = pipe_channel();
        let mut collector = tokio::spawn(collect_output(
            invalid_spool_path,
            "token".into(),
            64,
            receiver,
        ));

        tokio::task::yield_now().await;
        assert!(
            !collector.is_finished(),
            "collector must wait for EOF after a spool error"
        );
        for _ in 0..32 {
            sender.send(vec![0; 16]).await.expect("collector drains");
        }
        drop(sender);

        let error = tokio::time::timeout(Duration::from_secs(1), &mut collector)
            .await
            .expect("collector reaches EOF")
            .expect("collector task")
            .expect_err("spool creation fails");
        assert_ne!(error.kind(), std::io::ErrorKind::WriteZero);
    }

    #[tokio::test]
    async fn collector_preserves_exact_output_when_it_fits() {
        let directory = tempfile::tempdir().unwrap();
        let path = camino::Utf8PathBuf::from_path_buf(directory.path().join("output")).unwrap();
        let (sender, receiver) = pipe_channel();
        sender.send(b"first".to_vec()).await.unwrap();
        sender.send(b"-last".to_vec()).await.unwrap();
        drop(sender);

        let output = collect_output(path.clone(), "token".into(), 64, receiver)
            .await
            .unwrap();

        assert_eq!(fs::read(path).unwrap(), b"first-last");
        assert_eq!(output.retained, 10);
        assert_eq!(output.observed, 10);
    }

    #[tokio::test]
    async fn collector_marks_truncation_and_retains_the_tail() {
        let directory = tempfile::tempdir().unwrap();
        let path = camino::Utf8PathBuf::from_path_buf(directory.path().join("output")).unwrap();
        let (sender, receiver) = pipe_channel();
        sender.send(b"HEAD-".to_vec()).await.unwrap();
        sender.send(vec![b'x'; 100]).await.unwrap();
        sender.send(b"-TAIL".to_vec()).await.unwrap();
        drop(sender);

        let output = collect_output(path.clone(), "token".into(), 64, receiver)
            .await
            .unwrap();
        let retained = fs::read(path).unwrap();

        assert_eq!(retained.len(), 64);
        assert!(retained.starts_with(b"\n[... hoimin output truncated ...]\n"));
        assert!(retained.ends_with(b"-TAIL"));
        assert!(!retained.starts_with(b"HEAD-"));
        assert_eq!(output.retained, 64);
        assert_eq!(output.observed, 110);
    }

    #[tokio::test]
    async fn collector_with_a_tiny_cap_retains_only_the_final_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = camino::Utf8PathBuf::from_path_buf(directory.path().join("output")).unwrap();
        let (sender, receiver) = pipe_channel();
        sender.send(b"head-tail".to_vec()).await.unwrap();
        drop(sender);

        let output = collect_output(path.clone(), "token".into(), 4, receiver)
            .await
            .unwrap();

        assert_eq!(fs::read(path).unwrap(), b"tail");
        assert_eq!(output.retained, 4);
        assert_eq!(output.observed, 9);
    }

    #[tokio::test]
    async fn collector_with_a_zero_cap_drains_without_retaining_output() {
        let directory = tempfile::tempdir().unwrap();
        let path = camino::Utf8PathBuf::from_path_buf(directory.path().join("output")).unwrap();
        let (sender, receiver) = pipe_channel();
        sender.send(b"discarded".to_vec()).await.unwrap();
        drop(sender);

        let output = collect_output(path.clone(), "token".into(), 0, receiver)
            .await
            .unwrap();

        assert_eq!(fs::read(path).unwrap(), b"");
        assert_eq!(output.retained, 0);
        assert_eq!(output.observed, 9);
    }

    #[test]
    fn observed_byte_count_saturates() {
        assert_eq!(add_observed(u64::MAX - 2, 8), u64::MAX);
    }

    #[test]
    fn retained_chunk_length_handles_a_remaining_limit_larger_than_usize() {
        assert_eq!(retained_chunk_len(u64::MAX, 16), 16);
    }
}
