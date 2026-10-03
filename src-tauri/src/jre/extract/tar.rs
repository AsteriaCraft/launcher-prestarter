//! Linux and macOS JRE archives (`.tar.gz`). Every entry path is checked, symlink targets must stay inside the tree,
//! devices and FIFOs are refused, and the write itself goes through `unpack_in` (which also refuses to write through
//! a symlinked parent). Unix permissions are kept, so `bin/java` stays executable.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

use super::{ExtractError, Limits, guard};

struct Counting<R> {
    inner: R,
    read: Arc<AtomicU64>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

pub fn extract(
    archive: &Path,
    dest: &Path,
    limits: &Limits,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), ExtractError> {
    let file = File::open(archive)?;
    let compressed = file.metadata()?.len();
    let read = Arc::new(AtomicU64::new(0));
    let reader = Counting { inner: io::BufReader::with_capacity(1 << 20, file), read: Arc::clone(&read) };
    let mut tar = Archive::new(GzDecoder::new(reader));
    tar.set_preserve_permissions(true);
    tar.set_preserve_mtime(true);
    tar.set_unpack_xattrs(false);
    tar.set_overwrite(true);

    let mut entries: u64 = 0;
    let mut total: u64 = 0;
    for entry in tar.entries().map_err(|e| ExtractError::Corrupt(e.to_string()))? {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExtractError::Cancelled);
        }
        let mut entry = entry.map_err(|e| ExtractError::Corrupt(e.to_string()))?;
        entries += 1;
        if entries > limits.max_entries {
            return Err(ExtractError::TooBig(format!("more than {} entries", limits.max_entries)));
        }
        let raw_path = entry.path().map_err(|e| ExtractError::Corrupt(e.to_string()))?.into_owned();
        let shown = raw_path.display().to_string();
        let relative = guard::safe_relative(&raw_path).ok_or_else(|| ExtractError::UnsafePath(shown.clone()))?;
        match entry.header().entry_type() {
            EntryType::Regular | EntryType::Continuous | EntryType::Directory => {}
            EntryType::Symlink => {
                let target = entry
                    .link_name()
                    .map_err(|e| ExtractError::Corrupt(e.to_string()))?
                    .ok_or_else(|| ExtractError::UnsafeLink(shown.clone()))?;
                if !guard::symlink_stays_inside(&relative, &target) {
                    return Err(ExtractError::UnsafeLink(format!("{shown} -> {}", target.display())));
                }
            }
            EntryType::Link => {
                let target = entry
                    .link_name()
                    .map_err(|e| ExtractError::Corrupt(e.to_string()))?
                    .ok_or_else(|| ExtractError::UnsafeLink(shown.clone()))?;
                if guard::safe_relative(&target).is_none() {
                    return Err(ExtractError::UnsafeLink(format!("{shown} => {}", target.display())));
                }
            }
            EntryType::XGlobalHeader | EntryType::XHeader | EntryType::GNULongName | EntryType::GNULongLink => continue,
            other => return Err(ExtractError::UnsafePath(format!("{shown} ({other:?})"))),
        }
        total = total.saturating_add(entry.header().size().unwrap_or(0));
        if total > limits.max_bytes {
            return Err(ExtractError::TooBig(format!("more than {} bytes", limits.max_bytes)));
        }
        if !entry.unpack_in(dest)? {
            return Err(ExtractError::UnsafePath(shown));
        }
        progress(read.load(Ordering::Relaxed), compressed);
    }
    Ok(())
}
