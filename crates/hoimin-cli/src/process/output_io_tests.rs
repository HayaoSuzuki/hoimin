use super::*;
use std::pin::Pin;
use std::task::{Context, Poll};

struct ObservedFile {
    file: File,
    seeks: usize,
    fail_seek_at: Option<usize>,
}
impl AsyncSeek for ObservedFile {
    fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
        self.seeks += 1;
        if self.fail_seek_at == Some(self.seeks) {
            return Err(io::Error::other("required-seek-failed"));
        }
        Pin::new(&mut self.file).start_seek(position)
    }
    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        Pin::new(&mut self.file).poll_complete(cx)
    }
}
impl AsyncWrite for ObservedFile {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.file).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.file).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.file).poll_shutdown(cx)
    }
}

#[tokio::test]
async fn aligned_ring_writes_avoid_redundant_seeks() {
    for (capacity, lengths, expected_seeks) in [
        (256, vec![8; 16], 1),
        (16, vec![8; 4], 2),
        (16, vec![10, 10], 2),
        (16, vec![4, 40, 4], 3),
        (0, vec![8; 16], 0),
        (16, vec![0, 0], 0),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ring");
        let file = File::create(&path).await.unwrap();
        let mut file = ObservedFile {
            file,
            seeks: 0,
            fail_seek_at: None,
        };
        let mut position = 0;
        let mut expected = Vec::new();
        let mut cursor = 0;
        for (i, length) in lengths.iter().enumerate() {
            let chunk = vec![u8::try_from(i).unwrap(); *length];
            position = write_ring(&mut file, capacity, position, &chunk)
                .await
                .unwrap();
            for byte in chunk {
                if capacity > 0 {
                    if cursor == expected.len() {
                        expected.push(byte);
                    } else {
                        expected[cursor] = byte;
                    }
                    cursor = (cursor + 1) % usize::try_from(capacity).unwrap();
                }
            }
        }
        file.flush().await.unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            expected,
            "capacity={capacity} lengths={lengths:?}"
        );
        assert_eq!(
            file.seeks, expected_seeks,
            "capacity={capacity} lengths={lengths:?}"
        );
    }
}

#[tokio::test]
async fn required_wrap_and_skipped_offset_seek_errors_propagate() {
    for lengths in [[6, 6], [6, 12]] {
        let directory = tempfile::tempdir().unwrap();
        let mut file = ObservedFile {
            file: File::create(directory.path().join("ring")).await.unwrap(),
            seeks: 0,
            fail_seek_at: Some(2),
        };
        let position = write_ring(&mut file, 8, 0, &vec![1; lengths[0]])
            .await
            .unwrap();
        let error = write_ring(&mut file, 8, position, &vec![2; lengths[1]])
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "required-seek-failed");
    }
}

#[tokio::test]
async fn deferred_file_write_error_still_drains_to_eof() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("readonly");
    std::fs::write(&path, b"unchanged").unwrap();
    let file = File::open(&path).await.unwrap();
    let (sender, receiver) = pipe_channel();
    let collector = tokio::spawn(collect_output_with_sink(
        "test".into(),
        4096,
        receiver,
        Ok(FileOutputSink {
            file,
            staging_path: Utf8PathBuf::from_path_buf(directory.path().join("staging")).unwrap(),
        }),
    ));
    for _ in 0..32 {
        tokio::time::timeout(std::time::Duration::from_secs(2), sender.send(vec![1; 8]))
            .await
            .unwrap()
            .unwrap();
    }
    assert!(!collector.is_finished(), "error is returned only after EOF");
    drop(sender);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), collector)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"unchanged");
}

async fn pending_error(file: &mut File, baseline: bool) -> io::Error {
    let mut position = 0;
    for _ in 0..32 {
        let result = if baseline {
            async {
                file.seek(SeekFrom::Start(position)).await?;
                file.write_all(b"payload!").await?;
                Ok::<_, io::Error>(position + 8)
            }
            .await
        } else {
            write_ring(file, 4096, position, b"payload!").await
        };
        match result {
            Ok(next) => position = next,
            Err(error) => return error,
        }
    }
    panic!("read-only writes must fail");
}

#[tokio::test]
async fn aligned_pending_write_error_matches_baseline_diagnostic() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("readonly");
    std::fs::write(&path, b"unchanged").unwrap();
    let baseline = pending_error(&mut File::open(&path).await.unwrap(), true).await;
    let actual = pending_error(&mut File::open(&path).await.unwrap(), false).await;
    assert_eq!(actual.kind(), baseline.kind());
    assert_eq!(actual.raw_os_error(), baseline.raw_os_error());
    assert_eq!(actual.to_string(), baseline.to_string());
}
