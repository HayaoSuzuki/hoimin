use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use serde::Deserialize;

use super::{ResourceError, cleanup_error, parse_member_pids, signal_verified_member};

const CORPUS: &str = include_str!("../../../../../formal/HoiminOracle/corpus/cgroup-member.jsonl");
const CGROUP: &str = "/fixture/owned-cgroup";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    pid: u32,
    handle: u64,
    open_error: Option<Errno>,
    membership: Membership,
    signal_error: Option<Errno>,
    trace: Vec<Event>,
    failure: Option<Failure>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Errno {
    NoSuchProcess,
    Permission,
    Unsupported,
    TooManyFiles,
    Invalid,
    Io,
}

impl Errno {
    fn code(self) -> i32 {
        match self {
            Self::NoSuchProcess => libc::ESRCH,
            Self::Permission => libc::EPERM,
            Self::Unsupported => libc::ENOSYS,
            Self::TooManyFiles => libc::EMFILE,
            Self::Invalid => libc::EINVAL,
            Self::Io => libc::EIO,
        }
    }

    fn from_code(code: i32) -> Self {
        match code {
            libc::ESRCH => Self::NoSuchProcess,
            libc::EPERM => Self::Permission,
            libc::ENOSYS => Self::Unsupported,
            libc::EMFILE => Self::TooManyFiles,
            libc::EINVAL => Self::Invalid,
            libc::EIO => Self::Io,
            _ => panic!("unexpected errno: {code}"),
        }
    }

    fn error(self) -> io::Error {
        io::Error::from_raw_os_error(self.code())
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Membership {
    Present,
    Absent,
    Unreadable,
    Malformed,
    InvalidUtf8,
    DecimalOverflow,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum Event {
    OpenHandle(u64),
    ReadMembership,
    Signal(u64),
    Release(u64),
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", content = "errno", rename_all = "snake_case")]
enum Failure {
    PidOverflow,
    InvalidMembership,
    Syscall(Errno),
}

struct HeldHandle<'a> {
    id: u64,
    trace: &'a RefCell<Vec<Event>>,
}

impl Drop for HeldHandle<'_> {
    fn drop(&mut self) {
        self.trace.borrow_mut().push(Event::Release(self.id));
    }
}

fn read_membership(
    path: &Path,
    pid: u32,
    membership: Membership,
) -> Result<Vec<u32>, ResourceError> {
    let unrelated = pid ^ 1;
    let bytes = match membership {
        Membership::Present => format!("  {unrelated}\t{pid}\r\n{pid}\n").into_bytes(),
        Membership::Absent => format!("{unrelated}\n").into_bytes(),
        Membership::Unreadable => return Err(cleanup_error(path, Errno::Io.error())),
        Membership::Malformed => format!("{pid}\nnot-a-pid\n").into_bytes(),
        Membership::InvalidUtf8 => {
            let mut bytes = format!("{pid}\n").into_bytes();
            bytes.push(0xff);
            bytes
        }
        Membership::DecimalOverflow => format!("{pid}\n4294967296\n").into_bytes(),
    };
    parse_member_pids(&bytes)
}

fn failure(result: Result<(), ResourceError>, expected_path: &Path) -> Option<Failure> {
    match result.err()? {
        ResourceError::CgroupCleanup { path, source } => {
            assert_eq!(path, expected_path);
            if let Some(code) = source.raw_os_error() {
                Some(Failure::Syscall(Errno::from_code(code)))
            } else {
                assert_eq!(source.kind(), io::ErrorKind::Other);
                assert_eq!(source.to_string(), "cgroup pid overflow");
                Some(Failure::PidOverflow)
            }
        }
        ResourceError::InvalidCgroupData(message) => {
            assert_eq!(message, "invalid cgroup.procs pid");
            Some(Failure::InvalidMembership)
        }
        error => panic!("unexpected member cleanup error: {error:?}"),
    }
}

fn replay(case: &Case) {
    let path = Path::new(CGROUP);
    let trace = RefCell::new(Vec::new());
    let result = signal_verified_member(
        path,
        case.pid,
        |pid| {
            trace
                .borrow_mut()
                .push(Event::OpenHandle(u64::try_from(pid).unwrap()));
            case.open_error.map_or_else(
                || {
                    Ok(HeldHandle {
                        id: case.handle,
                        trace: &trace,
                    })
                },
                |error| Err(error.error()),
            )
        },
        |observed_path| {
            trace.borrow_mut().push(Event::ReadMembership);
            assert_eq!(observed_path, path, "{}", case.id);
            read_membership(observed_path, case.pid, case.membership)
        },
        |handle| {
            trace.borrow_mut().push(Event::Signal(handle.id));
            case.signal_error.map_or(Ok(()), |error| Err(error.error()))
        },
    );
    assert_eq!(trace.into_inner(), case.trace, "{}", case.id);
    assert_eq!(failure(result, path), case.failure, "{}", case.id);
}

#[test]
fn cgroup_member_recheck_matches_lean_oracle() {
    let mut ids = BTreeSet::new();
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        replay(&case);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 39);
}
