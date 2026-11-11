//! How ShellRS's own files are read and written: plainly, until the app
//! installs something that seals them.

use std::io::{self, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

/// Whole files, read and replaced in one step each.
pub trait Files: Send + Sync + 'static {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
}

static FILES: OnceLock<Arc<dyn Files>> = OnceLock::new();

/// Read and write through `files` from now on. Once per process.
pub fn install(files: Arc<dyn Files>) {
    let _ = FILES.set(files);
}

pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    match FILES.get() {
        Some(files) => files.read(path),
        None => std::fs::read(path),
    }
}

/// Replace the file at `path`, making its folder if need be.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    match FILES.get() {
        Some(files) => files.write(path, bytes),
        None => replace(path, bytes),
    }
}

/// Replace a file through a temporary one beside it, so that a reader or a
/// crash sees the old file or the new one, never half of either. Only the
/// user may read it.
pub fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let mut name = path.file_name().unwrap_or_default().to_owned();
    // A name of its own, so that two writers of one file never share it.
    name.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = path.with_file_name(name);
    let written = private_file(&temporary).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    let result = written.and_then(|()| std::fs::rename(&temporary, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn private_file(path: &Path) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path)
}
