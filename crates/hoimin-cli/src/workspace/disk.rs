//! Bounded disk measurement for Hoimin-owned workspace roots.

use std::collections::BTreeMap;
#[cfg(unix)]
use std::collections::BTreeSet;
use std::io;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};

const MAX_TREE_ENTRIES: usize = 250_000;
const MAX_TREE_DEPTH: usize = 128;
const MAX_SCAN_DURATION: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct FilesystemKey(pub(crate) u64);

#[derive(Debug)]
pub(crate) struct RootCapability {
    pub(crate) dir: cap_std::fs::Dir,
    pub(crate) display_path: Utf8PathBuf,
}

impl RootCapability {
    #[cfg(all(test, unix))]
    pub(crate) fn open(path: &Utf8Path) -> io::Result<Self> {
        let dir = cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority())?;
        Ok(Self {
            dir,
            display_path: path.to_owned(),
        })
    }

    #[cfg(all(test, windows))]
    pub(crate) fn open(path: &Utf8Path) -> io::Result<Self> {
        Ok(Self {
            dir: open_windows_meter_root(path, None)?,
            display_path: path.to_owned(),
        })
    }

    #[cfg(unix)]
    pub(crate) fn from_dir(dir: &cap_std::fs::Dir, display_path: Utf8PathBuf) -> io::Result<Self> {
        Ok(Self {
            dir: dir.try_clone()?,
            display_path,
        })
    }

    #[cfg(windows)]
    pub(crate) fn from_dir(dir: &cap_std::fs::Dir, display_path: Utf8PathBuf) -> io::Result<Self> {
        Ok(Self {
            dir: dir.try_clone()?,
            display_path,
        })
    }
}

