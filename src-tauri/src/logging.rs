//! JSON log to stderr and to a size-capped file. A release build on Windows has no
//! console, so the file is the only record there.
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
static LOG: OnceLock<Mutex<LogFile>> = OnceLock::new();
static FAILED: AtomicBool = AtomicBool::new(false);

struct LogFile {
    path: PathBuf,
    file: File,
    written: u64,
}
impl LogFile {
    fn open(path: PathBuf) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata()?.len();
        Ok(Self {
            path,
            file,
            written,
        })
    }
    /// Rotates before a write would cross the cap, so the file never exceeds it in a long session.
    fn append(&mut self, buffer: &[u8]) -> std::io::Result<()> {
        if self.written > 0 && self.written + buffer.len() as u64 > MAX_LOG_BYTES {
            std::fs::rename(&self.path, self.path.with_extension("log.1"))?;
            *self = Self::open(self.path.clone())?;
        }
        self.file.write_all(buffer)?;
        self.written += buffer.len() as u64;
        Ok(())
    }
}

/// Logging must never take the app down, but a broken log file must not go unnoticed either:
/// the first failure is reported on stderr and exposed to the UI through `failed()`.
fn mark_failed(error: &std::io::Error) {
    if !FAILED.swap(true, Ordering::SeqCst) {
        let _ = writeln!(std::io::stderr(), "kalbion: log file unavailable: {error}");
    }
}
pub struct Writer;
impl Write for Writer {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buffer);
        if let Some(log) = LOG.get() {
            match log.lock() {
                Ok(mut log) => {
                    if let Err(error) = log.append(buffer) {
                        mark_failed(&error);
                    }
                }
                Err(_) => mark_failed(&std::io::Error::other("log lock poisoned")),
            }
        }
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn open(directory: &Path) -> std::io::Result<()> {
    let result = std::fs::create_dir_all(directory)
        .and_then(|()| LogFile::open(directory.join("kalbion.log")))
        .map(|file| {
            let _ = LOG.set(Mutex::new(file));
        });
    if let Err(error) = &result {
        mark_failed(error);
    }
    result
}
pub fn path() -> Option<PathBuf> {
    LOG.get()
        .and_then(|log| log.lock().ok().map(|log| log.path.clone()))
}
pub fn failed() -> bool {
    FAILED.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_during_a_session_instead_of_growing_past_the_cap() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("kalbion.log");
        let mut log = LogFile::open(path.clone()).unwrap();
        let line = vec![b'x'; 1024 * 1024];
        for _ in 0..12 {
            log.append(&line).unwrap();
        }
        let current = std::fs::metadata(&path).unwrap().len();
        let previous = std::fs::metadata(path.with_extension("log.1"))
            .unwrap()
            .len();
        assert!(current <= MAX_LOG_BYTES && previous <= MAX_LOG_BYTES);
        assert_eq!(current + previous, 7 * 1024 * 1024);
    }
}
