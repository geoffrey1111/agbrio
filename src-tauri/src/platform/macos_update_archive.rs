//! Publisher-signed Mac archive inspection. No installation or owner authority.
//! Release-internal paths are ASCII; user installation/project paths are not.
use base64ct::{Base64, Encoding};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, Read, Write},
};

#[cfg(target_os = "macos")]
pub(crate) mod native;
#[cfg(test)]
mod tests;
const APP: &str = "Agbrio.app";
const MAX_ARCHIVE: usize = 600 * 1024 * 1024;
const MAX_EXPANDED: u64 = 2 * 1024 * 1024 * 1024;
const MAX_STREAM: u64 = MAX_EXPANDED + 128 * 1024 * 1024;
const MAX_FILES: usize = 100_000;
const MAX_PREFIX: usize = 1024 * 1024;
fn invalid() -> String {
    "MAC_UPDATE_ARCHIVE_INVALID".into()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Kind {
    Directory,
    File { size: u64, sha256: String },
    Symlink { target: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Record {
    path: String,
    mode: u32,
    #[serde(flatten)]
    kind: Kind,
}
pub(crate) struct Inspected {
    version: String,
    archive_sha256: String,
    records: BTreeMap<String, Record>,
}
impl Inspected {
    pub(crate) fn version(&self) -> &str {
        &self.version
    }
    pub(crate) fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }
    pub(crate) fn file_count(&self) -> usize {
        self.records.len()
    }
}

/// The sole production constructor: fixed existing publisher key, exact bytes.
pub(crate) fn inspect_signed(
    bytes: &[u8],
    signature: &str,
    version: &str,
) -> Result<Inspected, String> {
    if bytes.is_empty()
        || bytes.len() > MAX_ARCHIVE
        || signature.len() > 8192
        || version.len() > 128
    {
        return Err(invalid());
    }
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../tauri.conf.json")).map_err(|_| invalid())?;
    let encoded = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or_else(invalid)?;
    let public = String::from_utf8(Base64::decode_vec(encoded).map_err(|_| invalid())?)
        .map_err(|_| invalid())?;
    let signature = String::from_utf8(
        Base64::decode_vec(signature.trim()).map_err(|_| "MAC_UPDATE_SIGNATURE_INVALID")?,
    )
    .map_err(|_| "MAC_UPDATE_SIGNATURE_INVALID")?;
    let key =
        minisign_verify::PublicKey::decode(&public).map_err(|_| "MAC_UPDATE_SIGNATURE_INVALID")?;
    let signature = minisign_verify::Signature::decode(&signature)
        .map_err(|_| "MAC_UPDATE_SIGNATURE_INVALID")?;
    key.verify(bytes, &signature, true)
        .map_err(|_| "MAC_UPDATE_SIGNATURE_INVALID")?;
    scan(bytes, version, |_| Ok(None))
}

fn member_path(value: &str) -> Result<String, String> {
    if !value.is_ascii()
        || value.len() > 4096
        || value
            .bytes()
            .any(|c| c < 32 || c == 127 || b"\\:".contains(&c))
    {
        return Err(invalid());
    }
    let value = value.trim_end_matches('/');
    let parts: Vec<_> = value.split('/').collect();
    if parts.first() != Some(&APP)
        || parts
            .iter()
            .any(|p| p.is_empty() || matches!(*p, "." | ".."))
    {
        return Err(invalid());
    }
    Ok(value.into())
}
fn link_path(name: &str, target: &str) -> Result<String, String> {
    if target.is_empty()
        || !target.is_ascii()
        || target.len() > 4096
        || target.starts_with('/')
        || target
            .bytes()
            .any(|c| c < 32 || c == 127 || b"\\:".contains(&c))
    {
        return Err(invalid());
    }
    let mut parts: Vec<&str> = name.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.len() <= 1 {
                    return Err(invalid());
                }
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    if parts.first() != Some(&APP) {
        return Err(invalid());
    }
    Ok(parts.join("/"))
}
fn resolve(mut path: String, records: &BTreeMap<String, Record>) -> Result<(), String> {
    let mut visited = BTreeSet::new();
    for _ in 0..100 {
        if !visited.insert(path.clone()) {
            return Err(invalid());
        }
        let parts: Vec<_> = path.split('/').collect();
        let mut replacement = None;
        for i in 1..=parts.len() {
            let prefix = parts[..i].join("/");
            if let Some(Record {
                kind: Kind::Symlink { target },
                ..
            }) = records.get(&prefix)
            {
                let mut next = link_path(&prefix, target)?;
                if i < parts.len() {
                    next.push('/');
                    next.push_str(&parts[i..].join("/"));
                }
                replacement = Some(next);
                break;
            }
        }
        match replacement {
            Some(next) => path = next,
            None => {
                return if records.contains_key(&path) {
                    Ok(())
                } else {
                    Err(invalid())
                }
            }
        }
    }
    Err(invalid())
}
fn ascii(bytes: &[u8]) -> Result<String, String> {
    if !bytes.is_ascii() {
        return Err(invalid());
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| invalid())
}

/// Raw tar iteration prevents allocation of attacker-sized GNU/PAX metadata by
/// the generic iterator. Only the producer's bounded local path/linkpath records.
fn pax(bytes: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let mut fields = BTreeMap::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let space = bytes[cursor..]
            .iter()
            .position(|c| *c == b' ')
            .ok_or_else(invalid)?
            + cursor;
        let length = ascii(&bytes[cursor..space])?
            .parse::<usize>()
            .map_err(|_| invalid())?;
        let end = cursor
            .checked_add(length)
            .filter(|n| *n <= bytes.len() && *n > space + 2)
            .ok_or_else(invalid)?;
        if bytes[end - 1] != b'\n' {
            return Err(invalid());
        }
        let body = &bytes[space + 1..end - 1];
        let eq = body.iter().position(|c| *c == b'=').ok_or_else(invalid)?;
        let key = ascii(&body[..eq])?;
        let value = ascii(&body[eq + 1..])?;
        if !matches!(key.as_str(), "path" | "linkpath") || fields.insert(key, value).is_some() {
            return Err(invalid());
        }
        cursor = end;
    }
    Ok(fields)
}
struct Limited<R> {
    inner: R,
    count: u64,
}
impl<R: Read> Read for Limited<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count = self
            .count
            .checked_add(n as u64)
            .filter(|n| *n <= MAX_STREAM)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "expanded archive limit"))?;
        Ok(n)
    }
}
struct Header {
    path: String,
    mode: u32,
    size: u64,
    kind: u8,
}

