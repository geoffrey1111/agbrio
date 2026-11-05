use super::*;
use std::{io::Cursor, path::PathBuf};

pub(super) const VERSION: &str = "0.1.1";
pub(super) struct Member {
    pub path: String,
    pub kind: u8,
    pub data: Vec<u8>,
    pub target: String,
    pub mode: u32,
}
pub(super) fn member(path: &str, kind: u8, data: &[u8]) -> Member {
    Member {
        path: path.into(),
        kind,
        data: data.into(),
        target: String::new(),
        mode: if kind == b'd' { 0o755 } else { 0o644 },
    }
}
pub(super) fn link(path: &str, target: &str) -> Member {
    Member {
        path: path.into(),
        kind: b's',
        data: vec![],
        target: target.into(),
        mode: 0o777,
    }
}
pub(super) fn macho(cpu: u32, minimum: u32) -> Vec<u8> {
    [
        0xfeedfacf, cpu, 0, 2, 1, 24, 0, 0, 0x32, 24, 1, minimum, 0x001a0000, 0,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect()
}
pub(super) fn fixture() -> Vec<Member> {
    let mut dict = plist::Dictionary::new();
    for (key, value) in [
        ("CFBundleIdentifier", "com.geoffrey.aiworkrouter"),
        ("CFBundleExecutable", "ai-work-router"),
        ("CFBundleVersion", VERSION),
        ("CFBundleShortVersionString", VERSION),
        ("LSMinimumSystemVersion", "13.5"),
    ] {
        dict.insert(key.into(), plist::Value::String(value.into()));
    }
    let mut info = Vec::new();
    plist::Value::Dictionary(dict)
        .to_writer_xml(&mut info)
        .unwrap();
    let mut rows = vec![
        member(APP, b'd', b""),
        member("Agbrio.app/Contents", b'd', b""),
        member("Agbrio.app/Contents/MacOS", b'd', b""),
        member("Agbrio.app/Contents/Info.plist", b'f', &info),
    ];
    for name in ["ai-work-router", "browser-executor-node"] {
        let mut row = member(
            &format!("{APP}/Contents/MacOS/{name}"),
            b'f',
            &macho(0x0100000c, 0x000d0500),
        );
        row.mode = 0o755;
        rows.push(row);
    }
    rows
}
fn header(row: &Member) -> tar::Header {
    let mut h = tar::Header::new_ustar();
    h.set_mode(row.mode);
    h.set_uid(0);
    h.set_gid(0);
    h.set_mtime(0);
    h.set_entry_type(tar::EntryType::new(match row.kind {
        b'd' => b'5',
        b's' => b'2',
        b'f' => b'0',
        other => other,
    }));
    h.set_size(if row.kind == b'd' || row.kind == b's' {
        0
    } else {
        row.data.len() as u64
    });
    assert!(row.path.len() <= 100 && row.target.len() <= 100);
    h.as_mut_bytes()[..100].fill(0);
    h.as_mut_bytes()[..row.path.len()].copy_from_slice(row.path.as_bytes());
    h.as_mut_bytes()[157..257].fill(0);
    h.as_mut_bytes()[157..157 + row.target.len()].copy_from_slice(row.target.as_bytes());
    h.set_cksum();
    h
}
pub(super) fn gzip(raw: &[u8]) -> Vec<u8> {
    let mut w = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    w.write_all(raw).unwrap();
    w.finish().unwrap()
}
pub(super) fn archive(rows: &[Member]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for row in rows {
        tar.append(&header(row), Cursor::new(&row.data)).unwrap();
    }
    tar.finish().unwrap();
    gzip(&tar.into_inner().unwrap())
}
pub(super) fn unsigned(bytes: &[u8]) -> Result<Inspected, String> {
    scan(bytes, VERSION, |_| Ok(None))
}
fn pax_record(key: &str, value: &str) -> Vec<u8> {
    let body = format!(" {key}={value}\n");
    let mut n = body.len() + 1;
    loop {
        let next = body.len() + n.to_string().len();
        if next == n {
            break;
        }
        n = next;
    }
    format!("{n}{body}").into_bytes()
}

#[test]
fn real_gzip_tar_has_closed_file_records_and_expected_bundle() {
    let bytes = archive(&fixture());
    let checked = unsigned(&bytes).unwrap();
    assert_eq!(checked.file_count(), 6);
    assert_eq!(checked.version(), VERSION);
    assert_eq!(checked.archive_sha256(), digest(&bytes));
    assert!(matches!(
        &checked.records["Agbrio.app/Contents/MacOS/ai-work-router"].kind,
        Kind::File { size: 56, .. }
    ));
}
#[test]
fn pinned_signature_gate_rejects_untrusted_bytes_before_parsing() {
    let bytes = archive(&fixture());
    for signature in ["", "not-base64", "AA=="] {
        assert!(inspect_signed(&bytes, signature, VERSION).is_err());
    }
    assert!(inspect_signed(&[], "", VERSION).is_err());
}
#[test]
fn traversal_aliases_non_ascii_and_pi_are_rejected() {
    for path in [
        "../outside",
        "/Agbrio.app/other",
        "Agbrio.app/../other",
        "Agbrio.app//other",
        "Agbrio.app/C:/other",
        "Agbrio.app/中文",
        "Agbrio.app/contents/INFO.plist",
        "Agbrio.app/Contents/Info.plist",
        "Agbrio.app/Contents/Resources/pi-adapter",
    ] {
        let mut rows = fixture();
        rows.push(member(path, b'f', b"bad"));
        assert!(unsigned(&archive(&rows)).is_err(), "{path}");
    }
}
#[test]
fn missing_parent_and_privileged_or_writable_modes_are_rejected() {
    let mut rows = fixture();
    rows.push(member("Agbrio.app/missing/file", b'f', b"bad"));
    assert!(unsigned(&archive(&rows)).is_err());
    for mode in [0o4755, 0o2755, 0o1777, 0o777, 0o775, 0o757] {
        let mut rows = fixture();
        rows[0].mode = mode;
        assert!(unsigned(&archive(&rows)).is_err(), "{mode:o}");
    }
    let mut rows = fixture();
    rows[4].mode = 0o644;
    assert!(unsigned(&archive(&rows)).is_err());
}
#[test]
fn hard_links_devices_and_gnu_metadata_are_rejected_without_expansion() {
    for kind in [b'1', b'3', b'4', b'6', b'S', b'L', b'K', b'g'] {
        let mut rows = fixture();
        rows.push(member("Agbrio.app/special", kind, b""));
        assert!(unsigned(&archive(&rows)).is_err(), "{kind}");
    }
}
#[test]
fn relative_framework_links_resolve_intermediate_aliases() {
    let mut rows = fixture();
    for path in [
        "Agbrio.app/Contents/Frameworks",
        "Agbrio.app/Contents/Frameworks/F.framework",
        "Agbrio.app/Contents/Frameworks/F.framework/Versions",
        "Agbrio.app/Contents/Frameworks/F.framework/Versions/A",
        "Agbrio.app/Contents/Frameworks/F.framework/Versions/A/Resources",
    ] {
        rows.push(member(path, b'd', b""));
    }
    rows.push(link(
        "Agbrio.app/Contents/Frameworks/F.framework/Versions/Current",
        "A",
    ));
    rows.push(link(
        "Agbrio.app/Contents/Frameworks/F.framework/Resources",
        "Versions/Current/Resources",
    ));
    assert!(unsigned(&archive(&rows)).is_ok());
}
#[test]
fn escaping_dangling_cyclic_and_parent_links_fail() {
    for extra in [
        vec![link("Agbrio.app/link", "../../outside")],
        vec![link("Agbrio.app/link", "missing")],
        vec![link("Agbrio.app/a", "b"), link("Agbrio.app/b", "a")],
        vec![
            link("Agbrio.app/alias", "Contents"),
            member("Agbrio.app/alias/data", b'f', b"bad"),
        ],
        vec![link("Agbrio.app/link", "目标")],
    ] {
        let mut rows = fixture();
        rows.extend(extra);
        assert!(unsigned(&archive(&rows)).is_err());
    }
}
#[test]
fn bounded_pax_paths_are_applied_and_other_overrides_fail() {
    let mut rows = fixture();
    let long = format!("Agbrio.app/{}", "a".repeat(150));
    rows.push(member("pax", b'x', &pax_record("path", &long)));
    rows.push(member("placeholder", b'f', b"safe"));
    assert!(unsigned(&archive(&rows))
        .unwrap()
        .records
        .contains_key(&long));
    for payload in [
        pax_record("size", "9999999999999"),
        pax_record("path", "../escape"),
        vec![b'a'; 16385],
        b"99 path=bad\n".to_vec(),
    ] {
        let mut rows = fixture();
        rows.push(member("pax", b'x', &payload));
        rows.push(member("placeholder", b'f', b"bad"));
        assert!(unsigned(&archive(&rows)).is_err());
    }
    let mut rows = fixture();
    rows.push(member("pax", b'x', &pax_record("path", "Agbrio.app/last")));
    assert!(unsigned(&archive(&rows)).is_err());
}
#[test]
fn wrong_bundle_version_architecture_and_minimum_os_fail() {
    let bytes = archive(&fixture());
    assert!(scan(&bytes, "0.1.2", |_| Ok(None)).is_err());
    for binary in [
        macho(0x01000007, 0x000d0500),
        macho(0x0100000c, 0x001a0000),
        vec![0; 32],
    ] {
        let mut rows = fixture();
        rows[4].data = binary;
        assert!(unsigned(&archive(&rows)).is_err());
    }
}
#[test]
fn truncation_crc_and_trailing_gzip_members_are_rejected() {
    let bytes = archive(&fixture());
    assert!(unsigned(&bytes[..bytes.len() - 4]).is_err());
    let mut changed = bytes.clone();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    assert!(unsigned(&changed).is_err());
    let mut extra = bytes.clone();
    extra.extend_from_slice(&gzip(&[0; 1024]));
    assert!(unsigned(&extra).is_err());
    let mut junk = bytes;
    junk.push(1);
    assert!(unsigned(&junk).is_err());
}
#[test]
fn size_and_stream_overflow_are_bounded_before_large_allocation() {
    let mut h = header(&member("Agbrio.app/large", b'f', b""));
    h.set_size(MAX_EXPANDED + 1);
    h.set_cksum();
    let mut raw = h.as_bytes().to_vec();
    raw.extend_from_slice(&[0; 1024]);
    assert!(unsigned(&gzip(&raw)).is_err());
    let mut reader = Limited {
        inner: Cursor::new(vec![0; 4]),
        count: MAX_STREAM - 2,
    };
    assert!(reader.read(&mut [0; 4]).is_err());
}

#[test]
#[ignore = "Explicit publisher-local signed Mac archive inspection; never installs or uses private credentials"]
fn actual_signed_mac_archive_is_inspected() {
    let path = PathBuf::from(
        std::env::var_os("AGBRIO_MAC_SIGNED_ARCHIVE").expect("explicit signed archive"),
    );
    assert!(!std::fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_symlink());
    let bytes = std::fs::read(&path).unwrap();
    let signature = std::fs::read_to_string(format!("{}.sig", path.display())).unwrap();
    let expected =
        std::env::var("AGBRIO_MAC_EXPECTED_ARCHIVE_SHA256").expect("exact previously signed hash");
    let version =
        std::env::var("AGBRIO_MAC_EXPECTED_VERSION").expect("explicit signed archive version");
    assert_eq!(
        path.file_name().unwrap().to_str().unwrap(),
        format!("Agbrio_{version}_aarch64.app.tar.gz")
    );
    assert_eq!(digest(&bytes), expected);
    let checked = inspect_signed(&bytes, &signature, &version).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path.parent().unwrap().join("MACOS_UPDATE_MANIFEST.json")).unwrap(),
    )
    .unwrap();
    let records: Vec<_> = checked.records.values().collect();
    assert_eq!(serde_json::to_value(records).unwrap(), manifest["files"]);
    let mut bad = bytes;
    let last = bad.len() - 1;
    bad[last] ^= 1;
    assert!(inspect_signed(&bad, &signature, &version).is_err());
    println!("\nMACOS_ARCHIVE_INSPECTION_PASS:{expected}");
}
