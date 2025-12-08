use flate2::read::GzDecoder;
use std::{
    fs::{self, File},
    io::{copy, Read},
    path::{Path, PathBuf},
};
use tar::Archive;
use zip::read::ZipArchive;

pub type ProgressCallback = dyn Fn(u64, u64);

/// Extract the zip and report progress.
pub fn extract_zip(
    zip_path: &Path,
    dest_dir: &Path,
    progress: &ProgressCallback,
    skip_top_folder: bool,
) -> std::io::Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let total_files = archive.len() as u64;

    for i in 0..archive.len() {
        let mut zip_file = archive.by_index(i)?;
        let mut zip_path = zip_file.mangled_name();

        if skip_top_folder {
            let components: Vec<_> = zip_path.components().collect();
            if components.len() > 1 {
                zip_path = components[1..].iter().collect();
            }
        }

        let outpath = dest_dir.join(zip_path);

        if zip_file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            copy(&mut zip_file, &mut outfile)?;
        }

        progress(i as u64 + 1, total_files);
    }

    Ok(())
}

/// Extract the tar.gz file and report progress.
pub fn extract_tar_gz(
    tar_path: &Path,
    dest_dir: &Path,
    progress: &ProgressCallback,
    skip_top_folder: bool,
) -> std::io::Result<()> {
    let file = File::open(tar_path)?;
    let metadata = file.metadata()?;
    let total_size = metadata.len();

    let mut processed: u64 = 0;
    let mut reader = ProgressReader::new(file, total_size, progress, &mut processed);

    let gz = GzDecoder::new(&mut reader);
    let mut archive = Archive::new(gz);

    for entry in archive.entries()? {
        let mut entry = entry?;

        let mut path = entry.path()?.to_path_buf();
        if skip_top_folder {
            if let Some(stripped) = strip_top_level(&path) {
                path = stripped;
            } else {
                continue;
            }
        }

        let dest_path = dest_dir.join(path);
        entry.unpack(dest_path)?;
    }

    Ok(())
}

struct ProgressReader<'a, R: Read> {
    inner: R,
    total: u64,
    processed: &'a mut u64,
    cb: &'a ProgressCallback,
}

impl<'a, R: Read> ProgressReader<'a, R> {
    fn new(inner: R, total: u64, cb: &'a ProgressCallback, processed: &'a mut u64) -> Self {
        Self { inner, total, processed, cb }
    }
}

impl<'a, R: Read> Read for ProgressReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        *self.processed += n as u64;
        (self.cb)(*self.processed, self.total);
        Ok(n)
    }
}

/// Strip the top-level component from a path.
fn strip_top_level(path: &Path) -> Option<PathBuf> {
    let mut comps = path.components();
    comps.next()?; // skip top folder
    Some(comps.as_path().to_path_buf())
}