/// `open_file` is used only by the native private-directory extractor. It never
/// sees unvalidated names, links or arbitrary filesystem destinations.
fn scan(
    bytes: &[u8],
    version: &str,
    mut open_file: impl FnMut(&Header) -> Result<Option<File>, String>,
) -> Result<Inspected, String> {
    if bytes.is_empty()
        || bytes.len() > MAX_ARCHIVE
        || version.len() > 128
        || semver::Version::parse(version).is_err()
    {
        return Err(invalid());
    }
    let decoder = flate2::bufread::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(Limited {
        inner: decoder,
        count: 0,
    });
    let mut records = BTreeMap::new();
    let mut aliases = BTreeSet::new();
    let mut special = BTreeMap::new();
    let mut expanded = 0u64;
    let mut pending = None;
    let mut raw_count = 0;
    {
        let entries = archive.entries().map_err(|_| invalid())?.raw(true);
        for entry in entries {
            raw_count += 1;
            if raw_count > MAX_FILES * 2 {
                return Err(invalid());
            }
            let mut entry = entry.map_err(|_| invalid())?;
            let entry_type = entry.header().entry_type();
            if entry_type.is_pax_local_extensions() {
                if pending.is_some() || entry.size() > 16384 {
                    return Err(invalid());
                }
                let mut data = Vec::new();
                entry.read_to_end(&mut data).map_err(|_| invalid())?;
                pending = Some(pax(&data)?);
                continue;
            }
            if records.len() >= MAX_FILES {
                return Err(invalid());
            }
            let fields = pending.take().unwrap_or_default();
            let path = member_path(&match fields.get("path") {
                Some(p) => p.clone(),
                None => ascii(&entry.path_bytes())?,
            })?;
            if !aliases.insert(path.to_ascii_lowercase())
                || path.starts_with(&format!("{APP}/Contents/Resources/pi-adapter"))
            {
                return Err(invalid());
            }
            let mode = entry.header().mode().map_err(|_| invalid())?;
            if mode & !0o777 != 0
                || entry.header().uid().map_err(|_| invalid())? != 0
                || entry.header().gid().map_err(|_| invalid())? != 0
            {
                return Err(invalid());
            }
            let size = entry.size();
            let kind = if entry_type.is_dir() {
                b'd'
            } else if entry_type.is_file() {
                b'f'
            } else if entry_type.is_symlink() {
                b's'
            } else {
                return Err(invalid());
            };
            if kind != b's' && mode & 0o022 != 0 {
                return Err(invalid());
            }
            if kind == b'd' && mode & 0o500 != 0o500 {
                return Err(invalid());
            }
            if kind != b'f' && size != 0 {
                return Err(invalid());
            }
            if kind != b's' && fields.contains_key("linkpath") {
                return Err(invalid());
            }
            let target = if kind == b's' {
                match fields.get("linkpath") {
                    Some(p) => p.clone(),
                    None => ascii(entry.link_name_bytes().ok_or_else(invalid)?.as_ref())?,
                }
            } else {
                String::new()
            };
            let header = Header {
                path: path.clone(),
                mode,
                size,
                kind,
            };
            let kind = match kind {
                b'd' => Kind::Directory,
                b's' => {
                    link_path(&path, &target)?;
                    Kind::Symlink { target }
                }
                _ => {
                    expanded = expanded
                        .checked_add(size)
                        .filter(|n| *n <= MAX_EXPANDED)
                        .ok_or_else(invalid)?;
                    let capture = path == format!("{APP}/Contents/Info.plist")
                        || path == format!("{APP}/Contents/MacOS/ai-work-router")
                        || path == format!("{APP}/Contents/MacOS/browser-executor-node");
                    let mut output = open_file(&header)?;
                    let mut hash = Sha256::new();
                    let mut prefix = Vec::new();
                    let mut read = 0u64;
                    let mut chunk = [0; 65536];
                    loop {
                        let n = entry.read(&mut chunk).map_err(|_| invalid())?;
                        if n == 0 {
                            break;
                        }
                        read = read
                            .checked_add(n as u64)
                            .filter(|n| *n <= size)
                            .ok_or_else(invalid)?;
                        hash.update(&chunk[..n]);
                        if capture && prefix.len() < MAX_PREFIX {
                            prefix.extend_from_slice(&chunk[..n.min(MAX_PREFIX - prefix.len())]);
                        }
                        if let Some(file) = &mut output {
                            file.write_all(&chunk[..n])
                                .map_err(|_| "MAC_UPDATE_STAGING_FAILED")?;
                        }
                    }
                    if read != size {
                        return Err(invalid());
                    }
                    if let Some(file) = &mut output {
                        file.sync_all().map_err(|_| "MAC_UPDATE_STAGING_FAILED")?;
                    }
                    if capture {
                        if path.ends_with("Info.plist") && read > MAX_PREFIX as u64 {
                            return Err(invalid());
                        }
                        special.insert(path.clone(), prefix);
                    }
                    Kind::File {
                        size,
                        sha256: format!("{:x}", hash.finalize()),
                    }
                }
            };
            records.insert(path.clone(), Record { path, mode, kind });
        }
    }
    if pending.is_some() {
        return Err(invalid());
    }
    let mut stream = archive.into_inner();
    let mut tail = [0; 65536];
    let mut tail_len = 0usize;
    loop {
        let n = stream.read(&mut tail).map_err(|_| invalid())?;
        if n == 0 {
            break;
        }
        if tail[..n].iter().any(|b| *b != 0) {
            return Err(invalid());
        }
        tail_len += n;
    }
    if tail_len < 512 || tail_len % 512 != 0 || !stream.inner.into_inner().is_empty() {
        return Err(invalid());
    }
    if !matches!(records.get(APP).map(|r| &r.kind), Some(Kind::Directory)) {
        return Err(invalid());
    }
    for (path, record) in &records {
        let mut parent = path.as_str();
        while let Some((p, _)) = parent.rsplit_once('/') {
            if !matches!(records.get(p).map(|r| &r.kind), Some(Kind::Directory)) {
                return Err(invalid());
            }
            parent = p;
        }
        if matches!(record.kind, Kind::Symlink { .. }) {
            resolve(path.clone(), &records)?;
        }
    }
    let info = special
        .get(&format!("{APP}/Contents/Info.plist"))
        .ok_or_else(invalid)?;
    let info = plist::Value::from_reader(io::Cursor::new(info)).map_err(|_| invalid())?;
    let dict = info.as_dictionary().ok_or_else(invalid)?;
    for (key, expected) in [
        ("CFBundleIdentifier", "com.geoffrey.aiworkrouter"),
        ("CFBundleExecutable", "ai-work-router"),
        ("CFBundleVersion", version),
        ("CFBundleShortVersionString", version),
    ] {
        if dict.get(key).and_then(plist::Value::as_string) != Some(expected) {
            return Err(invalid());
        }
    }
    if !matches!(
        dict.get("LSMinimumSystemVersion")
            .and_then(plist::Value::as_string),
        Some("13.5" | "13.5.0")
    ) {
        return Err(invalid());
    }
    for filename in ["ai-work-router", "browser-executor-node"] {
        let path = format!("{APP}/Contents/MacOS/{filename}");
        if !matches!(records.get(&path),Some(Record{mode,kind:Kind::File{..},..}) if mode&0o100!=0)
        {
            return Err(invalid());
        }
        arm64(special.get(&path).ok_or_else(invalid)?)?;
    }
    Ok(Inspected {
        version: version.into(),
        archive_sha256: digest(bytes),
        records,
    })
}
fn arm64(data: &[u8]) -> Result<(), String> {
    let read = |offset: usize| -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            data.get(offset..offset + 4)
                .ok_or_else(invalid)?
                .try_into()
                .map_err(|_| invalid())?,
        ))
    };
    if data.len() < 32 || read(0)? != 0xfeedfacf || read(4)? != 0x0100000c {
        return Err(invalid());
    }
    let count = read(16)? as usize;
    let size = read(20)? as usize;
    let end = 32usize
        .checked_add(size)
        .filter(|n| *n <= data.len())
        .ok_or_else(invalid)?;
    if count > 65536 {
        return Err(invalid());
    }
    let mut cursor = 32;
    let mut minimum = None;
    for _ in 0..count {
        if cursor + 8 > end {
            return Err(invalid());
        }
        let command = read(cursor)?;
        let length = read(cursor + 4)? as usize;
        if length < 8 || cursor.checked_add(length).filter(|n| *n <= end).is_none() {
            return Err(invalid());
        }
        let value = if command == 0x32 {
            if length < 24 || read(cursor + 8)? != 1 {
                return Err(invalid());
            }
            Some(read(cursor + 12)?)
        } else if command == 0x24 {
            if length < 16 {
                return Err(invalid());
            }
            Some(read(cursor + 8)?)
        } else {
            None
        };
        if let Some(value) = value {
            minimum = Some(minimum.unwrap_or(0).max(value));
        }
        cursor += length;
    }
    if cursor != end || minimum.is_none_or(|v| v > 0x000d0500) {
        return Err(invalid());
    }
    Ok(())
}
