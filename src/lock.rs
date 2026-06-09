use std::env;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use fs2::FileExt;

pub(crate) const LOCK_EXCLUDE: &str = ".gnit/lock";

const DEFAULT_LOCK_TIMEOUT_MS: u64 = 2_000;
const DEFAULT_LOCK_RETRY_MS: u64 = 100;

pub(crate) struct WorkspaceLock {
    file: File,
}

impl WorkspaceLock {
    pub(crate) fn acquire(root: &Path) -> Result<Self> {
        acquire(root, WaitMode::Wait)
            .map(|lock| lock.expect("blocking acquisition should return a lock"))
    }

    pub(crate) fn try_acquire(root: &Path) -> Result<Option<Self>> {
        acquire(root, WaitMode::TryOnce)
    }
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn acquire(root: &Path, mode: WaitMode) -> Result<Option<WorkspaceLock>> {
    let gnit_dir = root.join(".gnit");
    fs::create_dir_all(&gnit_dir)
        .with_context(|| format!("create workspace lock directory {}", gnit_dir.display()))?;
    let path = gnit_dir.join("lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("open workspace lock {}", path.display()))?;

    match mode {
        WaitMode::TryOnce => match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(WorkspaceLock { file })),
            Err(err) if is_lock_contended(&err) => Ok(None),
            Err(err) => Err(err).with_context(|| format!("lock workspace {}", path.display())),
        },
        WaitMode::Wait => lock_with_retry(file, path).map(Some),
    }
}

fn lock_with_retry(file: File, path: PathBuf) -> Result<WorkspaceLock> {
    let timeout = lock_timeout();
    let retry = lock_retry();
    let started = Instant::now();

    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(WorkspaceLock { file }),
            Err(err) if is_lock_contended(&err) => {
                if started.elapsed() >= timeout {
                    bail!(
                        "another gnit process holds the workspace lock at {}; retry after it finishes",
                        path.display()
                    );
                }
                thread::sleep(retry.min(timeout.saturating_sub(started.elapsed())));
            }
            Err(err) => {
                return Err(err).with_context(|| format!("lock workspace {}", path.display()));
            }
        }
    }
}

fn is_lock_contended(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::AlreadyExists
    )
}

fn lock_timeout() -> Duration {
    env_duration("GNIT_LOCK_TIMEOUT_MS", DEFAULT_LOCK_TIMEOUT_MS)
}

fn lock_retry() -> Duration {
    let retry = env_duration("GNIT_LOCK_RETRY_MS", DEFAULT_LOCK_RETRY_MS);
    if retry.is_zero() {
        Duration::from_millis(1)
    } else {
        retry
    }
}

fn env_duration(name: &str, default_ms: u64) -> Duration {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(default_ms))
}

enum WaitMode {
    TryOnce,
    Wait,
}
