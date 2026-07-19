use camino::Utf8PathBuf;
use hoimin_core::OutputSpoolRef;
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

const PIPE_CHUNK_BYTES: usize = 8 * 1024;
const PIPE_CHANNEL_CHUNKS: usize = 8;

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
) -> std::io::Result<OutputSpoolRef> {
    let mut first_error = None;
    let mut spool = match File::create(path).await {
        Ok(spool) => Some(spool),
        Err(error) => {
            first_error = Some(error);
            None
        }
    };
    let mut retained = 0_u64;
    let mut observed = 0_u64;
    while let Some(chunk) = receiver.recv().await {
        let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        observed = add_observed(observed, chunk_len);
        let remaining = max_retained.saturating_sub(retained);
        let keep = retained_chunk_len(remaining, chunk.len());
        if keep != 0
            && let Some(file) = spool.as_mut()
        {
            match file.write_all(&chunk[..keep]).await {
                Ok(()) => retained = retained.saturating_add(keep as u64),
                Err(error) => {
                    first_error = Some(error);
                    spool = None;
                }
            }
        }
    }
    if let Some(file) = spool.as_mut()
        && let Err(error) = file.flush().await
    {
        first_error = Some(error);
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

    #[test]
    fn observed_byte_count_saturates() {
        assert_eq!(add_observed(u64::MAX - 2, 8), u64::MAX);
    }

    #[test]
    fn retained_chunk_length_handles_a_remaining_limit_larger_than_usize() {
        assert_eq!(retained_chunk_len(u64::MAX, 16), 16);
    }
}
