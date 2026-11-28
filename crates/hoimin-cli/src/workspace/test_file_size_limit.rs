/// A process-wide limit for use in an isolated fixture subprocess.
pub(super) struct FileSizeLimit {
    previous: libc::rlimit,
    previous_handler: libc::sighandler_t,
}

impl FileSizeLimit {
    pub(super) fn start(bytes: libc::rlim_t) -> Self {
        let mut previous = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: the kernel writes into a live rlimit value in this isolated subprocess.
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &raw mut previous) },
            0
        );
        // SAFETY: ignoring SIGXFSZ lets writes report EFBIG; the previous handler is restored.
        let previous_handler = unsafe { libc::signal(libc::SIGXFSZ, libc::SIG_IGN) };
        let reduced = libc::rlimit {
            rlim_cur: bytes,
            rlim_max: previous.rlim_max,
        };
        // SAFETY: change only this fixture subprocess's soft file-size limit.
        assert_eq!(
            unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &raw const reduced) },
            0
        );
        Self {
            previous,
            previous_handler,
        }
    }
}

impl Drop for FileSizeLimit {
    fn drop(&mut self) {
        // SAFETY: restore the captured process limit and signal handler before reporting results.
        unsafe {
            let _ = libc::setrlimit(libc::RLIMIT_FSIZE, &raw const self.previous);
            libc::signal(libc::SIGXFSZ, self.previous_handler);
        }
    }
}