pub(crate) trait AvailableSpace: Send + Sync {
    fn available(&self, root: &RootCapability) -> io::Result<u64>;
    fn filesystem_key(&self, root: &RootCapability) -> io::Result<FilesystemKey>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemAvailableSpace;

#[cfg(unix)]
impl AvailableSpace for SystemAvailableSpace {
    fn available(&self, root: &RootCapability) -> io::Result<u64> {
        use std::mem::MaybeUninit;
        use std::os::fd::AsRawFd;

        let file = root.dir.try_clone()?.into_std_file();
        let mut value = MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: value is a writable statvfs buffer and file remains open for this call.
        if unsafe { libc::fstatvfs(file.as_raw_fd(), value.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fstatvfs succeeded and initialized the entire structure.
        let value = unsafe { value.assume_init() };
        let available_blocks = statvfs_value_to_u64(value.f_bavail);
        let fragment_size = statvfs_value_to_u64(value.f_frsize);
        available_blocks
            .checked_mul(fragment_size)
            .ok_or_else(|| io::Error::other("available-space overflow"))
    }

    fn filesystem_key(&self, root: &RootCapability) -> io::Result<FilesystemKey> {
        use cap_fs_ext::MetadataExt;

        root.dir
            .metadata(".")
            .map(|metadata| FilesystemKey(metadata.dev()))
    }
}

#[cfg(unix)]
#[allow(
    clippy::useless_conversion,
    reason = "libc statvfs fields vary between Unix ABIs"
)]
fn statvfs_value_to_u64<T: Into<u64>>(value: T) -> u64 {
    value.into()
}

#[cfg(windows)]
impl AvailableSpace for SystemAvailableSpace {
    fn available(&self, root: &RootCapability) -> io::Result<u64> {
        use std::os::windows::io::AsRawHandle;

        use windows_sys::Win32::Storage::FileSystem::{
            GetDiskFreeSpaceExW, GetFinalPathNameByHandleW, GetVolumeInformationW,
            GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        };

        let file = root.dir.try_clone()?.into_std_file();
        let expected_volume = super::root::windows::file_identity_io(&file)?.0;
        let handle = file.as_raw_handle();
        // SAFETY: a null buffer with zero length requests the required UTF-16 length.
        let required = unsafe { GetFinalPathNameByHandleW(handle, std::ptr::null_mut(), 0, 0) };
        if required == 0 {
            return Err(io::Error::last_os_error());
        }
        let capacity = required
            .checked_add(1)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| io::Error::other("final path length overflow"))?;
        let mut final_path = vec![0_u16; capacity];
        // SAFETY: final_path is writable for its declared capacity and handle remains open.
        let written =
            unsafe { GetFinalPathNameByHandleW(handle, final_path.as_mut_ptr(), required + 1, 0) };
        if written == 0 || written > required {
            return Err(io::Error::last_os_error());
        }
        final_path.truncate(
            usize::try_from(written).map_err(|_| io::Error::other("final path overflow"))?,
        );
        final_path.push(0);
        let mut volume_path = vec![0_u16; 32_768];
        // SAFETY: both UTF-16 buffers are NUL-terminated/writable for their declared lengths.
        if unsafe {
            GetVolumePathNameW(
                final_path.as_ptr(),
                volume_path.as_mut_ptr(),
                u32::try_from(volume_path.len()).expect("fixed Windows path capacity"),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut volume_name = [0_u16; 128];
        // SAFETY: volume_path is NUL-terminated and volume_name is writable.
        if unsafe {
            GetVolumeNameForVolumeMountPointW(
                volume_path.as_ptr(),
                volume_name.as_mut_ptr(),
                u32::try_from(volume_name.len()).expect("fixed volume-name capacity"),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let volume_serial = |path: *const u16| -> io::Result<u64> {
            let mut serial = 0_u32;
            // SAFETY: path is a NUL-terminated volume GUID and serial is writable.
            if unsafe {
                GetVolumeInformationW(
                    path,
                    std::ptr::null_mut(),
                    0,
                    &raw mut serial,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                )
            } == 0
            {
                Err(io::Error::last_os_error())
            } else {
                Ok(u64::from(serial))
            }
        };
        if volume_serial(volume_name.as_ptr())? != expected_volume {
            return Err(io::Error::other(
                "capacity volume does not match the open managed-root handle",
            ));
        }
        let mut available = 0_u64;
        // SAFETY: volume_name is a NUL-terminated volume GUID and output is writable.
        if unsafe {
            GetDiskFreeSpaceExW(
                volume_name.as_ptr(),
                &raw mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if volume_serial(volume_name.as_ptr())? != expected_volume {
            return Err(io::Error::other(
                "capacity volume identity changed during the query",
            ));
        }
        Ok(available)
    }

    fn filesystem_key(&self, root: &RootCapability) -> io::Result<FilesystemKey> {
        let file = root.dir.try_clone()?.into_std_file();
        super::root::windows::file_identity_io(&file).map(|(volume, _)| FilesystemKey(volume))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MeterReading {
    pub(crate) owned_bytes: u64,
    pub(crate) available_by_filesystem: BTreeMap<FilesystemKey, u64>,
    pub(crate) conservative_entries: bool,
    pub(crate) elapsed: Duration,
}

#[derive(Debug)]
pub(crate) struct DiskMeter<S> {
    roots: Vec<RootCapability>,
    space: S,
}

impl<S: AvailableSpace> DiskMeter<S> {
    pub(crate) fn new(roots: Vec<RootCapability>, space: S) -> Self {
        Self { roots, space }
    }

    pub(crate) fn measure(&self) -> io::Result<MeterReading> {
        let started = Instant::now();
        let mut state = WalkState::default();
        let mut available_by_filesystem = BTreeMap::new();
        for root in &self.roots {
            check_scan_deadline(&|| started.elapsed())?;
            let key = self.space.filesystem_key(root)?;
            check_scan_deadline(&|| started.elapsed())?;
            if let std::collections::btree_map::Entry::Vacant(entry) =
                available_by_filesystem.entry(key)
            {
                let available = self.space.available(root)?;
                check_scan_deadline(&|| started.elapsed())?;
                entry.insert(available);
            }
            measure_owned_tree(root, started, &mut state)?;
        }
        Ok(MeterReading {
            owned_bytes: state.owned_bytes,
            available_by_filesystem,
            conservative_entries: cfg!(windows),
            elapsed: started.elapsed(),
        })
    }
}

pub(crate) trait DiskMeasurement: Send + 'static {
    fn measure(&self) -> io::Result<MeterReading>;
}

impl<S> DiskMeasurement for DiskMeter<S>
where
    S: AvailableSpace + Send + 'static,
{
    fn measure(&self) -> io::Result<MeterReading> {
        Self::measure(self)
    }
}

impl DiskMeasurement for Box<dyn DiskMeasurement> {
    fn measure(&self) -> io::Result<MeterReading> {
        self.as_ref().measure()
    }
}

struct FailedDiskMeasurement {
    kind: io::ErrorKind,
    message: String,
}

impl FailedDiskMeasurement {
    fn new(error: &io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

impl DiskMeasurement for FailedDiskMeasurement {
    fn measure(&self) -> io::Result<MeterReading> {
        Err(io::Error::new(self.kind, self.message.clone()))
    }
}

fn initial_meter_from_capabilities(
    capabilities: impl IntoIterator<Item = io::Result<RootCapability>>,
) -> Box<dyn DiskMeasurement> {
    match capabilities.into_iter().collect::<io::Result<Vec<_>>>() {
        Ok(capabilities) => Box::new(DiskMeter::new(capabilities, SystemAvailableSpace)),
        Err(error) => Box::new(FailedDiskMeasurement::new(&error)),
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DiskMonitorStats {
    pub(crate) peak_owned_bytes: u64,
    pub(crate) minimum_available_bytes: Option<u64>,
    pub(crate) sample_count: u64,
    pub(crate) maximum_measurement: Duration,
    pub(crate) filesystems: BTreeMap<FilesystemKey, FilesystemStats>,
    pub(crate) latest_owned_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FilesystemStats {
    pub(crate) start: u64,
    pub(crate) minimum: u64,
    pub(crate) latest: u64,
}

enum MonitorCommand {
    Sample(tokio::sync::oneshot::Sender<Option<hoimin_core::DiskFailure>>),
    Stop,
}

type MonitorWorker = Box<dyn FnOnce() + Send + 'static>;

pub(crate) struct DiskMonitor {
    commands: mpsc::Sender<MonitorCommand>,
    stop: tokio::sync::watch::Receiver<Option<hoimin_core::DiskFailure>>,
    stats: Arc<Mutex<DiskMonitorStats>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    join_status: Arc<AtomicU8>,
}

const MONITOR_JOIN_PENDING: u8 = 0;
const MONITOR_JOIN_SUCCEEDED: u8 = 1;
const MONITOR_JOIN_FAILED: u8 = 2;

impl DiskMonitor {
    pub(crate) async fn start(
        policy: hoimin_core::DiskPolicy,
        roots: Vec<Arc<super::ManagedRunRoot>>,
    ) -> io::Result<Self> {
        let meter =
            initial_meter_from_capabilities(roots.iter().map(|root| root.disk_capability()));
        Self::start_with_meter(meter, policy, roots).await
    }

    pub(crate) async fn start_with_meter<M>(
        meter: M,
        policy: hoimin_core::DiskPolicy,
        roots: Vec<Arc<super::ManagedRunRoot>>,
    ) -> io::Result<Self>
    where
        M: DiskMeasurement,
    {
        Self::start_with_meter_and_interval(meter, policy, roots, hoimin_core::DISK_SAMPLE_INTERVAL)
            .await
    }

    pub(crate) async fn start_with_meter_and_interval<M>(
        meter: M,
        policy: hoimin_core::DiskPolicy,
        roots: Vec<Arc<super::ManagedRunRoot>>,
        sample_interval: Duration,
    ) -> io::Result<Self>
    where
        M: DiskMeasurement,
    {
        Self::start_with_meter_and_interval_and_spawner(
            meter,
            policy,
            roots,
            sample_interval,
            |worker| {
                std::thread::Builder::new()
                    .name("hoimin-disk-monitor".to_owned())
                    .spawn(worker)
            },
        )
        .await
    }

    async fn start_with_meter_and_interval_and_spawner<M>(
        meter: M,
        policy: hoimin_core::DiskPolicy,
        roots: Vec<Arc<super::ManagedRunRoot>>,
        sample_interval: Duration,
        spawner: impl FnOnce(MonitorWorker) -> io::Result<JoinHandle<()>>,
    ) -> io::Result<Self>
    where
        M: DiskMeasurement,
    {
        let (commands, receiver) = mpsc::channel();
        let (stop_sender, stop) = tokio::sync::watch::channel(None);
        let stats = Arc::new(Mutex::new(DiskMonitorStats::default()));
        let thread_stats = Arc::clone(&stats);
        let join_status = Arc::new(AtomicU8::new(MONITOR_JOIN_PENDING));
        let thread = spawner(Box::new(move || {
            monitor_loop(
                meter,
                policy,
                roots,
                receiver,
                stop_sender,
                thread_stats,
                sample_interval,
            );
        }))?;
        let monitor = Self {
            commands,
            stop,
            stats,
            thread: Mutex::new(Some(thread)),
            join_status,
        };
        let _ = monitor.sample_now().await;
        Ok(monitor)
    }

    pub(crate) fn stop_receiver(
        &self,
    ) -> tokio::sync::watch::Receiver<Option<hoimin_core::DiskFailure>> {
        self.stop.clone()
    }

    pub(crate) async fn sample_now(&self) -> Option<hoimin_core::DiskFailure> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if self.commands.send(MonitorCommand::Sample(sender)).is_err() {
            return Some(measurement_failure("disk monitor thread is unavailable"));
        }
        receiver
            .await
            .unwrap_or_else(|_| Some(measurement_failure("disk monitor sample was abandoned")))
    }

    pub(crate) fn stats(&self) -> DiskMonitorStats {
        self.stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(crate) async fn stop_and_join(&self, budget: Duration) -> bool {
        let _ = self.commands.send(MonitorCommand::Stop);
        let handle = self
            .thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let Some(handle) = handle else {
            return self.join_status.load(Ordering::Acquire) == MONITOR_JOIN_SUCCEEDED;
        };
        let join_status = Arc::clone(&self.join_status);
        let join = tokio::task::spawn_blocking(move || {
            let joined = handle.join().is_ok();
            join_status.store(
                if joined {
                    MONITOR_JOIN_SUCCEEDED
                } else {
                    MONITOR_JOIN_FAILED
                },
                Ordering::Release,
            );
            joined
        });
        matches!(tokio::time::timeout(budget, join).await, Ok(Ok(true)))
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the dedicated monitor thread owns its meter, roots, channels, and stats"
)]
fn monitor_loop<M: DiskMeasurement>(
    meter: M,
    policy: hoimin_core::DiskPolicy,
    roots: Vec<Arc<super::ManagedRunRoot>>,
    commands: mpsc::Receiver<MonitorCommand>,
    stop: tokio::sync::watch::Sender<Option<hoimin_core::DiskFailure>>,
    stats: Arc<Mutex<DiskMonitorStats>>,
    sample_interval: Duration,
) {
    let mut last_refresh = None;
    loop {
        match commands.recv_timeout(sample_interval) {
            Ok(MonitorCommand::Sample(response)) => {
                if response.is_closed() {
                    continue;
                }
                let failure =
                    sample_meter(&meter, policy, &roots, &mut last_refresh, &stop, &stats);
                let _ = response.send(failure);
            }
            Ok(MonitorCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = sample_meter(&meter, policy, &roots, &mut last_refresh, &stop, &stats);
            }
        }
    }
}

fn sample_meter(
    meter: &impl DiskMeasurement,
    policy: hoimin_core::DiskPolicy,
    roots: &[Arc<super::ManagedRunRoot>],
    last_refresh: &mut Option<Instant>,
    stop: &tokio::sync::watch::Sender<Option<hoimin_core::DiskFailure>>,
    stats: &Mutex<DiskMonitorStats>,
) -> Option<hoimin_core::DiskFailure> {
    sample_meter_with_refresh_at(
        meter,
        policy,
        last_refresh,
        stop,
        stats,
        Instant::now(),
        &|| {
            for root in roots {
                root.refresh_heartbeat()
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
            Ok(())
        },
    )
}

fn sample_meter_with_refresh_at(
    meter: &impl DiskMeasurement,
    policy: hoimin_core::DiskPolicy,
    last_refresh: &mut Option<Instant>,
    stop: &tokio::sync::watch::Sender<Option<hoimin_core::DiskFailure>>,
    stats: &Mutex<DiskMonitorStats>,
    now: Instant,
    refresh: &impl Fn() -> io::Result<()>,
) -> Option<hoimin_core::DiskFailure> {
    let refresh_due = last_refresh.is_none_or(|value| {
        now.checked_duration_since(value)
            .is_none_or(|elapsed| elapsed >= Duration::from_secs(60))
    });
    if refresh_due {
        if let Err(error) = refresh() {
            return Some(publish_stop(stop, measurement_failure(error.to_string())));
        }
        *last_refresh = Some(now);
    }
    let reading = match meter.measure() {
        Ok(reading) => reading,
        Err(error) => return Some(publish_stop(stop, measurement_failure(error.to_string()))),
    };
    let Some(available_bytes) = reading.available_by_filesystem.values().copied().min() else {
        return Some(publish_stop(
            stop,
            measurement_failure("disk measurement returned no filesystem capacity"),
        ));
    };
    {
        let mut snapshot = stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        snapshot.sample_count = snapshot.sample_count.saturating_add(1);
        snapshot.peak_owned_bytes = snapshot.peak_owned_bytes.max(reading.owned_bytes);
        snapshot.minimum_available_bytes = Some(
            snapshot
                .minimum_available_bytes
                .map_or(available_bytes, |prior| prior.min(available_bytes)),
        );
        snapshot.maximum_measurement = snapshot.maximum_measurement.max(reading.elapsed);
        snapshot.latest_owned_bytes = Some(reading.owned_bytes);
        for (key, available) in &reading.available_by_filesystem {
            snapshot
                .filesystems
                .entry(*key)
                .and_modify(|filesystem| {
                    filesystem.minimum = filesystem.minimum.min(*available);
                    filesystem.latest = *available;
                })
                .or_insert(FilesystemStats {
                    start: *available,
                    minimum: *available,
                    latest: *available,
                });
        }
    }
    match policy.evaluate(hoimin_core::DiskObservation {
        owned_bytes: reading.owned_bytes,
        available_bytes,
        measured_in: reading.elapsed,
    }) {
        hoimin_core::DiskDecision::Continue => stop.borrow().clone(),
        hoimin_core::DiskDecision::Stop(failure) => Some(publish_stop(stop, failure)),
    }
}

pub(crate) fn measure_managed_roots(
    roots: &[Arc<super::ManagedRunRoot>],
) -> io::Result<MeterReading> {
    let capabilities = roots
        .iter()
        .map(|root| root.disk_capability())
        .collect::<io::Result<Vec<_>>>()?;
    DiskMeter::new(capabilities, SystemAvailableSpace).measure()
}

pub(crate) fn available_for_managed_roots(
    roots: &[Arc<super::ManagedRunRoot>],
) -> io::Result<BTreeMap<FilesystemKey, u64>> {
    let space = SystemAvailableSpace;
    let mut available = BTreeMap::new();
    for root in roots {
        let capability = root.disk_capability()?;
        let key = space.filesystem_key(&capability)?;
        if let std::collections::btree_map::Entry::Vacant(entry) = available.entry(key) {
            entry.insert(space.available(&capability)?);
        }
    }
    Ok(available)
}

fn measurement_failure(message: impl Into<String>) -> hoimin_core::DiskFailure {
    hoimin_core::DiskFailure {
        code: hoimin_core::DISK_MEASUREMENT_FAILED.to_owned(),
        reason: hoimin_core::DiskStopReason::MeasurementFailed,
        observation: None,
        message: Some(message.into()),
        secondary: Vec::new(),
    }
}

fn publish_stop(
    stop: &tokio::sync::watch::Sender<Option<hoimin_core::DiskFailure>>,
    mut failure: hoimin_core::DiskFailure,
) -> hoimin_core::DiskFailure {
    let Some(mut primary) = stop.borrow().clone() else {
        stop.send_replace(Some(failure.clone()));
        return failure;
    };
    let mut secondary = Vec::with_capacity(1 + failure.secondary.len());
    if let Some(observation) = failure.observation {
        secondary.push(hoimin_core::DiskSecondary::Observation {
            reason: failure.reason,
            value: observation,
        });
    } else if let Some(message) = failure.message.take() {
        secondary.push(hoimin_core::DiskSecondary::Error {
            code: failure.code.clone(),
            message,
        });
    }
    secondary.append(&mut failure.secondary);
    for value in secondary {
        if stop_matches_secondary(&primary, &value) || primary.secondary.contains(&value) {
            continue;
        }
        primary.secondary.push(value);
    }
    stop.send_replace(Some(primary.clone()));
    primary
}

fn stop_matches_secondary(
    stop: &hoimin_core::DiskFailure,
    secondary: &hoimin_core::DiskSecondary,
) -> bool {
    match secondary {
        hoimin_core::DiskSecondary::Observation { reason, value } => {
            stop.reason == *reason && stop.observation.as_ref() == Some(value)
        }
        hoimin_core::DiskSecondary::Error { code, message } => {
            stop.code == *code && stop.message.as_ref() == Some(message)
        }
    }
}

#[derive(Default)]
struct WalkState {
    owned_bytes: u64,
    entries: usize,
    #[cfg(unix)]
    identities: BTreeSet<(u64, u64)>,
}

#[cfg(unix)]
struct WalkFrame {
    entries: rustix::fs::Dir,
    depth: usize,
    display_path: Utf8PathBuf,
}

fn measure_owned_tree(
    root: &RootCapability,
    started: Instant,
    state: &mut WalkState,
) -> io::Result<()> {
    measure_owned_tree_with_elapsed(root, state, &|| started.elapsed())
}

fn measure_owned_tree_with_elapsed(
    root: &RootCapability,
    state: &mut WalkState,
    elapsed: &impl Fn() -> Duration,
) -> io::Result<()> {
    measure_owned_tree_with_hooks(root, state, elapsed, &|_| {}, &|_| {})
}

fn check_scan_deadline(elapsed: &impl Fn() -> Duration) -> io::Result<()> {
    if elapsed() >= MAX_SCAN_DURATION {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "owned workspace scan exceeded five seconds",
        ))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
#[allow(
    clippy::too_many_lines,
    reason = "the walker keeps every filesystem operation and its deadline check adjacent"
)]
fn measure_owned_tree_with_hooks(
    root: &RootCapability,
    state: &mut WalkState,
    elapsed: &impl Fn() -> Duration,
    before_directory_open: &impl Fn(&Utf8Path),
    observe_open_directories: &impl Fn(usize),
) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    use rustix::fs::{AtFlags, FileType};

    check_scan_deadline(elapsed)?;
    let root_dir = open_meter_root_directory(root)?;
    check_scan_deadline(elapsed)?;
    let root_metadata =
        rustix::fs::fstat(root_dir.fd().map_err(io::Error::from)?).map_err(io::Error::from)?;
    check_scan_deadline(elapsed)?;
    let root_device = root_metadata.st_dev;
    #[cfg(target_os = "macos")]
    let root_mount = {
        let identity = macos_mount_identity(&root_dir.fd().map_err(io::Error::from)?)?;
        check_scan_deadline(elapsed)?;
        identity
    };
    let mut stack = vec![WalkFrame {
        entries: root_dir,
        depth: 0,
        display_path: root.display_path.clone(),
    }];
    observe_open_directories(stack.len());
    while let Some(frame) = stack.last_mut() {
        check_scan_deadline(elapsed)?;
        let next_entry = frame.entries.next();
        check_scan_deadline(elapsed)?;
        let Some(entry) = next_entry else {
            stack.pop();
            continue;
        };
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if io::Error::from(error).kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io::Error::from(error)),
        };
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        record_entry(state)?;
        let name_os = std::ffi::OsStr::from_bytes(name.to_bytes());
        let display = frame.display_path.join(
            name_os
                .to_str()
                .ok_or_else(|| io::Error::other("non-UTF-8 workspace entry"))?,
        );
        let directory = frame.entries.fd().map_err(io::Error::from)?;
        let metadata_result = rustix::fs::statat(directory, name, AtFlags::SYMLINK_NOFOLLOW);
        check_scan_deadline(elapsed)?;
        let metadata = match metadata_result {
            Ok(metadata) => metadata,
            Err(error) if io::Error::from(error).kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io::Error::from(error)),
        };
        let file_type = FileType::from_raw_mode(metadata.st_mode);
        if file_type == FileType::Symlink {
            continue;
        }
        if file_type == FileType::Directory {
            if metadata.st_dev != root_device {
                return Err(io::Error::other(format!(
                    "owned workspace scan refuses to cross a filesystem boundary: {display}"
                )));
            }
            let child_depth = frame.depth + 1;
            if child_depth > MAX_TREE_DEPTH {
                return Err(io::Error::other(format!(
                    "owned workspace depth exceeds {MAX_TREE_DEPTH}: {display}"
                )));
            }
            before_directory_open(&display);
            check_scan_deadline(elapsed)?;
            let child_result = open_meter_directory(&directory, name);
            check_scan_deadline(elapsed)?;
            let child = match child_result {
                Ok(child) => child,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            #[cfg(target_os = "macos")]
            {
                let child_mount = macos_mount_identity(&child)?;
                check_scan_deadline(elapsed)?;
                if child_mount != root_mount {
                    return Err(io::Error::other(format!(
                        "owned workspace scan refuses to cross a mount boundary: {display}"
                    )));
                }
            }
            let opened_metadata = rustix::fs::fstat(&child).map_err(io::Error::from)?;
            check_scan_deadline(elapsed)?;
            if unix_stat_identity(&metadata) != unix_stat_identity(&opened_metadata) {
                return Err(io::Error::other(format!(
                    "owned workspace directory identity changed while opening: {display}"
                )));
            }
            let entries = rustix::fs::Dir::new(child).map_err(io::Error::from)?;
            check_scan_deadline(elapsed)?;
            stack.push(WalkFrame {
                entries,
                depth: child_depth,
                display_path: display,
            });
            observe_open_directories(stack.len());
            continue;
        }
        if file_type == FileType::RegularFile && should_count_unix_stat(&metadata, state) {
            let length = u64::try_from(metadata.st_size)
                .map_err(|_| io::Error::other("negative workspace file length"))?;
            state.owned_bytes = state
                .owned_bytes
                .checked_add(length)
                .ok_or_else(|| io::Error::other("owned workspace byte count overflow"))?;
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn open_meter_root_directory(root: &RootCapability) -> io::Result<rustix::fs::Dir> {
    // cap-std uses O_PATH for directory capabilities, while enumeration needs a readable fd.
    rustix::fs::Dir::new(open_meter_directory(&root.dir, c".")?).map_err(io::Error::from)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn open_meter_root_directory(root: &RootCapability) -> io::Result<rustix::fs::Dir> {
    rustix::fs::Dir::read_from(&root.dir).map_err(io::Error::from)
}

#[cfg(target_os = "macos")]
type MacosMountIdentity = [u8; std::mem::size_of::<libc::fsid_t>()];

#[cfg(target_os = "macos")]
fn macos_mount_identity(fd: &impl std::os::fd::AsFd) -> io::Result<MacosMountIdentity> {
    use std::os::fd::AsRawFd;

    // SAFETY: result points to writable storage and fd remains borrowed for the call.
    let result = unsafe {
        let mut result: libc::statfs = std::mem::zeroed();
        if libc::fstatfs(fd.as_fd().as_raw_fd(), &raw mut result) != 0 {
            return Err(io::Error::last_os_error());
        }
        result
    };
    let mut identity = [0_u8; std::mem::size_of::<libc::fsid_t>()];
    // SAFETY: identity has exactly the byte size of f_fsid and both ranges are valid/nonoverlap.
    unsafe {
        std::ptr::copy_nonoverlapping(
            (&raw const result.f_fsid).cast::<u8>(),
            identity.as_mut_ptr(),
            identity.len(),
        );
    }
    Ok(identity)
}

#[cfg(target_os = "linux")]
fn open_meter_directory(
    directory: &impl std::os::fd::AsFd,
    name: &std::ffi::CStr,
) -> io::Result<std::os::fd::OwnedFd> {
    use rustix::fs::{Mode, OFlags, ResolveFlags};

    rustix::fs::openat2(
        directory,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV,
    )
    .map_err(io::Error::from)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn open_meter_directory(
    directory: &impl std::os::fd::AsFd,
    name: &std::ffi::CStr,
) -> io::Result<std::os::fd::OwnedFd> {
    use rustix::fs::{Mode, OFlags};

    rustix::fs::openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)
}

#[cfg(windows)]
struct WindowsWalkFrame {
    entries: super::root::windows::DirectoryEntries,
    depth: usize,
    display_path: Utf8PathBuf,
}

#[cfg(windows)]
#[allow(
    clippy::too_many_lines,
    reason = "the walker keeps every filesystem operation and its deadline check adjacent"
)]
fn measure_owned_tree_with_hooks(
    root: &RootCapability,
    state: &mut WalkState,
    elapsed: &impl Fn() -> Duration,
    before_directory_open: &impl Fn(&Utf8Path),
    observe_open_directories: &impl Fn(usize),
) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };

    check_scan_deadline(elapsed)?;
    let root_dir = root.dir.try_clone()?.into_std_file();
    check_scan_deadline(elapsed)?;
    let root_entries = super::root::windows::DirectoryEntries::open(root_dir);
    check_scan_deadline(elapsed)?;
    let root_frame = WindowsWalkFrame {
        entries: root_entries,
        depth: 0,
        display_path: root.display_path.clone(),
    };
    let mut stack = vec![root_frame];
    observe_open_directories(stack.len());
    while let Some(frame) = stack.last_mut() {
        check_scan_deadline(elapsed)?;
        let next_entry = frame.entries.next_entry();
        check_scan_deadline(elapsed)?;
        let entry = match next_entry {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                stack.pop();
                continue;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if entry.name() == "." || entry.name() == ".." {
            continue;
        }
        record_entry(state)?;
        let attributes = entry.file_attributes();
        let entry_file_id = entry.file_id();
        let name = entry.into_name();
        let display = frame.display_path.join(
            name.to_str()
                .ok_or_else(|| io::Error::other("non-UTF-8 workspace entry"))?,
        );
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            continue;
        }
        let child_depth = frame.depth + 1;
        if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 && child_depth > MAX_TREE_DEPTH {
            return Err(io::Error::other(format!(
                "owned workspace depth exceeds {MAX_TREE_DEPTH}: {display}"
            )));
        }
        // At the deepest accepted frame, acquire only a non-directory handle. This pins the
        // enumerated file without increasing the directory-handle peak, and the file-id check
        // makes a same-name replacement fail closed.
        if frame.depth == MAX_TREE_DEPTH {
            let inspected_result =
                super::root::windows::open_regular_file_shared(frame.entries.directory(), &name);
            check_scan_deadline(elapsed)?;
            let inspected = match inspected_result {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let identity = super::root::windows::file_identity_io(&inspected)?;
            check_scan_deadline(elapsed)?;
            if identity.1 != entry_file_id {
                return Err(io::Error::other(format!(
                    "owned workspace entry identity changed while opening: {display}"
                )));
            }
            let length = inspected.metadata()?.len();
            check_scan_deadline(elapsed)?;
            state.owned_bytes = state
                .owned_bytes
                .checked_add(length)
                .ok_or_else(|| io::Error::other("owned workspace byte count overflow"))?;
            continue;
        }
        let inspected_result =
            super::root::windows::open_entry_shared(frame.entries.directory(), &name);
        check_scan_deadline(elapsed)?;
        let inspected = match inspected_result {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let metadata = inspected.metadata()?;
        check_scan_deadline(elapsed)?;
        let inspected_identity = super::root::windows::file_identity_io(&inspected)?;
        check_scan_deadline(elapsed)?;
        if inspected_identity.1 != entry_file_id {
            return Err(io::Error::other(format!(
                "owned workspace entry identity changed while opening: {display}"
            )));
        }
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            continue;
        }
        if metadata.is_dir() {
            before_directory_open(&display);
            check_scan_deadline(elapsed)?;
            observe_open_directories(child_depth + 1);
            let expected_identity = super::root::windows::file_identity_io(&inspected)?;
            check_scan_deadline(elapsed)?;
            // Keep the depth bound equal to the handle bound: release the inspection handle
            // before acquiring the one child handle that the next frame will own. The identity
            // retained above makes any replacement during this gap fail closed below.
            drop(inspected);
            let child_result =
                super::root::windows::open_directory_shared(frame.entries.directory(), &name);
            check_scan_deadline(elapsed)?;
            let child = match child_result {
                Ok(child) => child,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let child_identity = super::root::windows::file_identity_io(&child)?;
            check_scan_deadline(elapsed)?;
            if child_identity != expected_identity {
                return Err(io::Error::other(format!(
                    "owned workspace directory identity changed while opening: {display}"
                )));
            }
            let entries = super::root::windows::DirectoryEntries::open(child);
            check_scan_deadline(elapsed)?;
            let child_frame = WindowsWalkFrame {
                entries,
                depth: child_depth,
                display_path: display,
            };
            stack.push(child_frame);
            observe_open_directories(stack.len());
        } else if metadata.is_file() {
            state.owned_bytes = state
                .owned_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| io::Error::other("owned workspace byte count overflow"))?;
        }
    }
    Ok(())
}

#[cfg(all(test, windows))]
fn open_windows_meter_root(
    path: &Utf8Path,
    expected_identity: Option<(u64, u64)>,
) -> io::Result<cap_std::fs::Dir> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::FromRawHandle;

    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, SYNCHRONIZE,
    };

    let wide = path
        .as_std_path()
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: wide is NUL-terminated and no optional security/template pointers are used.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW returned one newly owned handle.
    let file = unsafe { std::fs::File::from_raw_handle(handle.cast()) };
    let metadata = file.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("meter root is not a real directory"));
    }
    let reopened_identity = super::root::windows::file_identity_io(&file)?;
    let dir = cap_std::fs::Dir::from_std_file(file);
    if expected_identity.is_some_and(|expected| reopened_identity != expected) {
        return Err(io::Error::other(
            "meter root identity changed while reopening",
        ));
    }
    Ok(dir)
}

fn record_entry(state: &mut WalkState) -> io::Result<()> {
    state.entries = state
        .entries
        .checked_add(1)
        .ok_or_else(|| io::Error::other("owned workspace entry count overflow"))?;
    if state.entries > MAX_TREE_ENTRIES {
        return Err(io::Error::other(
            "owned workspace entry count exceeds 250000",
        ));
    }
    Ok(())
}

#[cfg(unix)]
#[allow(
    clippy::cast_sign_loss,
    clippy::unnecessary_cast,
    reason = "libc stat identity fields vary between Unix ABIs"
)]
fn unix_stat_identity(metadata: &rustix::fs::Stat) -> (u64, u64) {
    (metadata.st_dev as u64, metadata.st_ino as u64)
}

#[cfg(unix)]
fn should_count_unix_stat(metadata: &rustix::fs::Stat, state: &mut WalkState) -> bool {
    state.identities.insert(unix_stat_identity(metadata))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque};

    use camino::Utf8Path;

    use super::measure_owned_tree_with_hooks;
    use super::{
        AvailableSpace, DiskMeasurement, DiskMeter, DiskMonitor, FilesystemKey, MeterReading,
        RootCapability, WalkState, initial_meter_from_capabilities,
        measure_owned_tree_with_elapsed, record_entry, sample_meter_with_refresh_at,
    };

    #[derive(Debug)]
    struct FixedSpace(u64);

    impl AvailableSpace for FixedSpace {
        fn available(&self, _root: &RootCapability) -> std::io::Result<u64> {
            Ok(self.0)
        }

        fn filesystem_key(&self, _root: &RootCapability) -> std::io::Result<FilesystemKey> {
            Ok(FilesystemKey(7))
        }
    }

    fn capability(path: &Utf8Path) -> RootCapability {
        RootCapability::open(path).unwrap()
    }

    struct ScriptedMeter {
        readings: std::sync::Mutex<VecDeque<std::io::Result<MeterReading>>>,
    }

    impl DiskMeasurement for ScriptedMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            self.readings
                .lock()
                .expect("scripted meter lock")
                .pop_front()
                .expect("scripted reading")
        }
    }

