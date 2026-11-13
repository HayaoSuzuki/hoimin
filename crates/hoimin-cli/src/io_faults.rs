//! One-shot, thread-local I/O faults for unit tests. Coverage builds retain the same
//! boundary shape; only unit tests can arm faults, and a guard clears unused faults.
#[cfg(any(test, coverage_nightly))]
mod enabled {
    use std::cell::RefCell;
    use std::fs::File;
    use std::io::{self, Write};

    struct Fault {
        point: &'static str,
        skip: usize,
        action: Box<dyn FnOnce() -> io::Result<()>>,
    }

    thread_local! {
        static PENDING: RefCell<Option<Fault>> = const { RefCell::new(None) };
    }

    #[cfg(test)]
    pub(crate) struct Guard(&'static str);

    #[cfg(test)]
    pub(crate) fn arm(point: &'static str) -> Guard {
        arm_after(point, 0)
    }

    #[cfg(test)]
    pub(crate) fn arm_after(point: &'static str, skip: usize) -> Guard {
        arm_action(point, skip, move || {
            Err(io::Error::other(format!(
                "injected owned I/O failure: {point}"
            )))
        })
    }

    #[cfg(test)]
    pub(crate) fn arm_action(
        point: &'static str,
        skip: usize,
        action: impl FnOnce() -> io::Result<()> + 'static,
    ) -> Guard {
        PENDING.with(|pending| {
            assert!(
                pending
                    .replace(Some(Fault {
                        point,
                        skip,
                        action: Box::new(action)
                    }))
                    .is_none(),
                "nested I/O fault guard"
            );
        });
        Guard(point)
    }

    #[cfg(test)]
    impl Guard {
        pub(crate) fn triggered(&self) -> bool {
            PENDING.with(|pending| pending.borrow().as_ref().map(|fault| fault.point))
                != Some(self.0)
        }
    }

    #[cfg(test)]
    impl Drop for Guard {
        fn drop(&mut self) {
            PENDING.with(|pending| drop(pending.take()));
        }
    }

    pub(crate) fn check(point: &'static str) -> io::Result<()> {
        let action = PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            if let Some(fault) = pending.as_mut()
                && fault.point == point
            {
                if fault.skip != 0 {
                    fault.skip -= 1;
                } else {
                    return pending.take().map(|fault| fault.action);
                }
            }
            None
        });
        match action {
            Some(action) => action(),
            None => Ok(()),
        }
    }

    pub(crate) struct Writer<'a> {
        file: &'a mut File,
        point: &'static str,
    }

    impl<'a> Writer<'a> {
        pub(crate) const fn new(file: &'a mut File, point: &'static str) -> Self {
            Self { file, point }
        }
    }

    impl Write for Writer<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            check(self.point)?;
            self.file.write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }
}

#[cfg(any(test, coverage_nightly))]
pub(crate) use enabled::{Writer, check};
#[cfg(test)]
pub(crate) use enabled::{arm, arm_action, arm_after};

// Keep instrumented library and test expression shapes identical for LLVM's
// per-function region maps. Ordinary builds evaluate the original I/O directly.
macro_rules! checked_io {
    ($point:literal, $operation:expr) => {{
        #[cfg(any(test, coverage_nightly))]
        {
            $crate::io_faults::check($point).and_then(|()| $operation)
        }
        #[cfg(not(any(test, coverage_nightly)))]
        {
            $operation
        }
    }};
    ($point:literal, $operation:expr, $error:path) => {{
        #[cfg(any(test, coverage_nightly))]
        {
            $crate::io_faults::check($point)
                .map_err($error)
                .and_then(|()| $operation)
        }
        #[cfg(not(any(test, coverage_nightly)))]
        {
            $operation
        }
    }};
}

pub(crate) use checked_io;
