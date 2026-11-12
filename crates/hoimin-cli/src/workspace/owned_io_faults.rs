//! One-shot faults at owned-workspace I/O boundaries. Production calls remain direct.
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

pub(super) struct Guard(&'static str);

pub(super) fn arm(point: &'static str) -> Guard {
    arm_after(point, 0)
}

pub(super) fn arm_after(point: &'static str, skip: usize) -> Guard {
    arm_action(point, skip, move || {
        Err(io::Error::other(format!(
            "injected owned I/O failure: {point}"
        )))
    })
}

pub(super) fn arm_action(
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

impl Guard {
    pub(super) fn triggered(&self) -> bool {
        PENDING.with(|pending| pending.borrow().as_ref().map(|fault| fault.point)) != Some(self.0)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        PENDING.with(|pending| drop(pending.take()));
    }
}

pub(super) fn check(point: &'static str) -> io::Result<()> {
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

pub(super) struct Writer<'a> {
    file: &'a mut File,
    point: &'static str,
}

impl<'a> Writer<'a> {
    pub(super) const fn new(file: &'a mut File, point: &'static str) -> Self {
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
