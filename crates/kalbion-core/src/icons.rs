//! Item icons from the official Albion render service, cached on disk so icons already seen
//! keep working offline. Only one fixed host is contacted, and the item ID is validated
//! before it becomes part of a URL or a file name.
use crate::domain::{parse_unique_name, validate_quality};
use crate::error::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

const RENDER_URL: &str = "https://render.albiononline.com/v1/item";
const ICON_SIZE: u32 = 64;
const MAX_ICON_BYTES: u64 = 512 * 1024;
/// The first render of a new item/quality can take several seconds on the service side.
const TIMEOUT: Duration = Duration::from_secs(15);
/// After a timeout or network error, wait before asking again; the render usually
/// finishes server-side in the meantime.
const RETRY_FAILED_AFTER: Duration = Duration::from_secs(120);
const FRESH_FOR: Duration = Duration::from_secs(30 * 24 * 3600);
/// The service answers unknown items slowly (~25 s), so a 404 is remembered on disk.
const MISSING_RETRY_AFTER: Duration = Duration::from_secs(7 * 24 * 3600);
const MAX_CONCURRENT_DOWNLOADS: usize = 6;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

pub struct IconCache {
    directory: PathBuf,
    base_url: String,
    agent: ureq::Agent,
    permits: Permits,
    /// Keys that recently failed for a reason other than 404 (timeout, offline), so a
    /// repaint does not trigger a new download for every visible row.
    recent_failures: Mutex<HashMap<String, Instant>>,
    failure_logged: AtomicBool,
}

enum Download {
    Found(Vec<u8>),
    Missing,
}

impl IconCache {
    pub fn new(directory: PathBuf) -> std::io::Result<Self> {
        Self::with_source(directory, RENDER_URL.into(), true)
    }
    /// Test seam: lets tests point at a local plain-HTTP server.
    #[doc(hidden)]
    pub fn with_source(
        directory: PathBuf,
        base_url: String,
        https_only: bool,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&directory)?;
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .https_only(https_only)
            .max_redirects(0)
            .user_agent(concat!("Kalbion/", env!("CARGO_PKG_VERSION")))
            .build();
        Ok(Self {
            directory,
            base_url,
            agent: ureq::Agent::new_with_config(config),
            permits: Permits::new(MAX_CONCURRENT_DOWNLOADS),
            recent_failures: Mutex::new(HashMap::new()),
            failure_logged: AtomicBool::new(false),
        })
    }

    /// Returns the PNG, or `None` when the icon is unavailable (unknown item, offline,
    /// service error); the UI then shows its generic icon. A stale cached icon is preferred
    /// over nothing when the service cannot be reached.
    pub fn get(&self, item_id: &str, quality: Option<u8>) -> Result<Option<Vec<u8>>> {
        parse_unique_name(item_id)?;
        validate_quality(quality)?;
        let key = format!("{item_id}_q{}", quality.unwrap_or(0));
        let file = self.directory.join(format!("{key}.png"));
        let marker = self.directory.join(format!("{key}.missing"));
        let cached = std::fs::read(&file).ok();
        if cached.is_some() && age(&file).is_some_and(|age| age < FRESH_FOR) {
            return Ok(cached);
        }
        if age(&marker).is_some_and(|age| age < MISSING_RETRY_AFTER)
            || self
                .lock_failures()
                .get(&key)
                .is_some_and(|failed| failed.elapsed() < RETRY_FAILED_AFTER)
        {
            return Ok(cached);
        }
        let _permit = self.permits.acquire();
        match self.download(item_id, quality) {
            Ok(Download::Found(bytes)) => {
                if let Err(error) = write_atomically(&file, &bytes) {
                    tracing::warn!(operation = "icon_cache_write_failed", cause = %error);
                }
                let _ = std::fs::remove_file(&marker);
                Ok(Some(bytes))
            }
            Ok(Download::Missing) => {
                let _ = std::fs::write(&marker, b"");
                Ok(cached)
            }
            Err(error) => {
                self.lock_failures().insert(key, Instant::now());
                if !self.failure_logged.swap(true, Ordering::SeqCst) {
                    tracing::warn!(
                        operation = "icon_download_failed",
                        cause = %error,
                        note = "further icon failures in this run are not logged"
                    );
                }
                Ok(cached)
            }
        }
    }
    fn lock_failures(&self) -> std::sync::MutexGuard<'_, HashMap<String, Instant>> {
        self.recent_failures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
    fn download(
        &self,
        item_id: &str,
        quality: Option<u8>,
    ) -> std::result::Result<Download, String> {
        let mut url = format!(
            "{}/{}.png?size={ICON_SIZE}",
            self.base_url,
            item_id.replace('@', "%40")
        );
        if let Some(quality) = quality {
            url.push_str(&format!("&quality={quality}"));
        }
        let mut response = self
            .agent
            .get(&url)
            .call()
            .map_err(|error| error.to_string())?;
        match response.status().as_u16() {
            200 => {}
            404 => return Ok(Download::Missing),
            status => return Err(format!("HTTP {status}")),
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_ICON_BYTES)
            .read_to_vec()
            .map_err(|error| error.to_string())?;
        if !bytes.starts_with(PNG_SIGNATURE) {
            return Err("resposta não é PNG".into());
        }
        Ok(Download::Found(bytes))
    }
}

fn age(path: &Path) -> Option<Duration> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
}
/// A crash mid-write must never leave a truncated PNG that later looks valid.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

struct Permits {
    available: Mutex<usize>,
    released: Condvar,
}
struct Permit<'a>(&'a Permits);
impl Permits {
    fn new(count: usize) -> Self {
        Self {
            available: Mutex::new(count),
            released: Condvar::new(),
        }
    }
    fn acquire(&self) -> Permit<'_> {
        let mut available = self
            .available
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while *available == 0 {
            available = self
                .released
                .wait(available)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *available -= 1;
        Permit(self)
    }
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self
            .0
            .available
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        self.0.released.notify_one();
    }
}
