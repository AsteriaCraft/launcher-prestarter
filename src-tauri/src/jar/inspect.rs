//! Reads just enough of a zip to know whether `java -jar` will accept it (ADR 0001): the end-of-central-directory
//! record, the central directory, and `META-INF/MANIFEST.MF` with a `Main-Class`.
//!
//! The jar may sit at the end of another file (LaunchServer appends `Asterium.jar` to the raw prestarter), so its
//! offsets are relative to where the jar starts: `prefix = (EOCD position - central directory size) - central
//! directory offset`, the same arithmetic the JDK uses. Java's launcher only accepts an EOCD whose comment length
//! covers every byte after it (that is how Gravit's Authenticode module keeps a signed exe a valid jar), so a valid
//! zip with stray bytes after it is reported as [`JarError::TrailingData`], not as "no jar".

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use flate2::read::DeflateDecoder;
use thiserror::Error;

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const CEN_SIGNATURE: u32 = 0x0201_4b50;
const LOC_SIGNATURE: u32 = 0x0403_4b50;
const EOCD_LEN: u64 = 22;
const MAX_COMMENT: u64 = 0xFFFF;
const MAX_CENTRAL_DIRECTORY: u64 = 16 * 1024 * 1024;
const MAX_MANIFEST: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarInfo {
    /// Bytes before the jar (the prestarter itself); 0 for a plain jar.
    pub prefix_len: u64,
    pub entries: u16,
    pub main_class: String,
}

