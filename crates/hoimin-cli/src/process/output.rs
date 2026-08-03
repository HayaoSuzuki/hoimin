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
    mut receiver: mpsc::Receiver<Vec<u8>>,
) -> io::Result<OutputSpoolRef> {
    let mut first_error = None;
    let mut spool = match OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(path)
        .await
    {
        Ok(spool) => Some(spool),
        Err(error) => {
            first_error = Some(error);
            None
        }
    };
    let mut observed = 0_u64;
    let mut ring_position = 0_u64;
    while let Some(chunk) = receiver.recv().await {
        let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        observed = add_observed(observed, chunk_len);
        if let Some(file) = spool.as_mut() {
            match write_ring(file, max_retained, ring_position, &chunk).await {
                Ok(position) => ring_position = position,
                Err(error) => {
                    first_error = Some(error);
                    spool = None;
                }
            }
        }
    }
    let retained = observed.min(max_retained);
    if let Some(file) = spool.as_mut() {
        let finalized = if observed > max_retained {
            finalize_truncated(file, max_retained, ring_position).await
        } else {
            file.flush().await
        };
        if let Err(error) = finalized {
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
    use std::fs;
    use std::time::Duration;

    use super::{add_observed, collect_output, pipe_channel, retained_chunk_len};

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
