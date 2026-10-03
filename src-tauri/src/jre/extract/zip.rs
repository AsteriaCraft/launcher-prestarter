//! Windows JRE archives (`.zip`). Entry names go through `enclosed_name` and our own check; symlinks are refused
//! (Liberica's Windows zips have none); every file must have exactly the size the central directory announces.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{ExtractError, Limits, guard};

pub fn extract(
    archive: &Path,
    dest: &Path,
    limits: &Limits,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), ExtractError> {
    let mut zip = ::zip::ZipArchive::new(File::open(archive)?).map_err(|e| ExtractError::Corrupt(e.to_string()))?;
    let count = zip.len();
    if count as u64 > limits.max_entries {
        return Err(ExtractError::TooBig(format!("{count} entries")));
    }
    let mut total: u64 = 0;
    for index in 0..count {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExtractError::Cancelled);
        }
        let mut entry = zip.by_index(index).map_err(|e| ExtractError::Corrupt(e.to_string()))?;
        let name = entry.name().to_owned();
        let relative = entry
            .enclosed_name()
            .and_then(|p| guard::safe_relative(&p))
            .ok_or_else(|| ExtractError::UnsafePath(name.clone()))?;
        if entry.is_symlink() {
            return Err(ExtractError::UnsafeLink(name));
        }
        let out_path = dest.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        let expected = entry.size();
        total = total.saturating_add(expected);
        if total > limits.max_bytes {
            return Err(ExtractError::TooBig(format!("more than {} bytes", limits.max_bytes)));
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&out_path)?;
        let copied = io::copy(&mut (&mut entry).take(expected + 1), &mut out)?;
        if copied != expected {
            return Err(ExtractError::Corrupt(format!("{name}: {copied} bytes instead of {expected}")));
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&out_path, fs::Permissions::from_mode(mode & 0o777))?;
        }
        progress(index as u64 + 1, count as u64);
    }
    Ok(())
}