#[derive(Debug, Error)]
pub enum JarError {
    #[error("no zip archive at the end of the file")]
    NoJar,
    #[error("a jar is present but followed by {0} bytes that its EOCD comment does not cover (java -jar refuses it)")]
    TrailingData(u64),
    #[error("the jar is damaged: {0}")]
    Corrupt(String),
    #[error("the jar has no META-INF/MANIFEST.MF")]
    NoManifest,
    #[error("the jar manifest has no Main-Class")]
    NoMainClass,
    #[error(transparent)]
    Io(#[from] io::Error),
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

pub fn inspect_file(path: &Path) -> Result<JarInfo, JarError> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    inspect(&mut file, len)
}

struct Eocd {
    position: u64,
    entries: u16,
    cen_size: u64,
    cen_offset: u64,
    comment_len: u64,
}

pub fn inspect<R: Read + Seek>(reader: &mut R, len: u64) -> Result<JarInfo, JarError> {
    if len < EOCD_LEN {
        return Err(JarError::NoJar);
    }
    // Java scans at most comment + EOCD from the end; scan a little more to recognise a jar with trailing bytes.
    let tail_len = len.min(MAX_COMMENT + EOCD_LEN + 64 * 1024);
    let tail_start = len - tail_len;
    let mut tail = vec![0u8; tail_len as usize];
    reader.seek(SeekFrom::Start(tail_start))?;
    reader.read_exact(&mut tail)?;

    let mut trailing_candidate: Option<u64> = None;
    let mut i = tail.len() as i64 - EOCD_LEN as i64;
    while i >= 0 {
        let at = i as usize;
        if u32_at(&tail, at) == EOCD_SIGNATURE {
            let eocd = Eocd {
                position: tail_start + at as u64,
                entries: u16_at(&tail, at + 10),
                cen_size: u32_at(&tail, at + 12) as u64,
                cen_offset: u32_at(&tail, at + 16) as u64,
                comment_len: u16_at(&tail, at + 20) as u64,
            };
            let disk_ok = u16_at(&tail, at + 4) == 0 && u16_at(&tail, at + 6) == 0;
            if disk_ok && let Some(info) = read_central_directory(reader, &eocd)? {
                let after = len - (eocd.position + EOCD_LEN);
                if after == eocd.comment_len {
                    return info;
                }
                trailing_candidate.get_or_insert(after.saturating_sub(eocd.comment_len));
            }
        }
        i -= 1;
    }
    Err(trailing_candidate.map_or(JarError::NoJar, JarError::TrailingData))
}

/// `Ok(None)`: this EOCD candidate does not point at a central directory (a chance match inside a binary).
/// `Ok(Some(result))`: a real zip; `result` says whether it is a usable jar.
fn read_central_directory<R: Read + Seek>(
    reader: &mut R,
    eocd: &Eocd,
) -> io::Result<Option<Result<JarInfo, JarError>>> {
    if eocd.cen_size > MAX_CENTRAL_DIRECTORY || eocd.cen_size > eocd.position || eocd.entries == 0 {
        return Ok(None);
    }
    let cen_start = eocd.position - eocd.cen_size;
    let Some(prefix_len) = cen_start.checked_sub(eocd.cen_offset) else { return Ok(None) };
    let mut cen = vec![0u8; eocd.cen_size as usize];
    reader.seek(SeekFrom::Start(cen_start))?;
    reader.read_exact(&mut cen)?;
    if cen.len() < 46 || u32_at(&cen, 0) != CEN_SIGNATURE {
        return Ok(None);
    }

    let mut manifest: Option<(u16, u64, u64)> = None; // method, compressed size, local header offset
    let mut at = 0usize;
    for _ in 0..eocd.entries {
        if at + 46 > cen.len() || u32_at(&cen, at) != CEN_SIGNATURE {
            return Ok(Some(Err(JarError::Corrupt("central directory entry out of place".into()))));
        }
        let method = u16_at(&cen, at + 10);
        let compressed = u32_at(&cen, at + 20) as u64;
        let name_len = u16_at(&cen, at + 28) as usize;
        let extra_len = u16_at(&cen, at + 30) as usize;
        let comment_len = u16_at(&cen, at + 32) as usize;
        let local_offset = u32_at(&cen, at + 42) as u64;
        let name_end = at + 46 + name_len;
        if name_end > cen.len() {
            return Ok(Some(Err(JarError::Corrupt("central directory name out of range".into()))));
        }
        if cen[at + 46..name_end].eq_ignore_ascii_case(b"META-INF/MANIFEST.MF") {
            manifest = Some((method, compressed, local_offset));
        }
        at = name_end + extra_len + comment_len;
    }
    let Some((method, compressed, local_offset)) = manifest else { return Ok(Some(Err(JarError::NoManifest))) };
    Ok(Some(read_manifest(reader, prefix_len, method, compressed, local_offset).map(|main_class| JarInfo {
        prefix_len,
        entries: eocd.entries,
        main_class,
    })))
}

fn read_manifest<R: Read + Seek>(
    reader: &mut R,
    prefix_len: u64,
    method: u16,
    compressed: u64,
    local_offset: u64,
) -> Result<String, JarError> {
    if compressed > MAX_MANIFEST {
        return Err(JarError::Corrupt("manifest too large".into()));
    }
    let mut local = [0u8; 30];
    reader.seek(SeekFrom::Start(prefix_len + local_offset))?;
    reader.read_exact(&mut local)?;
    if u32_at(&local, 0) != LOC_SIGNATURE {
        return Err(JarError::Corrupt("manifest local header not found".into()));
    }
    let skip = u16_at(&local, 26) as i64 + u16_at(&local, 28) as i64;
    reader.seek(SeekFrom::Current(skip))?;
    let mut data = vec![0u8; compressed as usize];
    reader.read_exact(&mut data)?;
    let text = match method {
        0 => data,
        8 => {
            let mut out = Vec::new();
            DeflateDecoder::new(&data[..])
                .take(MAX_MANIFEST)
                .read_to_end(&mut out)
                .map_err(|e| JarError::Corrupt(format!("manifest: {e}")))?;
            out
        }
        other => return Err(JarError::Corrupt(format!("manifest compression method {other}"))),
    };
    main_class(&String::from_utf8_lossy(&text)).ok_or(JarError::NoMainClass)
}

/// `Main-Class` from a manifest (header names are case-insensitive; continuation lines start with a space).
pub fn main_class(manifest: &str) -> Option<String> {
    let mut joined: Vec<String> = Vec::new();
    for line in manifest.split(['\n']).map(|l| l.trim_end_matches('\r')) {
        if let Some(rest) = line.strip_prefix(' ') {
            if let Some(last) = joined.last_mut() {
                last.push_str(rest);
            }
        } else {
            joined.push(line.to_owned());
        }
    }
    joined.iter().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim().eq_ignore_ascii_case("Main-Class")).then(|| value.trim().to_owned()).filter(|v| !v.is_empty())
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    /// A minimal jar built in memory: a manifest (deflated or stored) and one class file.
    pub(crate) fn jar_bytes(manifest: Option<&str>, deflate: bool) -> Vec<u8> {
        let mut writer = ::zip::ZipWriter::new(Cursor::new(Vec::new()));
        let method = if deflate { ::zip::CompressionMethod::Deflated } else { ::zip::CompressionMethod::Stored };
        let options = ::zip::write::SimpleFileOptions::default().compression_method(method);
        if let Some(text) = manifest {
            writer.start_file("META-INF/MANIFEST.MF", options).unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.start_file("pro/gravit/Main.class", options).unwrap();
        writer.write_all(&[0xCA, 0xFE, 0xBA, 0xBE]).unwrap();
        writer.finish().unwrap().into_inner()
    }

    pub(crate) const MANIFEST: &str =
        "Manifest-Version: 1.0\r\nMain-Class: pro.gravit.launcher.start.ClientLauncherWrapper\r\n\r\n";

    fn check(bytes: &[u8]) -> Result<JarInfo, JarError> {
        inspect(&mut Cursor::new(bytes), bytes.len() as u64)
    }

    #[test]
    fn plain_jar_deflated_and_stored() {
        for deflate in [true, false] {
            let info = check(&jar_bytes(Some(MANIFEST), deflate)).unwrap();
            assert_eq!(info.prefix_len, 0);
            assert_eq!(info.entries, 2);
            assert_eq!(info.main_class, "pro.gravit.launcher.start.ClientLauncherWrapper");
        }
    }

    #[test]
    fn jar_appended_to_an_executable() {
        let mut exe = b"MZ\x90\x00".to_vec();
        exe.extend(std::iter::repeat_n(0x41u8, 100_000));
        let prefix = exe.len() as u64;
        exe.extend(jar_bytes(Some(MANIFEST), true));
        let info = check(&exe).unwrap();
        assert_eq!(info.prefix_len, prefix);
    }

    #[test]
    fn executable_without_jar() {
        let exe: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        assert!(matches!(check(&exe), Err(JarError::NoJar)));
        assert!(matches!(check(b"tiny"), Err(JarError::NoJar)));
        // A stray EOCD signature that points nowhere is not a jar either.
        let mut fake = vec![0u8; 1000];
        fake.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 1, 0, 1, 0, 0xff, 0, 0, 0, 0x10, 0, 0, 0, 0, 0]);
        assert!(matches!(check(&fake), Err(JarError::NoJar)));
    }

    #[test]
    fn authenticode_tail_covered_by_the_comment_is_fine() {
        let mut jar = jar_bytes(Some(MANIFEST), true);
        let tail = vec![0x30u8; 5000]; // what a PKCS#7 signature table looks like to the zip reader
        let eocd = jar.len() - 22;
        jar[eocd + 20..eocd + 22].copy_from_slice(&(tail.len() as u16).to_le_bytes());
        jar.extend_from_slice(&tail);
        assert!(check(&jar).is_ok());
    }

    #[test]
    fn a_tail_not_covered_by_the_comment_is_reported() {
        let mut jar = jar_bytes(Some(MANIFEST), true);
        jar.extend_from_slice(&[0x30u8; 5000]);
        assert!(matches!(check(&jar), Err(JarError::TrailingData(5000))));
    }

    #[test]
    fn manifest_problems() {
        assert!(matches!(check(&jar_bytes(None, true)), Err(JarError::NoManifest)));
        assert!(matches!(check(&jar_bytes(Some("Manifest-Version: 1.0\n\n"), true)), Err(JarError::NoMainClass)));
    }

    #[test]
    fn a_truncated_jar_is_not_accepted() {
        let jar = jar_bytes(Some(MANIFEST), true);
        assert!(check(&jar[..jar.len() - 10]).is_err());
        assert!(check(&jar[jar.len() / 2..]).is_err());
    }

    #[test]
    fn main_class_parsing() {
        assert_eq!(main_class("main-class: a.B\n").as_deref(), Some("a.B"));
        assert_eq!(
            main_class("Main-Class: pro.gravit.launcher.start.Client\r\n LauncherWrapper\r\n").as_deref(),
            Some("pro.gravit.launcher.start.ClientLauncherWrapper")
        );
        assert_eq!(main_class("Main-Class:\n"), None);
        assert_eq!(main_class("Created-By: x\n"), None);
    }
}
