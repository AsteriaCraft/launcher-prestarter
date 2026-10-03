//! Unpacking a JRE archive into a staging directory (ADR 0005). Archives come from a checked download, but the
//! unpacker still treats them as hostile: no path may leave the staging directory.

pub mod guard;
pub mod tar;
pub mod zip;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use thiserror::Error;

use super::catalog::Package;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("archive entry escapes the target directory: {0}")]
    UnsafePath(String),
    #[error("archive link points outside the target directory: {0}")]
    UnsafeLink(String),
    #[error("archive is damaged: {0}")]
    Corrupt(String),
    #[error("archive is larger than allowed: {0}")]
    TooBig(String),
    #[error("archive must contain exactly one top-level directory, found {0}")]
    Layout(usize),
    #[error("unpacking was cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_bytes: u64,
    pub max_entries: u64,
}

/// A JRE unpacks to ~300-450 MB in ~1,000-3,000 entries; these bounds stop a decompression bomb.
pub const JRE_LIMITS: Limits = Limits { max_bytes: 2 * 1024 * 1024 * 1024, max_entries: 50_000 };

/// Unpacks `archive` into `staging` (which must be empty or absent) and returns the JRE root: the single top-level
/// directory of the archive (`jre-25.0.4.1-full` or `jre-25.0.4.1-full.jre`).
pub fn extract(
    archive: &Path,
    package: Package,
    staging: &Path,
    limits: &Limits,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, ExtractError> {
    fs::create_dir_all(staging)?;
    match package {
        Package::Zip => zip::extract(archive, staging, limits, cancel, progress)?,
        Package::TarGz => tar::extract(archive, staging, limits, cancel, progress)?,
    }
    let top: Vec<PathBuf> = fs::read_dir(staging)?.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>()?;
    match top.as_slice() {
        [single] if single.is_dir() => Ok(single.clone()),
        _ => Err(ExtractError::Layout(top.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::atomic::unique_suffix;
    use std::io::Write;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("asterium-extract-{tag}-{}", unique_suffix()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut writer = ::zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = ::zip::write::SimpleFileOptions::default().compression_method(::zip::CompressionMethod::Stored);
        for (name, data) in files {
            if name.ends_with('/') {
                writer.add_directory(*name, options).unwrap();
            } else {
                writer.start_file(*name, options).unwrap();
                writer.write_all(data).unwrap();
            }
        }
        writer.finish().unwrap();
    }

    /// A tar entry written byte by byte, so hostile names (`..`, `/`) can be produced.
    fn raw_entry(name: &str, kind: u8, link: &str, data: &[u8]) -> Vec<u8> {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..107].copy_from_slice(b"0000755");
        header[108..115].copy_from_slice(b"0000000");
        header[116..123].copy_from_slice(b"0000000");
        let size = format!("{:011o}", data.len());
        header[124..135].copy_from_slice(size.as_bytes());
        header[136..147].copy_from_slice(b"00000000000");
        header[156] = kind;
        header[157..157 + link.len()].copy_from_slice(link.as_bytes());
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|b| *b as u32).sum();
        header[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        let mut out = header.to_vec();
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(512) * 512, 0);
        out
    }

    fn make_tar_gz(path: &Path, entries: &[Vec<u8>]) {
        let mut gz = flate2::write::GzEncoder::new(fs::File::create(path).unwrap(), flate2::Compression::fast());
        for entry in entries {
            gz.write_all(entry).unwrap();
        }
        gz.write_all(&[0u8; 1024]).unwrap();
        gz.finish().unwrap();
    }

    fn run(archive: &Path, package: Package, staging: &Path) -> Result<PathBuf, ExtractError> {
        extract(archive, package, staging, &JRE_LIMITS, &AtomicBool::new(false), &mut |_, _| {})
    }

    #[test]
    fn zip_with_one_top_directory() {
        let dir = temp_dir("zip-ok");
        let archive = dir.join("jre.zip");
        make_zip(
            &archive,
            &[("jre-25/", b""), ("jre-25/bin/javaw.exe", b"MZ"), ("jre-25/release", b"JAVA_VERSION=\"25\"\n")],
        );
        let root = run(&archive, Package::Zip, &dir.join("staging")).unwrap();
        assert_eq!(root.file_name().unwrap(), "jre-25");
        assert_eq!(fs::read(root.join("bin/javaw.exe")).unwrap(), b"MZ");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn zip_slip_is_refused() {
        for name in ["../evil.txt", "jre/../../evil.txt"] {
            let dir = temp_dir("zip-slip");
            let archive = dir.join("bad.zip");
            make_zip(&archive, &[("jre/ok", b"x"), (name, b"evil")]);
            let err = run(&archive, Package::Zip, &dir.join("staging")).unwrap_err();
            assert!(matches!(err, ExtractError::UnsafePath(_)), "{name}: {err}");
            assert!(!dir.join("evil.txt").exists());
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn two_top_level_entries_are_refused() {
        let dir = temp_dir("zip-two");
        let archive = dir.join("two.zip");
        make_zip(&archive, &[("a/x", b"1"), ("b/y", b"2")]);
        assert!(matches!(run(&archive, Package::Zip, &dir.join("s")), Err(ExtractError::Layout(2))));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tar_with_files_dirs_and_inside_symlinks() {
        let dir = temp_dir("tar-ok");
        let archive = dir.join("jre.tar.gz");
        let mut entries = vec![
            raw_entry("jre/", b'5', "", b""),
            raw_entry("jre/bin/", b'5', "", b""),
            raw_entry("jre/bin/java", b'0', "", b"#!java"),
            raw_entry("jre/legal/java.base/LICENSE", b'0', "", b"gpl"),
        ];
        if cfg!(unix) {
            entries.push(raw_entry("jre/legal/java.prefs/LICENSE", b'2', "../java.base/LICENSE", b""));
        }
        make_tar_gz(&archive, &entries);
        let root = run(&archive, Package::TarGz, &dir.join("staging")).unwrap();
        assert_eq!(fs::read(root.join("bin/java")).unwrap(), b"#!java");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(root.join("bin/java")).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "bin/java stays executable");
            assert_eq!(fs::read(root.join("legal/java.prefs/LICENSE")).unwrap(), b"gpl");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hostile_tar_entries_are_refused() {
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("parent", raw_entry("jre/../../evil", b'0', "", b"x")),
            ("absolute", raw_entry("/tmp/asterium-evil", b'0', "", b"x")),
            ("symlink out", raw_entry("jre/link", b'2', "../../../etc", b"")),
            ("absolute symlink", raw_entry("jre/link", b'2', "/etc/passwd", b"")),
            ("hard link out", raw_entry("jre/hard", b'1', "../outside", b"")),
            ("device", raw_entry("jre/dev", b'3', "", b"")),
            ("fifo", raw_entry("jre/fifo", b'6', "", b"")),
        ];
        for (label, entry) in cases {
            let dir = temp_dir("tar-bad");
            let archive = dir.join("bad.tar.gz");
            make_tar_gz(&archive, &[raw_entry("jre/", b'5', "", b""), entry]);
            let err = run(&archive, Package::TarGz, &dir.join("staging")).unwrap_err();
            assert!(
                matches!(err, ExtractError::UnsafePath(_) | ExtractError::UnsafeLink(_)),
                "{label}: unexpected {err}"
            );
            assert!(!dir.join("evil").exists(), "{label}");
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn a_truncated_tar_is_corrupt() {
        let dir = temp_dir("tar-trunc");
        let archive = dir.join("t.tar.gz");
        make_tar_gz(&archive, &[raw_entry("jre/bin/java", b'0', "", &[7u8; 4096])]);
        let bytes = fs::read(&archive).unwrap();
        fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();
        let err = run(&archive, Package::TarGz, &dir.join("staging")).unwrap_err();
        assert!(matches!(err, ExtractError::Corrupt(_) | ExtractError::Io(_)), "{err}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn limits_stop_a_bomb() {
        let dir = temp_dir("bomb");
        let archive = dir.join("b.zip");
        make_zip(&archive, &[("jre/a", &[0u8; 2048]), ("jre/b", &[0u8; 2048])]);
        let small = Limits { max_bytes: 3000, max_entries: 100 };
        let err = extract(&archive, Package::Zip, &dir.join("s"), &small, &AtomicBool::new(false), &mut |_, _| {})
            .unwrap_err();
        assert!(matches!(err, ExtractError::TooBig(_)));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cancel_stops_unpacking() {
        let dir = temp_dir("cancel");
        let archive = dir.join("c.zip");
        make_zip(&archive, &[("jre/a", b"1")]);
        let err = extract(&archive, Package::Zip, &dir.join("s"), &JRE_LIMITS, &AtomicBool::new(true), &mut |_, _| {})
            .unwrap_err();
        assert!(matches!(err, ExtractError::Cancelled));
        fs::remove_dir_all(dir).unwrap();
    }
}