    struct SpacedMeter {
        active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        maximum_active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        starts: std::sync::mpsc::Sender<std::time::Instant>,
        scan_duration: std::time::Duration,
    }

    struct BlockingAfterInitialMeter {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        entered: std::sync::mpsc::Sender<()>,
        release: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }

    struct PanickingAfterInitialMeter {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl DiskMeasurement for PanickingAfterInitialMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            assert_eq!(call, 0, "injected monitor panic after initial sample");
            Ok(MeterReading {
                owned_bytes: 1,
                available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
                conservative_entries: false,
                elapsed: std::time::Duration::from_millis(1),
            })
        }
    }

    impl DiskMeasurement for BlockingAfterInitialMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            if call != 0 {
                let _ = self.entered.send(());
                let (released, changed) = &*self.release;
                let mut released = released.lock().expect("release lock");
                while !*released {
                    released = changed.wait(released).expect("release wait");
                }
            }
            Ok(MeterReading {
                owned_bytes: 1,
                available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
                conservative_entries: false,
                elapsed: std::time::Duration::from_millis(1),
            })
        }
    }

    #[tokio::test]
    async fn monitor_thread_spawn_failure_is_returned() {
        let result = DiskMonitor::start_with_meter_and_interval_and_spawner(
            ScriptedMeter {
                readings: std::sync::Mutex::new(VecDeque::from([Ok(MeterReading {
                    owned_bytes: 1,
                    available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
                    conservative_entries: false,
                    elapsed: std::time::Duration::from_millis(1),
                })])),
            },
            hoimin_core::DiskPolicy {
                max_owned_bytes: std::num::NonZeroU64::new(u64::MAX).unwrap(),
                min_free_bytes: std::num::NonZeroU64::new(1).unwrap(),
            },
            Vec::new(),
            std::time::Duration::from_secs(60),
            |worker| {
                let _worker = worker;
                Err(std::io::Error::other("injected monitor spawn failure"))
            },
        )
        .await;

        let Err(error) = result else {
            panic!("monitor spawn unexpectedly succeeded")
        };
        assert_eq!(error.to_string(), "injected monitor spawn failure");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn timed_out_monitor_join_stays_unjoined_until_the_scan_exits() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let release =
            std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let monitor = std::sync::Arc::new(
            DiskMonitor::start_with_meter_and_interval(
                BlockingAfterInitialMeter {
                    calls,
                    entered: entered_tx,
                    release: std::sync::Arc::clone(&release),
                },
                hoimin_core::DiskPolicy {
                    max_owned_bytes: std::num::NonZeroU64::new(u64::MAX).unwrap(),
                    min_free_bytes: std::num::NonZeroU64::new(1).unwrap(),
                },
                Vec::new(),
                std::time::Duration::from_secs(60),
            )
            .await
            .expect("monitor thread starts"),
        );
        let sampler = {
            let monitor = std::sync::Arc::clone(&monitor);
            tokio::spawn(async move { monitor.sample_now().await })
        };
        tokio::task::spawn_blocking(move || {
            entered_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("blocked scan entered");
        })
        .await
        .unwrap();

        assert!(
            !monitor
                .stop_and_join(std::time::Duration::from_millis(10))
                .await
        );
        let second_join = monitor
            .stop_and_join(std::time::Duration::from_millis(10))
            .await;

        let (released, changed) = &*release;
        *released.lock().expect("release lock") = true;
        changed.notify_all();
        sampler.await.unwrap();
        assert!(
            !second_join,
            "a missing JoinHandle must not masquerade as a completed monitor"
        );
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !monitor
                .stop_and_join(std::time::Duration::from_millis(10))
                .await
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("monitor eventually joined");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panicked_monitor_never_reports_join_success() {
        let monitor = DiskMonitor::start_with_meter_and_interval(
            PanickingAfterInitialMeter {
                calls: std::sync::atomic::AtomicUsize::new(0),
            },
            hoimin_core::DiskPolicy {
                max_owned_bytes: std::num::NonZeroU64::new(u64::MAX).unwrap(),
                min_free_bytes: std::num::NonZeroU64::new(1).unwrap(),
            },
            Vec::new(),
            std::time::Duration::from_secs(60),
        )
        .await
        .expect("monitor thread starts");

        assert!(monitor.sample_now().await.is_some());
        assert!(
            !monitor
                .stop_and_join(std::time::Duration::from_secs(1))
                .await
        );
        assert!(
            !monitor
                .stop_and_join(std::time::Duration::from_millis(10))
                .await,
            "a panicked monitor must never be reported as cleanly joined"
        );
    }

    impl DiskMeasurement for SpacedMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            let active = self
                .active
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                + 1;
            self.maximum_active
                .fetch_max(active, std::sync::atomic::Ordering::AcqRel);
            self.starts
                .send(std::time::Instant::now())
                .expect("scan start receiver");
            std::thread::sleep(self.scan_duration);
            self.active
                .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            Ok(MeterReading {
                owned_bytes: 1,
                available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
                conservative_entries: false,
                elapsed: self.scan_duration,
            })
        }
    }

    #[tokio::test]
    async fn monitor_runs_one_scan_at_a_time_and_waits_after_each_scan() {
        let active = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let maximum_active = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (starts_tx, starts_rx) = std::sync::mpsc::channel();
        let scan_duration = std::time::Duration::from_millis(20);
        let interval = std::time::Duration::from_millis(20);
        let monitor = DiskMonitor::start_with_meter_and_interval(
            SpacedMeter {
                active: std::sync::Arc::clone(&active),
                maximum_active: std::sync::Arc::clone(&maximum_active),
                starts: starts_tx,
                scan_duration,
            },
            hoimin_core::DiskPolicy {
                max_owned_bytes: std::num::NonZeroU64::new(u64::MAX).unwrap(),
                min_free_bytes: std::num::NonZeroU64::new(1).unwrap(),
            },
            Vec::new(),
            interval,
        )
        .await
        .expect("monitor thread starts");

        let first = starts_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("initial scan starts");
        let second = starts_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("periodic scan starts");
        assert!(
            monitor
                .stop_and_join(std::time::Duration::from_secs(1))
                .await
        );

        assert_eq!(maximum_active.load(std::sync::atomic::Ordering::Acquire), 1);
        assert!(
            second.duration_since(first) >= scan_duration.saturating_add(interval),
            "periodic scan started too early: first={first:?}, second={second:?}"
        );
    }

    #[tokio::test]
    async fn monitor_samples_at_start_and_keeps_the_first_stop_reason() {
        let reading = |owned_bytes, available_bytes| MeterReading {
            owned_bytes,
            available_by_filesystem: BTreeMap::from([(FilesystemKey(7), available_bytes)]),
            conservative_entries: false,
            elapsed: std::time::Duration::from_millis(3),
        };
        let meter = ScriptedMeter {
            readings: std::sync::Mutex::new(VecDeque::from([
                Ok(reading(100, 1_000)),
                Ok(reading(200, 400)),
                Ok(reading(900, 800)),
                Err(std::io::Error::other("statvfs failed")),
                Err(std::io::Error::other("statvfs failed differently")),
                Err(std::io::Error::other("statvfs failed differently")),
            ])),
        };
        let policy = hoimin_core::DiskPolicy {
            max_owned_bytes: std::num::NonZeroU64::new(800).unwrap(),
            min_free_bytes: std::num::NonZeroU64::new(500).unwrap(),
        };

        let monitor = DiskMonitor::start_with_meter(meter, policy, Vec::new())
            .await
            .expect("monitor thread starts");
        assert!(monitor.stop_receiver().borrow().is_none());
        let first = monitor.sample_now().await.expect("reserve stop");
        assert_eq!(
            first.reason,
            hoimin_core::DiskStopReason::FilesystemReserveReached
        );
        let second = monitor.sample_now().await.expect("sticky stop");
        assert_eq!(second.reason, first.reason);
        assert_eq!(
            second
                .secondary
                .iter()
                .map(hoimin_core::DiskSecondary::code)
                .collect::<Vec<_>>(),
            vec![hoimin_core::WORKSPACE_SIZE_EXCEEDED]
        );
        let third = monitor.sample_now().await.expect("sticky measurement stop");
        assert_eq!(third.reason, first.reason);
        assert_eq!(
            third
                .secondary
                .iter()
                .map(hoimin_core::DiskSecondary::code)
                .collect::<Vec<_>>(),
            vec![
                hoimin_core::WORKSPACE_SIZE_EXCEEDED,
                hoimin_core::DISK_MEASUREMENT_FAILED,
            ]
        );
        let fourth = monitor
            .sample_now()
            .await
            .expect("distinct sticky measurement stop");
        assert_eq!(fourth.reason, first.reason);
        assert_eq!(fourth.secondary.len(), 3);
        assert!(fourth.secondary.iter().any(|secondary| {
            matches!(
                secondary,
                hoimin_core::DiskSecondary::Error { code, message }
                    if code == hoimin_core::DISK_MEASUREMENT_FAILED
                        && message == "statvfs failed differently"
            )
        }));
        let fifth = monitor
            .sample_now()
            .await
            .expect("exact duplicate measurement stop");
        assert_eq!(fifth.secondary.len(), 3);
        let stats = monitor.stats();
        assert_eq!(stats.sample_count, 3);
        assert_eq!(stats.latest_owned_bytes, Some(900));
        assert_eq!(
            stats.filesystems.get(&FilesystemKey(7)),
            Some(&super::FilesystemStats {
                start: 1_000,
                minimum: 400,
                latest: 800,
            })
        );
        assert!(
            monitor
                .stop_and_join(std::time::Duration::from_secs(1))
                .await
        );
    }

    #[tokio::test]
    async fn initial_measurement_failure_is_published_as_the_typed_monitor_stop() {
        let monitor = DiskMonitor::start_with_meter(
            ScriptedMeter {
                readings: std::sync::Mutex::new(VecDeque::from([Err(std::io::Error::other(
                    "initial statvfs failed",
                ))])),
            },
            hoimin_core::DiskPolicy {
                max_owned_bytes: std::num::NonZeroU64::new(u64::MAX).unwrap(),
                min_free_bytes: std::num::NonZeroU64::new(1).unwrap(),
            },
            Vec::new(),
        )
        .await
        .expect("monitor thread starts");

        let failure = monitor
            .stop_receiver()
            .borrow()
            .clone()
            .expect("initial measurement failure");
        assert_eq!(failure.code, hoimin_core::DISK_MEASUREMENT_FAILED);
        assert_eq!(
            failure.reason,
            hoimin_core::DiskStopReason::MeasurementFailed
        );
        assert_eq!(failure.message.as_deref(), Some("initial statvfs failed"));
        assert_eq!(monitor.stats().sample_count, 0);
        assert!(
            monitor
                .stop_and_join(std::time::Duration::from_secs(1))
                .await
        );
    }

    #[test]
    fn initial_capability_failure_becomes_a_failed_measurement() {
        let meter = initial_meter_from_capabilities([Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "capability denied",
        ))]);

        let error = meter.measure().unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "capability denied");
    }

    #[tokio::test]
    async fn initial_empty_filesystem_map_is_published_as_a_typed_failure() {
        let monitor = DiskMonitor::start_with_meter(
            ScriptedMeter {
                readings: std::sync::Mutex::new(VecDeque::from([Ok(MeterReading {
                    owned_bytes: 1,
                    available_by_filesystem: BTreeMap::new(),
                    conservative_entries: false,
                    elapsed: std::time::Duration::from_millis(1),
                })])),
            },
            hoimin_core::DiskPolicy {
                max_owned_bytes: std::num::NonZeroU64::new(u64::MAX).unwrap(),
                min_free_bytes: std::num::NonZeroU64::new(1).unwrap(),
            },
            Vec::new(),
        )
        .await
        .expect("monitor thread starts");

        let failure = monitor
            .stop_receiver()
            .borrow()
            .clone()
            .expect("empty-filesystem failure");
        assert_eq!(failure.code, hoimin_core::DISK_MEASUREMENT_FAILED);
        assert_eq!(
            failure.message.as_deref(),
            Some("disk measurement returned no filesystem capacity")
        );
        assert_eq!(monitor.stats().sample_count, 0);
        assert!(
            monitor
                .stop_and_join(std::time::Duration::from_secs(1))
                .await
        );
    }

    #[test]
    fn heartbeat_refresh_uses_the_exact_sixty_second_boundary_and_fails_closed() {
        let base = std::time::Instant::now();
        let mut last_refresh = Some(base);
        let meter = ScriptedMeter {
            readings: std::sync::Mutex::new(VecDeque::from([Ok(MeterReading {
                owned_bytes: 1,
                available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
                conservative_entries: false,
                elapsed: std::time::Duration::from_millis(1),
            })])),
        };
        let policy = hoimin_core::DiskPolicy {
            max_owned_bytes: std::num::NonZeroU64::new(u64::MAX).unwrap(),
            min_free_bytes: std::num::NonZeroU64::new(1).unwrap(),
        };
        let (stop, _) = tokio::sync::watch::channel(None);
        let stats = std::sync::Mutex::new(super::DiskMonitorStats::default());
        let refreshes = std::cell::Cell::new(0_u32);

        assert!(
            sample_meter_with_refresh_at(
                &meter,
                policy,
                &mut last_refresh,
                &stop,
                &stats,
                base + std::time::Duration::from_millis(59_999),
                &|| {
                    refreshes.set(refreshes.get() + 1);
                    Ok(())
                },
            )
            .is_none()
        );
        assert_eq!(refreshes.get(), 0);

        let failure = sample_meter_with_refresh_at(
            &meter,
            policy,
            &mut last_refresh,
            &stop,
            &stats,
            base + std::time::Duration::from_secs(60),
            &|| {
                refreshes.set(refreshes.get() + 1);
                Err(std::io::Error::other("heartbeat fsync failed"))
            },
        )
        .expect("heartbeat failure must stop dispatch");
        assert_eq!(refreshes.get(), 1);
        assert_eq!(
            failure.reason,
            hoimin_core::DiskStopReason::MeasurementFailed
        );
        assert_eq!(last_refresh, Some(base));
    }

    #[test]
    fn injected_deadline_and_entry_cap_fail_without_large_fixtures() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = capability(temp);
        let mut state = WalkState::default();

        let deadline = measure_owned_tree_with_elapsed(&root, &mut state, &|| {
            std::time::Duration::from_secs(5)
        })
        .unwrap_err();
        assert_eq!(deadline.kind(), std::io::ErrorKind::TimedOut);

        state.entries = super::MAX_TREE_ENTRIES;
        assert!(record_entry(&mut state).is_err());
        assert_eq!(state.entries, super::MAX_TREE_ENTRIES + 1);
    }

    #[cfg(unix)]
    #[test]
    fn scan_does_not_open_child_after_prior_operation_crosses_deadline() {
        use std::cell::Cell;

        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir(temp.join("child")).unwrap();
        let root = capability(temp);
        let deadline_crossed = Cell::new(false);
        let child_opened = Cell::new(false);
        let mut state = WalkState::default();

        let error = measure_owned_tree_with_hooks(
            &root,
            &mut state,
            &|| {
                if deadline_crossed.get() {
                    super::MAX_SCAN_DURATION
                } else {
                    std::time::Duration::ZERO
                }
            },
            &|_| deadline_crossed.set(true),
            &|open| {
                if open > 1 {
                    child_opened.set(true);
                }
            },
        )
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(!child_opened.get());
    }

    #[cfg(unix)]
    #[test]
    fn directory_replaced_by_a_symlink_before_open_is_never_followed() {
        use std::cell::Cell;
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let outside = Utf8Path::from_path(outside.path()).unwrap();
        std::fs::create_dir(temp.join("victim")).unwrap();
        std::fs::write(outside.join("must-not-count"), vec![0_u8; 4096]).unwrap();
        let root = capability(temp);
        let swapped = Cell::new(false);
        let mut state = WalkState::default();

        let result = measure_owned_tree_with_hooks(
            &root,
            &mut state,
            &|| std::time::Duration::ZERO,
            &|display| {
                if display == temp.join("victim") && !swapped.replace(true) {
                    std::fs::rename(temp.join("victim"), temp.join("parked")).unwrap();
                    symlink(outside, temp.join("victim")).unwrap();
                }
            },
            &|_| {},
        );

        assert!(result.is_err());
        assert!(swapped.get());
        assert_eq!(state.owned_bytes, 0);
        assert!(outside.join("must-not-count").exists());
    }

    #[test]
    fn exact_depth_bound_uses_at_most_one_hundred_twenty_nine_directory_handles() {
        use std::cell::Cell;

        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = temp.join("root");
        std::fs::create_dir(&root).unwrap();
        let mut cursor = root.clone();
        for _ in 0..super::MAX_TREE_DEPTH {
            cursor.push("d");
            std::fs::create_dir(&cursor).unwrap();
        }
        let root = capability(&root);
        let mut state = WalkState::default();
        let peak = Cell::new(0_usize);

        measure_owned_tree_with_hooks(
            &root,
            &mut state,
            &|| std::time::Duration::ZERO,
            &|_| {},
            &|open| peak.set(peak.get().max(open)),
        )
        .unwrap();

        assert_eq!(peak.get(), super::MAX_TREE_DEPTH + 1);
        assert_eq!(peak.get(), 129);
    }

    #[cfg(unix)]
    fn open_file_descriptor_count() -> usize {
        #[cfg(target_os = "linux")]
        let directory = "/proc/self/fd";
        #[cfg(not(target_os = "linux"))]
        let directory = "/dev/fd";
        std::fs::read_dir(directory).unwrap().count()
    }

    #[cfg(unix)]
    #[test]
    fn directory_handle_bound_child() {
        use std::cell::Cell;

        if std::env::var_os("HOIMIN_DISK_HANDLE_BOUND_CHILD").is_none() {
            return;
        }

        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = temp.join("root");
        std::fs::create_dir(&root).unwrap();
        let mut cursor = root.clone();
        for _ in 0..super::MAX_TREE_DEPTH {
            cursor.push("d");
            std::fs::create_dir(&cursor).unwrap();
        }
        let root = capability(&root);
        let baseline = open_file_descriptor_count();
        let peak = Cell::new(baseline);
        let mut state = WalkState::default();

        measure_owned_tree_with_hooks(
            &root,
            &mut state,
            &|| std::time::Duration::ZERO,
            &|_| {},
            &|_| peak.set(peak.get().max(open_file_descriptor_count())),
        )
        .unwrap();

        assert_eq!(peak.get().checked_sub(baseline), Some(129));
    }

    #[cfg(unix)]
    #[test]
    fn fresh_process_observes_the_real_directory_descriptor_bound() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("workspace::disk::tests::directory_handle_bound_child")
            .arg("--nocapture")
            .env("HOIMIN_DISK_HANDLE_BOUND_CHILD", "1")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    #[test]
    fn directory_that_vanishes_before_open_is_ignored() {
        use std::cell::Cell;

        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir(temp.join("victim")).unwrap();
        let root = capability(temp);
        let removed = Cell::new(false);
        let mut state = WalkState::default();

        measure_owned_tree_with_hooks(
            &root,
            &mut state,
            &|| std::time::Duration::ZERO,
            &|display| {
                if display == temp.join("victim") && !removed.replace(true) {
                    std::fs::remove_dir(display).unwrap();
                }
            },
            &|_| {},
        )
        .unwrap();

        assert!(removed.get());
        assert_eq!(state.owned_bytes, 0);
    }

    #[test]
    fn owned_byte_overflow_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::write(temp.join("one-byte"), b"x").unwrap();
        let root = capability(temp);
        let mut state = WalkState {
            owned_bytes: u64::MAX,
            ..WalkState::default()
        };

        let error =
            measure_owned_tree_with_elapsed(&root, &mut state, &|| std::time::Duration::ZERO)
                .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(state.owned_bytes, u64::MAX);
    }

    #[test]
    fn aggregates_files_across_owned_roots_and_queries_a_filesystem_once() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir(temp.join("snapshot")).unwrap();
        std::fs::create_dir(temp.join("worker")).unwrap();
        std::fs::write(temp.join("snapshot/a"), b"abcd").unwrap();
        std::fs::write(temp.join("worker/b"), b"123456").unwrap();
        let roots = vec![
            capability(&temp.join("snapshot")),
            capability(&temp.join("worker")),
        ];

        let reading = DiskMeter::new(roots, FixedSpace(99)).measure().unwrap();

        assert_eq!(reading.owned_bytes, 10);
        assert_eq!(
            reading.available_by_filesystem,
            BTreeMap::from([(FilesystemKey(7), 99)])
        );
        assert_eq!(reading.conservative_entries, cfg!(windows));
    }

    #[test]
    fn repeated_measurements_restart_directory_enumeration() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir(temp.join("nested")).unwrap();
        std::fs::write(temp.join("nested/data"), b"1234567").unwrap();
        let meter = DiskMeter::new(vec![capability(temp)], FixedSpace(99));

        let first = meter.measure().unwrap();
        let second = meter.measure().unwrap();

        assert_eq!(first.owned_bytes, second.owned_bytes);
        assert_eq!(
            first.available_by_filesystem,
            second.available_by_filesystem
        );
        assert_eq!(first.conservative_entries, second.conservative_entries);
        assert_eq!(second.owned_bytes, 7);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks_and_counts_hard_links_once() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = temp.join("root");
        let outside = temp.join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(root.join("data"), b"12345").unwrap();
        std::fs::hard_link(root.join("data"), root.join("alias")).unwrap();
        std::fs::write(outside.join("large"), vec![0_u8; 1_024]).unwrap();
        symlink(&outside, root.join("escape")).unwrap();

        let reading = DiskMeter::new(vec![capability(&root)], FixedSpace(99))
            .measure()
            .unwrap();

        assert_eq!(reading.owned_bytes, 5);
        assert!(!reading.conservative_entries);
    }

    #[test]
    fn rejects_a_tree_deeper_than_the_bound() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = temp.join("root");
        std::fs::create_dir(&root).unwrap();
        let mut cursor = root.clone();
        for _ in 0..129 {
            cursor.push("d");
            std::fs::create_dir(&cursor).unwrap();
        }

        let error = DiskMeter::new(vec![capability(&root)], FixedSpace(99))
            .measure()
            .unwrap_err();

        assert!(error.to_string().contains("depth"));
    }
}
